//! The Workshop Manual for a VIN (the Electron app's `manual.js` steps 3 and 4, and `manual/viewer/contents.js`,
//! `vehicle-filter.js`, `procedure.js`):
//!
//! 3. the contents: Ford's "tree and cover" page (asked as Ford's own manual page asks), kept on disk for a week
//!    (a new manual release gets a new file), read into folders and procedures ([`Toc`]); Ford marks which options
//!    each is for, and [`Fit`] works out which apply to this vehicle, as Ford's page does ("only this vehicle")
//! 4. one procedure: from Ford's content server, read into a document ([`ProcPage`])

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use crate::doc::{self, Block, ProcCtx, ProcLink, split_target};
use crate::html::{Doc, El, Piece, clean};
use crate::vehicle::{Book, enc};
use crate::{Error, Ford, Result};
use workbench_net::{Http, Method};

/// One entry of the contents
#[derive(Debug, Clone, PartialEq)]
pub struct TocNode {
    pub title: String,
    pub kind: TocKind,
    /// Doesn't apply to this vehicle (by Ford's marks)
    pub na: bool,
    /// A folder's entries; a procedure's parts listed under it
    pub children: Vec<TocNode>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TocKind {
    Folder,
    Doc(ProcLink),
    /// A document elsewhere (a PDF…): its address
    Link(String),
}

/// The contents, read: everything (with what doesn't apply to this vehicle marked), and whether Ford marked it
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Toc {
    pub roots: Vec<TocNode>,
    /// Ford marked which options entries are for, and the vehicle's options are known
    pub filterable: bool,
}

/// A procedure, read
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ProcPage {
    /// "303-01A Engine - 2.7L EcoBoost · Removal and Installation"
    pub section: String,
    pub title: String,
    /// Ford's revision line
    pub rev: String,
    pub blocks: Vec<Block>,
}

// ---------- only this vehicle ----------

/// Which of the book's option codes this vehicle has (from PTS's VIN decode), and when it was built
#[derive(Debug, Clone, Default)]
pub struct Fit {
    set: HashSet<String>,
    mfcs: HashSet<String>,
    minor_known: bool,
    build: u32,
}

fn codes(v: &str) -> Vec<String> {
    v.to_uppercase()
        .split(|c: char| c == '|' || c == ',' || c.is_whitespace())
        .filter(|c| !c.is_empty())
        .map(str::to_string)
        .collect()
}

/// Whether Ford's contents can be narrowed to this vehicle (`primary`, `minor`: its option codes; `build`: its build
/// date, digits), as Ford's own manual page works it out; none when they can't (and nothing is hidden)
pub fn fit(d: &Doc, primary: &[String], minor: &[String], build: &str) -> Option<Fit> {
    if primary.is_empty() && minor.is_empty() {
        return None;
    }
    let marked = d.root().find_all(|e| e.attr("data-qual").is_some());
    if marked.is_empty() {
        return None;
    }
    let holder = d
        .root()
        .find(|e| e.attr("data-feature-vl").is_some() || e.attr("data-feature-pfcs").is_some());
    let mut pfcs = holder.and_then(|h| h.attr("data-feature-pfcs")).map(codes).unwrap_or_default();
    let mut mfcs = holder.and_then(|h| h.attr("data-feature-mfcs")).map(codes).unwrap_or_default();
    if pfcs.is_empty() && mfcs.is_empty() {
        // the book's list missing: every code its marks name
        let mut used: Vec<String> = Vec::new();
        for n in &marked {
            for g in n.attr("data-qual").unwrap_or("").split('#') {
                if let Some(rest) = g.strip_prefix("FC~").or_else(|| g.strip_prefix("fc~")) {
                    for c in codes(&rest.replace('|', " ")) {
                        if !used.contains(&c) {
                            used.push(c);
                        }
                    }
                }
            }
        }
        pfcs = used.iter().filter(|c| c.len() <= 4).cloned().collect();
        mfcs = used.iter().filter(|c| c.len() > 4).cloned().collect();
    }
    let primary: HashSet<&str> = primary.iter().map(String::as_str).collect();
    let minor: HashSet<String> = minor.iter().map(|m| m.chars().take(5).collect()).collect();
    let set: HashSet<String> = pfcs
        .iter()
        .filter(|c| primary.contains(c.as_str()))
        .chain(mfcs.iter().filter(|c| minor.contains(c.as_str())))
        .cloned()
        .collect();
    // none of the book's main options among the vehicle's: the codes don't line up, so nothing is hidden
    if !pfcs.iter().any(|c| set.contains(c)) {
        return None;
    }
    Some(Fit {
        set,
        mfcs: mfcs.into_iter().collect(),
        minor_known: !minor.is_empty(),
        build: build.chars().filter(char::is_ascii_digit).collect::<String>().parse().unwrap_or(0),
    })
}

/// Whether Ford's mark ("#FC~ene3|enr9#SD~20220101") fits: every "#" group must; in an FC group any one option code
/// will do; SD / ED: built on or after / on or before
pub fn qual_fits(qual: Option<&str>, fit: Option<&Fit>) -> bool {
    let (Some(qual), Some(fit)) = (qual, fit) else { return true };
    qual.split('#').skip(1).filter(|g| !g.is_empty()).all(|g| {
        let g = g.to_uppercase();
        let (kind, rest) = g.split_once('~').unwrap_or((g.as_str(), ""));
        let quals: Vec<&str> = rest.split('|').collect();
        match kind {
            "FC" => quals
                .iter()
                .any(|q| fit.set.contains(*q) || (!fit.minor_known && fit.mfcs.contains(*q))),
            "SD" => fit.build == 0 || quals[0].parse::<u32>().is_ok_and(|d| d <= fit.build),
            "ED" => fit.build == 0 || quals[0].parse::<u32>().is_ok_and(|d| d >= fit.build),
            _ => true,
        }
    })
}

// ---------- the contents ----------

/// A folder's name: its text before its first list or link (Ford sometimes wraps the name and the list together)
fn folder_title(li: El) -> String {
    let mut out = String::new();
    fn walk(e: El, out: &mut String) -> bool {
        for p in e.pieces() {
            match p {
                Piece::Text(t) => out.push_str(t),
                Piece::El(c) if matches!(c.tag(), "ul" | "ol" | "a") => {
                    if !out.trim().is_empty() {
                        return true;
                    }
                }
                Piece::El(c) => {
                    if walk(c, out) {
                        return true;
                    }
                }
            }
        }
        false
    }
    walk(li, &mut out);
    clean(&out)
}

struct Arena {
    title: String,
    kind: TocKind,
    na: bool,
    children: Vec<usize>,
}

/// Reads Ford's contents. `fit`: what applies to this vehicle (entries that don't are marked).
pub fn read_toc(html: &str, primary: &[String], minor: &[String], build: &str) -> Toc {
    let d = Doc::parse(html);
    let fit = fit(&d, primary, minor, build);
    let fit_ref = fit.as_ref();
    let spa = |e: &El| e.attr("data-spa-file").map(|v| v.eq_ignore_ascii_case("y"));
    // PC/ED books list their charts twice (current "Y", old "N"): only the current
    let has_spa = d.root().find(|e| e.tag() == "li" && spa(e) == Some(true)).is_some();
    fn li_of<'a>(e: &El<'a>) -> Option<El<'a>> {
        e.parent().and_then(|p| p.closest(|x| x.tag() == "li"))
    }
    fn own_li<'a>(a: &El<'a>) -> Option<El<'a>> {
        a.closest(|x| x.tag() == "li")
    }
    let anchors: Vec<El> = d
        .root()
        .find_all(|a| {
            let f = a.attr("data-for").unwrap_or("");
            a.tag() == "a"
                && a.attr("data-for").is_some()
                && !a.hidden()
                && !f.to_ascii_lowercase().contains("adttestindex")
                && !f.to_ascii_lowercase().contains("spaindex_test")
                && !a.text().is_empty()
        })
        .into_iter()
        .filter(|a| !(has_spa && a.closest(|x| x.tag() == "li" && spa(x) == Some(false)).is_some()))
        .collect();
    // a list item's own link (not one in a list inside it)
    fn own_link<'a>(li: &El<'a>) -> Option<El<'a>> {
        li.find(|a| a.tag() == "a" && a.attr("data-for").is_some() && own_li(a).is_some_and(|l| l == *li))
    }

    let mut arena: Vec<Arena> = Vec::new();
    let mut roots: Vec<usize> = Vec::new();
    let mut folders: HashMap<usize, usize> = HashMap::new(); // <li> → folder
    let mut doc_nodes: HashMap<usize, usize> = HashMap::new(); // <li> → the procedure it is
    let mut whole: HashMap<String, usize> = HashMap::new(); // procedure number → its entry (for its parts)
    let mut seen: HashSet<String> = HashSet::new();
    for a in anchors {
        let f = a.attr("data-for").unwrap_or("").trim().to_string();
        let key = format!("{f}|{}", a.attr("data-procuid").unwrap_or(""));
        if own_li(&a).is_none() && seen.contains(&key) {
            continue; // the same link again (the manual's cover page)
        }
        seen.insert(key);
        let (sn_part, hash_part) = split_target(&f);
        let parent = hash_part
            .to_ascii_lowercase()
            .starts_with("procsub")
            .then(|| whole.get(&sn_part).copied())
            .flatten();
        // where it goes: under the procedure it's a part of, or down its folders
        let mut list: Option<usize> = parent; // None: the roots
        if parent.is_none() {
            let mut chain = Vec::new();
            let mut li = own_li(&a).and_then(|l| li_of(&l));
            while let Some(l) = li {
                chain.push(l);
                li = li_of(&l);
            }
            chain.reverse();
            for li in chain {
                if let Some(&p) = doc_nodes.get(&li.key()) {
                    list = Some(p);
                    continue;
                }
                let fid = match folders.get(&li.key()) {
                    Some(&f) => f,
                    None => {
                        let title = Some(folder_title(li))
                            .filter(|t| !t.is_empty())
                            .or_else(|| own_link(&li).map(|l| l.text()))
                            .unwrap_or_else(|| "Section".into());
                        arena.push(Arena {
                            title,
                            kind: TocKind::Folder,
                            na: false,
                            children: Vec::new(),
                        });
                        let id = arena.len() - 1;
                        match list {
                            Some(p) => arena[p].children.push(id),
                            None => roots.push(id),
                        }
                        folders.insert(li.key(), id);
                        id
                    }
                };
                list = Some(fid);
            }
        }
        // for this vehicle? (its own options, and those of every section it's in)
        let mut na = !qual_fits(a.attr("data-qual"), fit_ref);
        let mut li = own_li(&a);
        while let (Some(l), false) = (li, na) {
            let sec = l
                .children()
                .find(|c| (c.tag() == "span" && c.attr("data-qual").is_some()) || c.attr("data-qual-source") == Some("section"));
            if sec.is_some_and(|s| !qual_fits(s.attr("data-qual"), fit_ref)) {
                na = true;
            }
            li = li_of(&l);
        }
        let title = a.text();
        let class = a.attr("class").unwrap_or("").to_ascii_lowercase();
        let kind = if class.contains("quicklinkpdf-link") || f.starts_with('/') || f.to_ascii_lowercase().starts_with("http") {
            TocKind::Link(f.clone())
        } else {
            let mut sn = sn_part.clone();
            if a.parent().and_then(|p| spa(&p)) == Some(true) && !sn.to_ascii_lowercase().starts_with("spapced_") {
                sn = format!("spapced_{sn}");
            }
            TocKind::Doc(ProcLink {
                search_number: sn,
                proc_uid: a.attr("data-procuid").unwrap_or("").to_string(),
                link_type: if class.contains("publication-content") {
                    "PublicationContent"
                } else {
                    "Procedure"
                }
                .into(),
                hash: hash_part.clone(),
                ppt: String::new(),
            })
        };
        let is_doc = matches!(kind, TocKind::Doc(_));
        arena.push(Arena {
            title,
            kind,
            na,
            children: Vec::new(),
        });
        let id = arena.len() - 1;
        match list {
            Some(p) => arena[p].children.push(id),
            None => roots.push(id),
        }
        if is_doc {
            if hash_part.is_empty() && !whole.contains_key(&sn_part) {
                whole.insert(sn_part.clone(), id);
            }
            if let Some(l) = own_li(&a) {
                if own_link(&l).is_some_and(|o| o == a) && !doc_nodes.contains_key(&l.key()) {
                    doc_nodes.insert(l.key(), id);
                }
            }
        }
    }
    fn make(arena: &[Arena], id: usize) -> TocNode {
        let a = &arena[id];
        TocNode {
            title: a.title.clone(),
            kind: a.kind.clone(),
            na: a.na,
            children: a.children.iter().map(|&c| make(arena, c)).collect(),
        }
    }
    Toc {
        roots: roots.iter().map(|&r| make(&arena, r)).collect(),
        filterable: fit.is_some(),
    }
}

