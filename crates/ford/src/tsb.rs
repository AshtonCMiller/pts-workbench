//! TSBs / GSBs / SSMs (and ISBs) for a VIN: Ford's TSB_SSM "DisplayArticles" page (as PTS shows it), read into a
//! list with Ford's categories; and one article (or an FSA's document) fetched from Ford, ready to be read as a
//! document ([`crate::doc`]), or a PDF. (The Electron app's `manual.js` TSB part and `manual/tsb/list.js`.)

use crate::doc::{self, Block, query_value, resolve};
use crate::html::{Doc, El, clean};
use crate::vehicle::enc;
use crate::{Error, Ford, Result};
use workbench_net::{Http, Method};

/// Where Ford's TSB pages are
pub const TSB_BASE: &str = "https://www.fordtechservice.dealerconnection.com/vdirsnet/TSB_SSM/";

/// One bulletin on the list
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Article {
    /// "TSB", "GSB", "SSM" or "ISB"
    pub kind: String,
    /// "26-2414"
    pub id: String,
    /// What Ford's list calls it ("TSB 26-2414")
    pub title: String,
    /// "Effective …", as Ford says it
    pub date: String,
    pub summary: String,
    /// Its page on Ford's site
    pub url: String,
    /// Ford's categories it's in (their ids)
    pub categories: Vec<String>,
}

/// One of Ford's categories, for narrowing the list
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Category {
    pub id: String,
    pub label: String,
    /// How far down Ford's menu it is (0 at the top)
    pub depth: usize,
    pub count: usize,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct TsbList {
    pub articles: Vec<Article>,
    pub categories: Vec<Category>,
}

/// A document fetched from Ford: to read, or a PDF to open
#[derive(Debug, Clone, PartialEq)]
pub enum Fetched {
    Doc { blocks: Vec<Block>, url: String },
    Pdf { url: String },
}

fn kind_of(letter: &str) -> Option<&'static str> {
    match letter {
        "T" => Some("TSB"),
        "G" => Some("GSB"),
        "S" => Some("SSM"),
        "I" => Some("ISB"),
        _ => None,
    }
}

/// Reads Ford's list page
pub fn read_list(html: &str) -> TsbList {
    let d = Doc::parse(html);
    let mut list = TsbList::default();
    let items = d.root().find_all(|e| {
        e.tag() == "li"
            && e.closest(|p| p.attr("id") == Some("articles") || (p.tag() == "ul" && p.has_class("filterThis")))
                .is_some()
    });
    for li in items {
        let Some(a) = li
            .find(|e| e.tag() == "a" && e.closest(|p| p.has_class("article")).is_some_and(|p| p.is_inside(&li) || p == li))
            .or_else(|| li.find(|e| e.tag() == "a" && e.has_class("small")))
        else {
            continue;
        };
        let href = a.attr("href").unwrap_or("");
        let after_hash = href.split_once('#').map(|(_, h)| h).unwrap_or(href);
        let target = if after_hash.to_ascii_lowercase().contains("displayarticle.aspx") {
            after_hash.to_string()
        } else {
            let onclick = a.attr("onclick").unwrap_or("");
            match onclick.to_ascii_lowercase().find("displayarticle.aspx") {
                Some(at) => onclick[at..].split(['\'', '"']).next().unwrap_or("").to_string(),
                None => continue,
            }
        };
        let Some(url) = resolve(TSB_BASE, &target) else { continue };
        let label = a.text();
        let classes: Vec<String> = li.attr("class").unwrap_or("").split_whitespace().map(str::to_string).collect();
        let letter = classes
            .iter()
            .find(|c| matches!(c.as_str(), "T" | "G" | "S" | "I"))
            .cloned()
            .or_else(|| {
                let up = label.to_uppercase();
                ["TSB", "GSB", "SSM", "ISB"]
                    .iter()
                    .find(|k| up.starts_with(*k))
                    .map(|k| k[..1].to_string())
            })
            .unwrap_or_else(|| "T".into());
        let kind = kind_of(&letter)
            .map(str::to_string)
            .or_else(|| query_value(&url, "type").map(|t| t.to_uppercase()))
            .unwrap_or_else(|| "TSB".into());
        let article_text = li.find(|e| e.has_class("article")).map(|e| e.text()).unwrap_or_default();
        let after = article_text.replacen(&label, "", 1);
        let date = after
            .to_ascii_lowercase()
            .find("effective")
            .map(|at| after[at + "effective".len()..].trim().to_string())
            .unwrap_or_default();
        let summary = li
            .find(|e| e.has_class("summary"))
            .map(|e| e.text())
            .filter(|s| !s.is_empty())
            .or_else(|| query_value(&url, "t").map(|t| clean(&t)))
            .unwrap_or_default();
        let id = query_value(&url, "id")
            .filter(|i| !i.is_empty())
            .unwrap_or_else(|| label.split_once(' ').map(|(_, r)| r.to_string()).unwrap_or_else(|| label.clone()));
        list.articles.push(Article {
            kind,
            id,
            title: label,
            date,
            summary,
            url,
            categories: classes,
        });
    }
    // Ford's category menu: nested lists of <a id="CATEGORY"><div class="desc">…</div><div class="acnt">n</div></a>
    let root = d
        .by_id("container")
        .and_then(|c| c.children().find(|e| e.tag() == "ul"))
        .or_else(|| d.by_id("symptoms").and_then(|s| s.first("ul")));
    fn walk(ul: El, depth: usize, articles: &[Article], out: &mut Vec<Category>) {
        for li in ul.children().filter(|e| e.tag() == "li") {
            if let Some(a) = li.children().find(|e| e.tag() == "a") {
                let id = a.attr("id").unwrap_or("").to_string();
                let label = a.find(|e| e.has_class("desc")).map(|e| e.text()).unwrap_or_else(|| a.text());
                if !id.is_empty() && id != "x" && !label.is_empty() {
                    let count = a
                        .find(|e| e.has_class("acnt"))
                        .and_then(|e| e.text().parse().ok())
                        .unwrap_or_else(|| articles.iter().filter(|x| x.categories.contains(&id)).count());
                    if count > 0 {
                        out.push(Category { id, label, depth, count });
                    }
                }
            }
            if let Some(sub) = li.children().find(|e| e.tag() == "ul") {
                walk(sub, depth + 1, articles, out);
            }
        }
    }
    if let Some(root) = root {
        walk(root, 0, &list.articles, &mut list.categories);
    }
    list
}

