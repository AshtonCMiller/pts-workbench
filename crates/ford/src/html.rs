//! A forgiving HTML reader for Ford's pages (OASIS now; the manual's procedures later). It builds the page's tree the
//! way a browser roughly would: tags in any case, unquoted attributes, elements that never close (`<br>`, `<img>`…),
//! `<li>` / `<p>` / table cells closed by the next one, stray closing tags ignored. Scripts and styles are left out.
//!
//! Then it's asked like the browser's DOM: [`Doc::by_id`], [`El::find`] / [`El::find_all`] (by a test on each
//! element), [`El::attr`], [`El::has_class`], [`El::text`], [`El::next_element`], [`El::closest`].

/// A whole page
#[derive(Debug, Clone)]
pub struct Doc {
    nodes: Vec<Node>,
}

#[derive(Debug, Clone)]
struct Node {
    parent: Option<usize>,
    children: Vec<usize>,
    kind: Kind,
}

#[derive(Debug, Clone)]
enum Kind {
    Element { tag: String, attrs: Vec<(String, String)> },
    Text(String),
}

/// A piece of an element's content: an element, or text
#[derive(Clone, Copy, Debug)]
pub enum Piece<'a> {
    El(El<'a>),
    Text(&'a str),
}

/// An element of a page (cheap to copy)
#[derive(Clone, Copy)]
pub struct El<'a> {
    doc: &'a Doc,
    id: usize,
}

impl std::fmt::Debug for El<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "<{}", self.tag())?;
        if let Some(id) = self.attr("id") {
            write!(f, " id={id:?}")?;
        }
        write!(f, ">")
    }
}

impl PartialEq for El<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id && std::ptr::eq(self.doc, other.doc)
    }
}

/// Elements that never have content
const VOID: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param", "source", "track", "wbr",
];
/// Elements whose content is text to skip entirely
const SKIPPED: &[&str] = &["script", "style", "noscript", "template"];
/// Elements that end an open `<p>`
const BLOCKS: &[&str] = &[
    "p",
    "div",
    "ul",
    "ol",
    "dl",
    "table",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "section",
    "form",
    "pre",
    "blockquote",
    "hr",
];