/// The contents as shown: this vehicle's only (unless `all`); folders left empty go, a folder all of whose entries
/// don't apply is marked too
pub fn shown(nodes: &[TocNode], all: bool) -> Vec<TocNode> {
    let mut out = Vec::new();
    for n in nodes {
        let children = shown(&n.children, all);
        match n.kind {
            TocKind::Folder => {
                if children.is_empty() {
                    continue;
                }
                let na = n.na || children.iter().all(|c| c.na);
                out.push(TocNode {
                    title: n.title.clone(),
                    kind: n.kind.clone(),
                    na,
                    children,
                });
            }
            _ => {
                if all || !n.na || !children.is_empty() {
                    out.push(TocNode {
                        title: n.title.clone(),
                        kind: n.kind.clone(),
                        na: n.na,
                        children,
                    });
                }
            }
        }
    }
    out
}

/// How many procedures (and links) in these entries
pub fn count(nodes: &[TocNode]) -> usize {
    nodes
        .iter()
        .map(|n| if matches!(n.kind, TocKind::Folder) { 0 } else { 1 } + count(&n.children))
        .sum()
}

// ---------- a procedure ----------

/// Reads a procedure from Ford's content server
pub fn read_proc(html: &str, base: &str, link: &ProcLink) -> ProcPage {
    let d = Doc::parse(html);
    let root = d.by_id("contentDiv").unwrap_or(d.root());
    let meta = |name: &str| {
        root.find(|e| e.tag() == "meta" && e.attr("name") == Some(name))
            .and_then(|m| m.attr("content"))
            .unwrap_or("")
            .trim()
            .to_string()
    };
    let title = meta("tps_proctitle");
    let section = [
        [meta("tps_sectionnumber"), meta("tps_section_title")]
            .into_iter()
            .filter(|x| !x.is_empty())
            .collect::<Vec<_>>()
            .join(" "),
        meta("tps_ss_title"),
    ]
    .into_iter()
    .filter(|x| !x.is_empty())
    .collect::<Vec<_>>()
    .join(" · ");
    let rev = root
        .find(|e| e.tag() == "a" && e.has_class("bookinfosmall"))
        .map(|e| e.text())
        .unwrap_or_default();
    let ctx = ProcCtx {
        search_number: link.search_number.clone(),
        proc_uid: link.proc_uid.clone(),
    };
    let mut blocks = doc::read_procedure(root, base, &ctx);
    // the procedure's title again as its first heading: shown once, above
    if let Some(Block::Heading(_, inl)) = blocks.first() {
        let words: String = inl
            .iter()
            .map(|i| match i {
                doc::Inline::Text { text, .. } | doc::Inline::Link { text, .. } => text.as_str(),
                doc::Inline::Break => " ",
            })
            .collect();
        if !title.is_empty() && clean(&words) == title {
            blocks.remove(0);
        }
    }
    ProcPage {
        section,
        title,
        rev,
        blocks,
    }
}