/// A page that only holds its article in a frame (and little text of its own): the frame's address
fn frame_of(html: &str, base: &str) -> Option<String> {
    let d = Doc::parse(html);
    if d.root().first("body").map(|b| b.text()).unwrap_or_default().len() >= 1500 {
        return None;
    }
    let f = d
        .root()
        .find(|e| matches!(e.tag(), "frame" | "iframe") && e.attr("src").is_some())?;
    resolve(base, f.attr("src")?)
}

impl Ford {
    /// The TSBs / GSBs / SSMs for a VIN (the dealer code from settings is needed: Ford's list asks for it)
    pub async fn tsb_list(&self, vin: &str) -> Result<TsbList> {
        let pacode = self.dealer_code().trim_start_matches(|c: char| c.is_ascii_alphabetic()).to_string();
        if pacode.is_empty() {
            return Err(Error::Format("Ford's TSB list needs the dealer code (in Settings)".into()));
        }
        let book = self.book(vin).await?;
        let p = |k: &str| book.param(k).to_string();
        let year = if book.vehicle.year.is_empty() {
            p("modelYear")
        } else {
            book.vehicle.year.clone()
        };
        let or = |v: String, d: &str| if v.is_empty() { d.to_string() } else { v };
        let url = self.url(&format!(
            "/vdirsnet/TSB_SSM/DisplayArticles.aspx?vin={}&year={}&model={}&pacode={}&country={}&masterId={}&prodType={}&classic=false",
            enc(&book.vin),
            enc(&year),
            enc(&book.vehicle.market_name),
            enc(&pacode),
            enc(&or(p("country"), "USA")),
            enc(&p("vehicleId")),
            enc(&or(p("prodType"), "C")),
        ))?;
        let page = self.text(url).await?;
        let list = read_list(&page);
        self.log(&format!(
            "   TSB list: {} articles, {} categories",
            list.articles.len(),
            list.categories.len()
        ));
        Ok(list)
    }

    /// A document on Ford's site (a TSB, an FSA's document…): read as a document, or a PDF. A page that only holds
    /// the document in a frame is followed.
    pub async fn document(&self, url: &str) -> Result<Fetched> {
        let mut href = url.to_string();
        for _ in 0..3 {
            let u = workbench_net::Url::parse(&href).map_err(|_| Error::Format("Not an address on Ford's site".into()))?;
            if u.scheme() != "https" || !crate::ford_host(u.host_str().unwrap_or("")) {
                return Err(Error::Format("Not an address on Ford's site".into()));
            }
            let req = Http::request(Method::GET, u).header("Accept", "text/html,application/pdf,*/*");
            // Ford sometimes turns the first request away and lets the next one through
            let res = match self.fetch(req.clone()).await {
                Err(Error::NeedsLogin) => {
                    tokio_sleep(400).await;
                    self.fetch(req).await?
                }
                r => r?,
            };
            let at = res.url.to_string();
            if res.content_type.to_ascii_lowercase().contains("pdf") {
                return Ok(Fetched::Pdf { url: at });
            }
            if crate::looks_like_sign_in(&res.body) {
                return Err(Error::NeedsLogin);
            }
            if let Some(next) = frame_of(&res.body, &at).filter(|n| n.starts_with("https://")) {
                href = next;
                continue;
            }
            let blocks = doc::read(&res.body, &at);
            self.log(&format!(
                "   document: {} blocks, {} pictures",
                blocks.len(),
                Block::images(&blocks).len()
            ));
            return Ok(Fetched::Doc { blocks, url: at });
        }
        Err(Error::Format("Couldn't find the document in Ford's page".into()))
    }