/// &amp; &nbsp; &#8211; … (the common named ones; others are left as they are)
pub fn decode(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        let after = &rest[i + 1..];
        let end = after.find(';').filter(|e| *e <= 10);
        let ch = end.and_then(|e| {
            let name = &after[..e];
            if let Some(hex) = name.strip_prefix("#x").or_else(|| name.strip_prefix("#X")) {
                return u32::from_str_radix(hex, 16).ok().and_then(char::from_u32);
            }
            if let Some(dec) = name.strip_prefix('#') {
                return dec.parse::<u32>().ok().and_then(char::from_u32);
            }
            Some(match name {
                "amp" => '&',
                "lt" => '<',
                "gt" => '>',
                "quot" => '"',
                "apos" => '\'',
                "nbsp" => '\u{a0}',
                "ndash" => '–',
                "mdash" => '—',
                "lsquo" => '‘',
                "rsquo" => '’',
                "ldquo" => '“',
                "rdquo" => '”',
                "hellip" => '…',
                "bull" => '•',
                "middot" => '·',
                "deg" => '°',
                "copy" => '©',
                "reg" => '®',
                "trade" => '™',
                "times" => '×',
                "frac12" => '½',
                "frac14" => '¼',
                "frac34" => '¾',
                "plusmn" => '±',
                "micro" => 'µ',
                "ohm" | "Omega" => 'Ω',
                "le" => '≤',
                "ge" => '≥',
                "rarr" => '→',
                "larr" => '←',
                "uarr" => '↑',
                "darr" => '↓',
                _ => return None,
            })
        });
        match (ch, end) {
            (Some(c), Some(e)) => {
                out.push(c);
                rest = &after[e + 1..];
            }
            _ => {
                out.push('&');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Spaces of any kind (non-breaking ones too) as one, trimmed
pub fn clean(s: &str) -> String {
    s.split(|c: char| c.is_whitespace() || c == '\u{a0}')
        .filter(|w| !w.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

fn find_ci(hay: &str, needle: &str) -> Option<usize> {
    let (h, n) = (hay.as_bytes(), needle.as_bytes());
    (0..h.len().saturating_sub(n.len() - 1)).find(|&i| h[i..i + n.len()].eq_ignore_ascii_case(n))
}

/// name="value" name='value' name=value name
fn attributes(s: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        while i < b.len() && (b[i].is_ascii_whitespace() || b[i] == b'/') {
            i += 1;
        }
        let start = i;
        while i < b.len() && !b[i].is_ascii_whitespace() && b[i] != b'=' && b[i] != b'/' {
            i += 1;
        }
        if start == i {
            break;
        }
        let name = s[start..i].to_ascii_lowercase();
        while i < b.len() && b[i].is_ascii_whitespace() {
            i += 1;
        }
        let mut value = String::new();
        if i < b.len() && b[i] == b'=' {
            i += 1;
            while i < b.len() && b[i].is_ascii_whitespace() {
                i += 1;
            }
            if i < b.len() && (b[i] == b'"' || b[i] == b'\'') {
                let q = b[i];
                let vs = i + 1;
                let ve = s[vs..].find(q as char).map(|e| vs + e).unwrap_or(s.len());
                value = decode(&s[vs..ve]);
                i = (ve + 1).min(s.len());
            } else {
                let vs = i;
                while i < b.len() && !b[i].is_ascii_whitespace() {
                    i += 1;
                }
                value = decode(&s[vs..i]);
            }
        }
        if !out.iter().any(|(k, _)| *k == name) {
            out.push((name, value));
        }
    }
    out
}

impl Doc {
    pub fn parse(src: &str) -> Doc {
        let mut doc = Doc {
            nodes: vec![Node {
                parent: None,
                children: Vec::new(),
                kind: Kind::Element {
                    tag: "#root".into(),
                    attrs: Vec::new(),
                },
            }],
        };
        let mut stack: Vec<usize> = vec![0];
        let mut i = 0;
        let bytes = src.as_bytes();
        while i < src.len() {
            let Some(lt) = src[i..].find('<').map(|p| p + i) else {
                doc.text(*stack.last().unwrap_or(&0), &src[i..]);
                break;
            };
            if lt > i {
                doc.text(*stack.last().unwrap_or(&0), &src[i..lt]);
            }
            let rest = &src[lt..];
            if rest.starts_with("<!--") {
                i = rest.find("-->").map(|e| lt + e + 3).unwrap_or(src.len());
                continue;
            }
            if rest.starts_with("<!") || rest.starts_with("<?") {
                i = rest.find('>').map(|e| lt + e + 1).unwrap_or(src.len());
                continue;
            }
            // not a tag at all ("a < b"): text
            let next = bytes.get(lt + 1).copied().unwrap_or(b' ');
            if !(next.is_ascii_alphabetic() || next == b'/') {
                doc.text(*stack.last().unwrap_or(&0), "<");
                i = lt + 1;
                continue;
            }
            // the tag's end, skipping ">" inside quoted values
            let mut j = lt + 1;
            let mut quote: Option<u8> = None;
            while j < bytes.len() {
                let c = bytes[j];
                match quote {
                    Some(q) if c == q => quote = None,
                    Some(_) => {}
                    None if c == b'"' || c == b'\'' => quote = Some(c),
                    None if c == b'>' => break,
                    None => {}
                }
                j += 1;
            }
            let inner = &src[lt + 1..j.min(src.len())];
            i = (j + 1).min(src.len());

            if let Some(close) = inner.strip_prefix('/') {
                let name = close.trim().split_whitespace().next().unwrap_or("").to_ascii_lowercase();
                // closes the nearest open one of its kind (and anything left open inside it); a stray one is ignored
                if let Some(at) = stack.iter().rposition(|&n| doc.tag_of(n) == name) {
                    if at > 0 {
                        stack.truncate(at);
                    }
                }
                continue;
            }
            let name: String = inner
                .chars()
                .take_while(|c| !c.is_whitespace() && *c != '/' && *c != '>')
                .collect::<String>()
                .to_ascii_lowercase();
            let self_close = inner.trim_end().ends_with('/');
            let attrs = attributes(&inner[name.len()..]);

            if SKIPPED.contains(&name.as_str()) {
                if !self_close {
                    i = find_ci(&src[i..], &format!("</{name}"))
                        .map(|e| {
                            let at = i + e;
                            src[at..].find('>').map(|g| at + g + 1).unwrap_or(src.len())
                        })
                        .unwrap_or(src.len());
                }
                continue;
            }

            // what a new element closes on its own
            let close_back = |doc: &Doc, stack: &mut Vec<usize>, tags: &[&str], stop: &[&str]| {
                for k in (1..stack.len()).rev() {
                    let t = doc.tag_of(stack[k]);
                    if stop.contains(&t.as_str()) {
                        break;
                    }
                    if tags.contains(&t.as_str()) {
                        stack.truncate(k);
                        break;
                    }
                }
            };
            match name.as_str() {
                "li" => close_back(&doc, &mut stack, &["li"], &["ul", "ol"]),
                "dt" | "dd" => close_back(&doc, &mut stack, &["dt", "dd"], &["dl"]),
                "option" => close_back(&doc, &mut stack, &["option"], &["select"]),
                "tr" => close_back(&doc, &mut stack, &["tr"], &["table", "tbody", "thead", "tfoot"]),
                "td" | "th" => close_back(&doc, &mut stack, &["td", "th"], &["tr", "table"]),
                "tbody" | "thead" | "tfoot" => close_back(&doc, &mut stack, &["tbody", "thead", "tfoot"], &["table"]),
                _ => {}
            }
            if BLOCKS.contains(&name.as_str()) {
                if let Some(&top) = stack.last() {
                    if doc.tag_of(top) == "p" {
                        stack.pop();
                    }
                }
            }

            let parent = *stack.last().unwrap_or(&0);
            let id = doc.nodes.len();
            doc.nodes.push(Node {
                parent: Some(parent),
                children: Vec::new(),
                kind: Kind::Element { tag: name.clone(), attrs },
            });
            doc.nodes[parent].children.push(id);
            if !self_close && !VOID.contains(&name.as_str()) {
                stack.push(id);
            }
        }
        doc
    }

    fn text(&mut self, parent: usize, raw: &str) {
        if raw.is_empty() {
            return;
        }
        let id = self.nodes.len();
        self.nodes.push(Node {
            parent: Some(parent),
            children: Vec::new(),
            kind: Kind::Text(decode(raw)),
        });
        self.nodes[parent].children.push(id);
    }

    fn tag_of(&self, n: usize) -> String {
        match &self.nodes[n].kind {
            Kind::Element { tag, .. } => tag.clone(),
            Kind::Text(_) => String::new(),
        }
    }

    pub fn root(&self) -> El<'_> {
        El { doc: self, id: 0 }
    }

    /// The element with this id
    pub fn by_id(&self, id: &str) -> Option<El<'_>> {
        self.root().find(|e| e.attr("id") == Some(id))
    }

    /// The page's title, cleaned up
    pub fn title(&self) -> String {
        self.root().find(|e| e.tag() == "title").map(|t| t.text()).unwrap_or_default()
    }
}

impl<'a> El<'a> {
    fn node(&self) -> &'a Node {
        &self.doc.nodes[self.id]
    }

    /// Which element it is (the same for the same element of the same page)
    pub fn key(&self) -> usize {
        self.id
    }

    /// Its tag, in lowercase ("div")
    pub fn tag(&self) -> &'a str {
        match &self.node().kind {
            Kind::Element { tag, .. } => tag,
            Kind::Text(_) => "",
        }
    }

    pub fn attr(&self, name: &str) -> Option<&'a str> {
        match &self.node().kind {
            Kind::Element { attrs, .. } => attrs.iter().find(|(k, _)| k == name).map(|(_, v)| v.as_str()),
            Kind::Text(_) => None,
        }
    }

    pub fn has_class(&self, class: &str) -> bool {
        self.attr("class").is_some_and(|c| c.split_whitespace().any(|x| x == class))
    }

    /// Hidden on the page (`style="display: none"`, or `hidden`)
    pub fn hidden(&self) -> bool {
        self.attr("hidden").is_some()
            || self
                .attr("style")
                .is_some_and(|s| s.to_ascii_lowercase().replace(' ', "").contains("display:none"))
    }

    /// All the text inside, cleaned up (spaces as one, trimmed)
    pub fn text(&self) -> String {
        clean(&self.raw_text())
    }

    /// All the text inside, as it is
    pub fn raw_text(&self) -> String {
        let mut out = String::new();
        fn walk(doc: &Doc, n: usize, out: &mut String) {
            match &doc.nodes[n].kind {
                Kind::Text(t) => out.push_str(t),
                Kind::Element { tag, .. } => {
                    // blocks and cells keep their words apart
                    let apart = matches!(
                        tag.as_str(),
                        "p" | "div" | "li" | "td" | "th" | "tr" | "label" | "title" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "dt" | "dd"
                    );
                    if tag == "br" {
                        out.push('\n');
                    }
                    if apart {
                        out.push(' ');
                    }
                    for &c in &doc.nodes[n].children {
                        walk(doc, c, out);
                    }
                    if apart {
                        out.push(' ');
                    }
                }
            }
        }
        walk(self.doc, self.id, &mut out);
        out
    }

    /// Its content in order: child elements and text
    pub fn pieces(&self) -> Vec<Piece<'a>> {
        let doc = self.doc;
        self.node()
            .children
            .iter()
            .map(|&c| match &doc.nodes[c].kind {
                Kind::Element { .. } => Piece::El(El { doc, id: c }),
                Kind::Text(t) => Piece::Text(t.as_str()),
            })
            .collect()
    }

    /// Its child elements, in order
    pub fn children(&self) -> impl Iterator<Item = El<'a>> + 'a {
        let doc = self.doc;
        self.node()
            .children
            .iter()
            .filter(move |&&c| matches!(doc.nodes[c].kind, Kind::Element { .. }))
            .map(move |&c| El { doc, id: c })
    }

    pub fn parent(&self) -> Option<El<'a>> {
        self.node().parent.filter(|p| *p != 0).map(|p| El { doc: self.doc, id: p })
    }

    /// The next element after it, under the same parent
    pub fn next_element(&self) -> Option<El<'a>> {
        let parent = self.node().parent?;
        let sibs = &self.doc.nodes[parent].children;
        let at = sibs.iter().position(|&c| c == self.id)?;
        sibs[at + 1..]
            .iter()
            .find(|&&c| matches!(self.doc.nodes[c].kind, Kind::Element { .. }))
            .map(|&c| El { doc: self.doc, id: c })
    }

    /// It, or the nearest element around it, that passes the test
    pub fn closest(&self, test: impl Fn(&El<'a>) -> bool) -> Option<El<'a>> {
        let mut cur = Some(*self);
        while let Some(e) = cur {
            if test(&e) {
                return Some(e);
            }
            cur = e.parent();
        }
        None
    }

    /// Whether it's inside `other`
    pub fn is_inside(&self, other: &El<'a>) -> bool {
        let mut cur = self.parent();
        while let Some(e) = cur {
            if e == *other {
                return true;
            }
            cur = e.parent();
        }
        false
    }

    /// Every element inside it, in page order
    pub fn descendants(&self) -> Vec<El<'a>> {
        let mut out = Vec::new();
        fn walk<'a>(doc: &'a Doc, n: usize, out: &mut Vec<El<'a>>) {
            for &c in &doc.nodes[n].children {
                if matches!(doc.nodes[c].kind, Kind::Element { .. }) {
                    out.push(El { doc, id: c });
                    walk(doc, c, out);
                }
            }
        }
        walk(self.doc, self.id, &mut out);
        out
    }

    /// The first element inside it that passes the test
    pub fn find(&self, test: impl Fn(&El<'a>) -> bool) -> Option<El<'a>> {
        self.descendants().into_iter().find(|e| test(e))
    }

    /// Every element inside it that passes the test
    pub fn find_all(&self, test: impl Fn(&El<'a>) -> bool) -> Vec<El<'a>> {
        self.descendants().into_iter().filter(|e| test(e)).collect()
    }

    /// The first element inside with this tag
    pub fn first(&self, tag: &str) -> Option<El<'a>> {
        self.find(|e| e.tag() == tag)
    }

    /// Every element inside with this tag
    pub fn all(&self, tag: &str) -> Vec<El<'a>> {
        self.find_all(|e| e.tag() == tag)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_untidy_html() {
        let d = Doc::parse(concat!(
            "<!DOCTYPE html><HTML><head><title>OASIS RESULTS: 1FT</title><script>if (a < b) { x = '</div>'; }</script></head>",
            "<body><div id=main class='panel  panel-primary'><p>One<p>Two &amp; three<br>four",
            "<ul><li>A<li>B <b>bold</b></ul><img src=x.png><label class=label>VIN:</label> <label>1FT&nbsp;X</label>",
            "<table><tr><td>a<td>b<tr><td>c</table></span></div><div id=after style='display: none'>x</div>",
        ));
        assert_eq!(d.title(), "OASIS RESULTS: 1FT");
        let main = d.by_id("main").unwrap();
        assert!(main.has_class("panel-primary") && main.has_class("panel"));
        assert_eq!(main.all("p").len(), 2);
        assert_eq!(main.all("p")[1].text(), "Two & three four");
        let lis = main.all("li");
        assert_eq!(lis.iter().map(|l| l.text()).collect::<Vec<_>>(), ["A", "B bold"]);
        let label = main.find(|e| e.has_class("label")).unwrap();
        assert_eq!(label.next_element().unwrap().text(), "1FT X");
        assert_eq!(main.all("tr").len(), 2);
        assert_eq!(main.all("td").len(), 3);
        assert!(d.by_id("after").unwrap().hidden());
        assert_eq!(d.by_id("after").unwrap().parent().unwrap().tag(), "body"); // the stray </span> didn't close anything wrong
        assert!(lis[1].first("b").unwrap().is_inside(&main));
        assert_eq!(lis[1].first("b").unwrap().closest(|e| e.tag() == "ul").unwrap().tag(), "ul");
        assert!(d.root().find(|e| e.tag() == "script").is_none());
    }

    #[test]
    fn reads_attributes_and_entities() {
        let a = attributes(r#" href="/a?x=1&amp;y=2" data-fsanum=24S10 disabled title='it"s'"#);
        assert_eq!(
            a,
            vec![
                ("href".into(), "/a?x=1&y=2".into()),
                ("data-fsanum".into(), "24S10".into()),
                ("disabled".into(), String::new()),
                ("title".into(), "it\"s".into()),
            ]
        );
        assert_eq!(decode("5&deg; &#8211; &#x2014; &bogus; a&b"), "5° – — &bogus; a&b");
        assert_eq!(clean("  a \u{a0} b\n c "), "a b c");
    }
}