/// Form-style encoding (as URLSearchParams writes it: spaces as +)
fn form(pairs: &[(&str, String)]) -> String {
    pairs
        .iter()
        .map(|(k, v)| format!("{k}={}", enc(v).replace("%20", "+")))
        .collect::<Vec<_>>()
        .join("&")
}

impl Ford {
    /// Keeps the manual's contents on disk in `dir` (a week each)
    pub fn with_cache(mut self, dir: PathBuf) -> Self {
        self.cache = Some(dir);
        self
    }

    /// The Workshop Manual's contents for a VIN: from disk when they were kept in the last week, else from Ford.
    /// What doesn't apply to this vehicle is marked.
    pub async fn toc(&self, vin: &str) -> Result<Toc> {
        let book = self.book(vin).await?;
        let html = self.toc_html(&book).await?;
        let v = &book.vehicle;
        let build: String = v.build_date.chars().filter(char::is_ascii_digit).collect();
        let toc = read_toc(&html, &v.features, &v.minor_features, &build);
        self.log(&format!(
            "   contents: {} entries{}",
            count(&toc.roots),
            if toc.filterable { ", narrowed to this vehicle" } else { "" }
        ));
        if toc.roots.is_empty() {
            return Err(Error::Format("Ford sent the manual's contents back empty".into()));
        }
        Ok(toc)
    }