    /// A picture or file from Ford's site (a PDF), as bytes (with its type). Asked as Ford's own pages ask (as a
    /// browser, from a PTS page). Ford's document server (fordservicecontent.com) wants its own sign-on cookies:
    /// when it turns the request away, they're fetched ([`Ford::content_sign_on`]) and it's asked once more.
    pub async fn bytes(&self, url: &str) -> Result<(Vec<u8>, String)> {
        let u = workbench_net::Url::parse(url).map_err(|_| Error::Format("Not an address".into()))?;
        let host = u.host_str().unwrap_or("").to_ascii_lowercase();
        if !crate::ford_host(&host) {
            return Err(Error::Format("Not an address on Ford's site".into()));
        }
        let content = host.ends_with("fordservicecontent.com");
        if content {
            self.content_sign_on(false).await;
        }
        let ask = || {
            crate::browser_headers(
                Http::request(Method::GET, u.clone())
                    .header("Accept", "application/pdf,image/avif,image/webp,image/apng,image/*,*/*;q=0.8")
                    .header("Sec-Fetch-Dest", "empty")
                    .header("Sec-Fetch-Mode", "no-cors")
                    .header("Sec-Fetch-Site", if content { "cross-site" } else { "same-origin" }),
            )
        };
        let mut res = self.http().send_bytes(ask()).await?;
        if matches!(res.status.as_u16(), 401 | 403) {
            // Ford sometimes turns the first request away; its document server wants fresh sign-on cookies
            if content {
                self.content_sign_on(true).await;
            } else {
                tokio_sleep(300).await;
            }
            res = self.http().send_bytes(ask()).await?;
        }
        let short: String = url.chars().take(160).collect();
        if !crate::ford_host(res.url.host_str().unwrap_or("")) || matches!(res.status.as_u16(), 401 | 403) {
            self.log(&format!("   picture/file turned away ({}): {short}", res.status.as_u16()));
            return Err(Error::Format(format!(
                "Ford's document server turned it away ({})",
                res.status.as_u16()
            )));
        }
        if !res.status.is_success() {
            self.log(&format!("   picture/file {}: {short}", res.status.as_u16()));
            return Err(Error::Status(res.status.as_u16()));
        }
        Ok((res.bytes, res.content_type))
    }
}

async fn tokio_sleep(ms: u64) {
    tokio::time::sleep(std::time::Duration::from_millis(ms)).await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_list() {
        let html = r##"<html><body>
          <div id="container"><ul>
            <li><a id="ENG"><div class="desc">Engine</div><div class="acnt">2</div></a>
              <ul><li><a id="ENGCOOL"><div class="desc">Cooling</div><div class="acnt">1</div></a></li></ul></li>
            <li><a id="BODY"><div class="desc">Body</div></a></li>
            <li><a id="x"><div class="desc">All</div></a></li>
          </ul></div>
          <ul id="articles">
            <li class="T ENG ENGCOOL"><div class="article"><a href="#DisplayArticle.aspx?id=26-2414&amp;type=TSB&amp;t=Coolant%20leak">TSB 26-2414</a> Effective 01/15/2026</div>
              <div class="summary">Coolant leak at the water pump</div></li>
            <li class="G ENG"><div class="article"><a href="#" onclick="openArt('DisplayArticle.aspx?id=GSB0001&amp;t=Noise'); return false">GSB GSB0001</a></div></li>
            <li class="T"><div class="article">no link</div></li>
          </ul></body></html>"##;
        let l = read_list(html);
        assert_eq!(l.articles.len(), 2);
        let a = &l.articles[0];
        assert_eq!(
            (a.kind.as_str(), a.id.as_str(), a.title.as_str()),
            ("TSB", "26-2414", "TSB 26-2414")
        );
        assert_eq!(a.date, "01/15/2026");
        assert_eq!(a.summary, "Coolant leak at the water pump");
        assert_eq!(
            a.url,
            "https://www.fordtechservice.dealerconnection.com/vdirsnet/TSB_SSM/DisplayArticle.aspx?id=26-2414&type=TSB&t=Coolant%20leak"
        );
        assert!(a.categories.contains(&"ENGCOOL".to_string()));
        let g = &l.articles[1];
        assert_eq!((g.kind.as_str(), g.id.as_str(), g.summary.as_str()), ("GSB", "GSB0001", "Noise"));
        assert_eq!(
            l.categories.iter().map(|c| (c.id.as_str(), c.depth, c.count)).collect::<Vec<_>>(),
            [("ENG", 0, 2), ("ENGCOOL", 1, 1)]
        );
    }

    #[test]
    fn follows_a_frame() {
        let base = "https://www.fordtechservice.dealerconnection.com/vdirsnet/TSB_SSM/DisplayArticle.aspx?id=1";
        assert_eq!(
            frame_of(r#"<html><body><iframe src="Article.aspx?doc=5"></iframe></body></html>"#, base).as_deref(),
            Some("https://www.fordtechservice.dealerconnection.com/vdirsnet/TSB_SSM/Article.aspx?doc=5")
        );
        assert_eq!(frame_of("<html><body><p>The article itself</p></body></html>", base), None);
    }
}