    async fn toc_html(&self, book: &Book) -> Result<String> {
        let p = |k: &str| book.param(k).to_string();
        let key: String = format!("{}_{}_{}_q_{}", p("vehicleId"), p("bookcode"), p("environment"), book.vin)
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'))
            .collect();
        let file = self.cache.as_ref().map(|d| d.join(format!("tree_{key}.html")));
        if let Some(f) = &file {
            let fresh = std::fs::metadata(f)
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.elapsed().ok())
                .is_some_and(|age| age.as_secs() < 7 * 24 * 3600);
            if fresh {
                if let Ok(h) = std::fs::read_to_string(f) {
                    return Ok(h);
                }
            }
        }
        let filterable = (p("marketGroup") == "EU" || p("modelYear").parse::<u32>().is_ok_and(|y| y >= 2019))
            && p("contenttype") != "pced"
            && p("booktype") == "ody";
        let base = book.bases.get("publicationBookTreeAndCoverBase").cloned().unwrap_or_default();
        let url = format!(
            "{base}?bookTitle={}&WiringBookTitle={}",
            enc(&p("bookTitle")),
            enc(&p("WiringBookTitle"))
        );
        let url = doc::resolve(crate::paths::PTS, &url).ok_or_else(|| Error::Format("the contents' address".into()))?;
        let u = workbench_net::Url::parse(&url).map_err(|_| Error::Format("the contents' address".into()))?;
        let or = |k: &str, d: &str| if p(k).is_empty() { d.to_string() } else { p(k) };
        let body = form(&[
            ("isMobile", or("isMobile", "no")),
            ("vin", or("vin", &book.vin)),
            ("vehicleId", p("vehicleId")),
            (
                "modelYear",
                p("modelYear").parse::<u32>().map(|y| y.to_string()).unwrap_or_default(),
            ),
            ("searchNumber", "0".into()),
            ("channel", p("channelId")),
            ("category", p("categoryId")),
            ("CategoryDescription", enc(&p("categorydesc"))),
            ("book", p("bookcode")),
            ("booktype", p("booktype")),
            ("country", p("country")),
            ("language", p("language")),
            ("contentmarket", p("contentmarket")),
            ("contentlanguage", p("contentlang")),
            ("languageOdysseyCode", p("languageOdysseyCode")),
            ("contentgroup", p("contentgroup")),
            ("WiringBookCode", p("WiringBookCode")),
            ("WiringFormat", p("WiringFormat")),
            ("fromPageBase", or("origin", crate::paths::PTS)),
            (
                "strVehLine",
                if filterable {
                    book.vehicle.vehicle_line.clone()
                } else {
                    String::new()
                },
            ),
            (
                "strProdType",
                if filterable {
                    book.vehicle.product_type.clone()
                } else {
                    String::new()
                },
            ),
        ]);
        let req = Http::request(Method::POST, u)
            .header("Content-Type", "application/x-www-form-urlencoded; charset=UTF-8")
            .header("Accept", "text/html, */*; q=0.01")
            .header("X-Requested-With", "XMLHttpRequest")
            .header("Referer", book.url.as_str())
            .body(body);
        let res = self.fetch(req).await?;
        let html = res.body;
        if !(html.contains("class=\"tree\"") || html.contains("procedure-link") || html.contains("data-for")) {
            self.log(&format!("   contents: unexpected page: {}", crate::error_page_says(&html)));
            return Err(Error::Format("Ford sent back something other than the manual's contents".into()));
        }
        if let (Some(f), Some(dir)) = (&file, &self.cache) {
            let _ = std::fs::create_dir_all(dir);
            // older copies for this VIN go
            if let Ok(list) = std::fs::read_dir(dir) {
                for e in list.flatten() {
                    let name = e.file_name().to_string_lossy().to_string();
                    if name.starts_with("tree_") && name.ends_with(&format!("_{}.html", book.vin)) && e.path() != *f {
                        let _ = std::fs::remove_file(e.path());
                    }
                }
            }
            let _ = std::fs::write(f, &html);
        }
        Ok(html)
    }

    /// Where procedures come from (Ford's content server: PTS's procedure page is only a shell that asks it)
    fn proc_content_url(book: &Book) -> String {
        let env = enc(book.param("environment"));
        let cpr = book.param("cprBasePath");
        if cpr.contains("{0}") {
            return cpr.replace("{0}", &format!("/publication/Proc?environment={env}"));
        }
        let tree = book.bases.get("publicationBookTreeAndCoverBase").map(String::as_str).unwrap_or("");
        match tree.find("/publication/") {
            Some(i) if i > 0 => format!("{}/publication/Proc?environment={env}", &tree[..i]),
            _ => {
                format!("https://www.fordservicecontent.com/Ford_Content/PublicationRuntimeRefreshPTS//publication/Proc?environment={env}")
            }
        }
    }

    /// PTS's own address for a procedure (sent as where the request comes from, as Ford's page does)
    fn proc_page_url(book: &Book, link: &ProcLink) -> String {
        let p = |k: &str| book.param(k).to_string();
        let or = |k: &str, d: &str| if p(k).is_empty() { d.to_string() } else { p(k) };
        let q = [
            ("linkType", link.link_type.clone()),
            ("isMobile", or("isMobile", "no")),
            ("userType", or("userType", "Dealer")),
            ("vehicleId", p("vehicleId")),
            ("modelYear", p("modelYear")),
            ("searchNumber", link.search_number.clone()),
            ("procUid", link.proc_uid.clone()),
            ("bookCode", p("bookcode")),
            ("bookType", p("booktype")),
            ("country", p("country")),
            ("language", p("language")),
        ]
        .iter()
        .map(|(k, v)| format!("{k}={}", enc(v)))
        .collect::<Vec<_>>()
        .join("&");
        format!("{}/publication/proc?{q}", crate::paths::PTS)
    }

    /// One procedure, read
    pub async fn procedure(&self, vin: &str, link: &ProcLink) -> Result<ProcPage> {
        let book = self.book(vin).await?;
        let p = |k: &str| book.param(k).to_string();
        let or = |k: &str, d: &str| if p(k).is_empty() { d.to_string() } else { p(k) };
        let url = Self::proc_content_url(&book);
        let u = workbench_net::Url::parse(&url).map_err(|_| Error::Format("the procedure's address".into()))?;
        let body = form(&[
            ("isMobile", or("isMobile", "no")),
            ("usertype", or("userType", "Dealer")),
            ("userQsPilot", p("userQsPilot")),
            ("vehicleId", p("vehicleId")),
            ("modelYear", p("modelYear")),
            ("channel", p("channelId")),
            ("category", p("categoryId")),
            ("book", p("bookcode")),
            ("bookTitle", p("bookTitle")),
            ("booktype", p("booktype")),
            ("country", p("country")),
            ("language", p("language")),
            ("contentmarket", p("contentmarket")),
            ("contentlanguage", p("contentlang")),
            ("languageOdysseyCode", p("languageOdysseyCode")),
            ("adt", p("adt")),
            ("diagTool", p("diagTool")),
            ("otx", p("otx")),
            ("searchNumber", link.search_number.clone()),
            ("procUid", link.proc_uid.clone()),
            ("adtLocation", String::new()),
            ("Vid", String::new()),
            ("byvin", "NO".into()),
            ("marketGroup", p("marketGroup")),
            ("WiringBookCode", p("WiringBookCode")),
            ("WiringBookTitle", p("WiringBookTitle")),
            ("WiringFormat", p("WiringFormat")),
            ("fromPageBase", or("origin", crate::paths::PTS)),
        ]);
        let req = Http::request(Method::POST, u)
            .header("Content-Type", "application/x-www-form-urlencoded; charset=UTF-8")
            .header("Accept", "text/html, */*; q=0.01")
            .header("X-Requested-With", "XMLHttpRequest")
            .header("Referer", Self::proc_page_url(&book, link))
            .header("Sec-Fetch-Site", "cross-site")
            .body(body);
        let res = self.fetch(req).await?;
        if res.body.trim().is_empty() {
            return Err(Error::Format("Ford sent this procedure back empty".into()));
        }
        let page = read_proc(&format!("<div id=\"contentDiv\">{}</div>", res.body), &url, link);
        self.log(&format!(
            "   procedure {}/{}: {} blocks, {} pictures",
            link.search_number,
            link.proc_uid,
            page.blocks.len(),
            Block::images(&page.blocks).len()
        ));
        Ok(page)
    }

    /// A newer pinpoint test's steps (Ford keeps them in a file of their own, `file`, fetched when it's opened): its
    /// introduction (if any) and steps
    pub async fn spa_steps(&self, vin: &str, link: &ProcLink, name: &str, file: &str) -> Result<(Vec<Block>, Vec<doc::Step>)> {
        let book = self.book(vin).await?;
        let p = |k: &str| book.param(k).to_string();
        let or = |k: &str, d: &str| if p(k).is_empty() { d.to_string() } else { p(k) };
        let url = book
            .bases
            .get("publicationProcContentBase")
            .filter(|u| !u.trim().is_empty())
            .cloned()
            .unwrap_or_else(|| Self::proc_content_url(&book));
        let u = workbench_net::Url::parse(&url).map_err(|_| Error::Format("where the test's steps come from".into()))?;
        let body = form(&[
            ("moduleName", String::new()),
            ("isMobile", or("isMobile", "no")),
            ("usertype", or("userType", "Dealer")),
            ("userQsPilot", p("userQsPilot")),
            ("vehicleId", p("vehicleId")),
            ("modelYear", p("modelYear")),
            ("channel", p("channelId")),
            ("category", p("categoryId")),
            ("book", p("bookcode")),
            ("bookTitle", p("bookTitle")),
            ("booktype", p("booktype")),
            ("country", p("country")),
            ("language", p("language")),
            ("contentmarket", p("contentmarket")),
            ("contentlanguage", p("contentlang")),
            ("languageOdysseyCode", p("languageOdysseyCode")),
            ("diagTool", p("diagTool")),
            ("otx", p("otx")),
            ("searchNumber", format!("{file}_spappt")),
            ("adtLocation", String::new()),
            ("procUid", link.proc_uid.clone()),
            ("Vid", String::new()),
            ("byVin", "NO".into()),
            ("marketGroup", p("marketGroup")),
            ("WiringBookCode", p("WiringBookCode")),
            ("WiringBookTitle", enc(&p("WiringBookTitle"))),
            ("WiringFormat", p("WiringFormat")),
            ("fromPageBase", or("origin", crate::paths::PTS)),
        ]);
        let req = Http::request(Method::POST, u)
            .header("Content-Type", "application/x-www-form-urlencoded; charset=UTF-8")
            .header("Accept", "text/html, */*; q=0.01")
            .header("X-Requested-With", "XMLHttpRequest")
            .header("Referer", Self::proc_page_url(&book, link))
            .body(body);
        let res = self.fetch(req).await?;
        let ctx = ProcCtx {
            search_number: link.search_number.clone(),
            proc_uid: link.proc_uid.clone(),
        };
        let (intro, steps) = doc::read_spa_steps(&res.body, &url, &ctx, name);
        self.log(&format!(
            "   test {name} ({file}): {} characters, {} steps",
            res.body.len(),
            steps.len()
        ));
        if steps.is_empty() && intro.is_empty() {
            return Err(Error::Format("this test came back without steps".into()));
        }
        Ok((intro, steps))
    }

    /// Tells Vehicle History about a pinpoint test, as Ford's own manual does (so warranty can see the test was run,
    /// and its results): that it was opened, or a step's answer
    pub async fn report_pinpoint(&self, vin: &str, r: &PinpointReport) -> Result<()> {
        let book = self.book(vin).await?;
        let cut = |s: &str, n: usize| clean(s).chars().take(n).collect::<String>();
        let step = r.step.as_ref().map(|s| {
            format!(
                "[{{\"id\":{},\"title\":{},\"question\":{},\"answer\":{},\"outcome\":{},\"values\":{{\"inputData\":null}},\"mode\":\"SINGLE\"}}]",
                js(&cut(&s.id, 20)),
                js(&cut(&s.title, 500)),
                js(&cut(&s.question, 500)),
                js(&cut(&s.answer, 20)),
                js(&cut(&s.outcome, 1000)),
            )
        });
        let value = match &r.step {
            Some(s) => cut(&s.title, 500),
            None => cut(&r.title, 500),
        };
        let vin_sent = if book.param("vin").is_empty() {
            book.vin.clone()
        } else {
            book.param("vin").to_string()
        };
        let body = format!(
            "{{\"eventActionpptName\":\"PPT_Letter_Title\",\"eventCustompptId\":{},\"eventCustompptValue\":{},\"eventpinpointsteps\":{},\"vin\":{},\"pptname\":{}}}",
            js(&cut(&r.proc_uid, 40)),
            js(&value),
            step.as_deref().unwrap_or("null"),
            js(&vin_sent),
            js(&cut(&r.name, 20)),
        );
        let u = workbench_net::Url::parse(&format!("{}/Events/ajaxLoggerForSPAppt", crate::paths::PTS))
            .map_err(|_| Error::Format("Vehicle History's address".into()))?;
        let link = ProcLink {
            search_number: r.search_number.clone(),
            proc_uid: r.proc_uid.clone(),
            link_type: "Procedure".into(),
            ..Default::default()
        };
        let req = Http::request(Method::POST, u)
            .header("Content-Type", "application/json")
            .header("Accept", "*/*")
            .header("X-Requested-With", "XMLHttpRequest")
            .header("Referer", Self::proc_page_url(&book, &link))
            .body(body);
        let res = self.fetch(req).await?;
        // (what's recorded, never the VIN)
        self.log(&format!(
            "   pinpoint {} {} → {}",
            cut(&r.name, 20),
            match &r.step {
                Some(s) => format!("step {} {}", cut(&s.id, 20), cut(&s.answer, 20)),
                None => "opened".into(),
            },
            cut(&res.body, 120)
        ));
        if crate::looks_like_sign_in(&res.body) {
            return Err(Error::NeedsLogin);
        }
        Ok(())
    }
}

/// What Vehicle History is told about a pinpoint test: it was opened (no step), or a step's answer
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PinpointReport {
    pub search_number: String,
    pub proc_uid: String,
    /// The test's letter ("A")
    pub name: String,
    pub title: String,
    pub step: Option<StepReport>,
}

/// A step answered: its label ("A1"), title, question, the answer ("Yes"), and what the answer said to do when it
/// ended the test
#[derive(Debug, Clone, Default, PartialEq)]
pub struct StepReport {
    pub id: String,
    pub title: String,
    pub question: String,
    pub answer: String,
    pub outcome: String,
}

/// A JSON string
fn js(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_json_strings() {
        assert_eq!(js("A1"), r#""A1""#);
        assert_eq!(js("say \"hi\"\\ok\n"), r#""say \"hi\"\\ok\n""#);
    }

    const TOC: &str = r##"<div class="tree" data-feature-pfcs="ENE3|ENR9" data-feature-mfcs="">
      <ul>
        <li>303 Engine
          <ul>
            <li><span data-qual="#FC~ENE3">303-01A Engine - 2.7L</span>
              <ul>
                <li>Removal and Installation
                  <ul>
                    <li><a class="procedure-link" data-for="G1001" data-procuid="P1">Engine Mount</a></li>
                    <li><a class="procedure-link" data-for="G1002" data-procuid="P2" data-qual="#SD~20250101">Later only</a></li>
                  </ul></li>
              </ul></li>
            <li><span data-qual="#FC~ENR9">303-01B Engine - 3.5L</span>
              <ul><li><a class="procedure-link" data-for="G2001" data-procuid="P3">Oil Pan</a></li></ul></li>
          </ul></li>
        <li><a class="procedure-link" data-for="G3001" data-procuid="P4">Quick Test</a>
          <ul><li><a class="procedure-link" data-for="G3001:procsub2" data-procuid="P4">KOEO</a></li></ul></li>
        <li><a class="Quicklinkpdf-link" data-for="/docs/x.pdf">A PDF</a></li>
      </ul></div>"##;

    #[test]
    fn reads_the_contents() {
        let t = read_toc(TOC, &["ENE3".into()], &[], "20240101");
        assert!(t.filterable);
        let engine = &t.roots[0];
        assert_eq!((engine.title.as_str(), engine.kind.clone()), ("303 Engine", TocKind::Folder));
        let a = &engine.children[0];
        assert_eq!(a.title, "303-01A Engine - 2.7L");
        let ri = &a.children[0];
        assert_eq!(ri.title, "Removal and Installation");
        assert_eq!(ri.children[0].title, "Engine Mount");
        assert!(!ri.children[0].na);
        assert!(ri.children[1].na, "built before its start date");
        let b = &engine.children[1];
        assert!(b.children[0].na, "the 3.5L's section");
        let TocKind::Doc(l) = &ri.children[0].kind else { panic!() };
        assert_eq!((l.search_number.as_str(), l.proc_uid.as_str()), ("G1001", "P1"));
        // a procedure's parts under it
        let qt = &t.roots[1];
        assert_eq!(qt.title, "Quick Test");
        assert_eq!(qt.children[0].title, "KOEO");
        let TocKind::Doc(k) = &qt.children[0].kind else { panic!() };
        assert_eq!(k.hash, "procsub2");
        assert_eq!(t.roots[2].kind, TocKind::Link("/docs/x.pdf".into()));

        // this vehicle's only: the 3.5L folder and the later-only procedure go
        let s = shown(&t.roots, false);
        assert_eq!(s[0].children.len(), 1);
        assert_eq!(s[0].children[0].children[0].children.len(), 1);
        assert_eq!(count(&t.roots), 6);
        assert_eq!(count(&s), 4);
        // the options don't line up: nothing hidden
        assert!(!read_toc(TOC, &["XXXX".into()], &[], "").filterable);
    }

    #[test]
    fn reads_a_procedure() {
        let html = r#"<div id="contentDiv"><meta name="tps_proctitle" content="Engine Mount">
          <meta name="tps_sectionnumber" content="303-01A"><meta name="tps_section_title" content="Engine - 2.7L">
          <meta name="tps_ss_title" content="Removal and Installation">
          <table><tr><td><a class="bookinfosmall">2026 F-150 Workshop Manual</a></td></tr></table>
          <h1>Engine Mount</h1>
          <p>NOTE: Use a <a class="procedure-link" data-for="G555#part2" data-procuid="P9">jack</a>.</p>
          <a href="https://x/learn.html"><img src="training.png" alt="Workshop Manual Graphics Training"></a>
          <img src="/pics/E123.gif"><img src="/img/CaretRight.gif">
          <div id="copyright">© Ford</div></div>"#;
        let link = ProcLink {
            search_number: "G1001".into(),
            proc_uid: "P1".into(),
            ..Default::default()
        };
        let p = read_proc(
            html,
            "https://www.fordservicecontent.com/Ford_Content/x/publication/Proc?environment=e",
            &link,
        );
        assert_eq!(p.title, "Engine Mount");
        assert_eq!(p.section, "303-01A Engine - 2.7L · Removal and Installation");
        assert_eq!(p.rev, "2026 F-150 Workshop Manual");
        assert!(matches!(&p.blocks[0], Block::Para(_, Some(doc::Callout::Note))));
        let Block::Para(inl, _) = &p.blocks[0] else { panic!() };
        let doc::Inline::Link { url, .. } = &inl[1] else {
            panic!("{inl:?}")
        };
        let l = ProcLink::from_url(url).unwrap();
        assert_eq!(
            (l.search_number.as_str(), l.proc_uid.as_str(), l.hash.as_str()),
            ("G555", "P9", "part2")
        );
        assert_eq!(Block::images(&p.blocks), ["https://www.fordservicecontent.com/pics/E123.gif"]);
        assert_eq!(p.blocks.len(), 2);
    }
}
