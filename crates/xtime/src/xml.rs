//! A small XML reader for Xtime's chat messages (XMPP stanzas): the Electron app's `xml.js`. [`parse`] gives the top
//! element; each element has its local name (prefixes dropped: "stream:features" → "features"), attributes, child
//! elements and text. Enough for chat, which only looks at local names.

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Element {
    pub name: String,
    pub attrs: Vec<(String, String)>,
    pub nodes: Vec<Node>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Node {
    Element(Element),
    Text(String),
}

fn local(name: &str) -> &str {
    name.split_once(':').map(|(_, l)| l).unwrap_or(name)
}

/// &amp; &lt; &gt; &quot; &apos; and &#…; / &#x…;
pub fn decode(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        let after = &rest[i + 1..];
        let Some(end) = after.find(';').filter(|e| *e <= 10) else {
            out.push('&');
            rest = after;
            continue;
        };
        let e = &after[..end];
        let ch = match e {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            _ if e.starts_with("#x") || e.starts_with("#X") => u32::from_str_radix(&e[2..], 16).ok().and_then(char::from_u32),
            _ if e.starts_with('#') => e[1..].parse::<u32>().ok().and_then(char::from_u32),
            _ => None,
        };
        match ch {
            Some(c) => {
                out.push(c);
                rest = &after[end + 1..];
            }
            None => {
                out.push('&');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Text safe inside XML (and its attribute values)
pub fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

impl Element {
    pub fn attr(&self, name: &str) -> Option<&str> {
        self.attrs.iter().find(|(k, _)| k == name).map(|(_, v)| v.as_str())
    }

    pub fn children(&self) -> impl Iterator<Item = &Element> {
        self.nodes.iter().filter_map(|n| match n {
            Node::Element(e) => Some(e),
            Node::Text(_) => None,
        })
    }

    pub fn child(&self, name: &str) -> Option<&Element> {
        self.children().find(|c| c.name == name)
    }

    pub fn text(&self) -> String {
        self.nodes
            .iter()
            .map(|n| match n {
                Node::Element(e) => e.text(),
                Node::Text(t) => t.clone(),
            })
            .collect()
    }

    /// Every element below with this local name (any depth, in order)
    pub fn descendants(&self, name: &str) -> Vec<&Element> {
        let mut out = Vec::new();
        fn walk<'a>(e: &'a Element, name: &str, out: &mut Vec<&'a Element>) {
            for c in e.children() {
                if name == "*" || c.name == name {
                    out.push(c);
                }
                walk(c, name, out);
            }
        }
        walk(self, local(name), &mut out);
        out
    }
}

/// The top element of an XML text; none when it isn't XML. (XMPP over WebSocket sends "<open …/>" whole; an
/// unclosed top element, a stream's start, still counts.)
pub fn parse(src: &str) -> Option<Element> {
    let mut stack: Vec<Element> = Vec::new();
    let mut root: Option<Element> = None;
    let mut i = 0;
    let add_text = |stack: &mut Vec<Element>, t: String| {
        if !t.is_empty() {
            if let Some(top) = stack.last_mut() {
                top.nodes.push(Node::Text(t));
            }
        }
    };
    while i < src.len() {
        let Some(lt) = src[i..].find('<').map(|p| p + i) else {
            add_text(&mut stack, decode(&src[i..]));
            break;
        };
        if lt > i {
            add_text(&mut stack, decode(&src[i..lt]));
        }
        let rest = &src[lt..];
        if rest.starts_with("<!--") {
            i = lt + rest.find("-->")? + 3;
            continue;
        }
        if rest.starts_with("<![CDATA[") {
            let end = rest.find("]]>")?;
            add_text(&mut stack, rest[9..end].to_string());
            i = lt + end + 3;
            continue;
        }
        if rest.starts_with("<?") || rest.starts_with("<!") {
            i = lt + rest.find('>')? + 1;
            continue;
        }
        // a tag: its end, skipping ">" inside quoted attribute values
        let bytes = src.as_bytes();
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
        if j >= bytes.len() {
            return None; // cut off
        }
        let inner = &src[lt + 1..j];
        i = j + 1;
        if let Some(name) = inner.strip_prefix('/') {
            let el = stack.pop()?;
            if el.name != local(name.trim()) {
                return None; // badly nested: not XML
            }
            match stack.last_mut() {
                Some(parent) => parent.nodes.push(Node::Element(el)),
                None => {
                    root = Some(el);
                    break;
                }
            }
            continue;
        }
        let self_close = inner.ends_with('/');
        let body = if self_close { &inner[..inner.len() - 1] } else { inner };
        let name: String = body.chars().take_while(|c| !c.is_whitespace() && *c != '/' && *c != '>').collect();
        if name.is_empty() {
            return None;
        }
        let mut el = Element {
            name: local(&name).to_string(),
            ..Default::default()
        };
        el.attrs = attributes(&body[name.len()..]);
        if self_close {
            match stack.last_mut() {
                Some(parent) => parent.nodes.push(Node::Element(el)),
                None => {
                    root = Some(el);
                    break;
                }
            }
        } else {
            stack.push(el);
        }
    }
    root.or_else(|| {
        // an unclosed top element (a stream's start): what's inside it so far
        let mut it = stack.into_iter();
        let first = it.next()?;
        Some(first)
    })
}

/// name="value" / name='value'
fn attributes(s: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut rest = s;
    while let Some(eq) = rest.find('=') {
        let name = rest[..eq].trim().to_string();
        let after = rest[eq + 1..].trim_start();
        let Some(q) = after.chars().next().filter(|c| *c == '"' || *c == '\'') else {
            break;
        };
        let Some(end) = after[1..].find(q) else { break };
        if !name.is_empty() {
            out.push((name, decode(&after[1..1 + end])));
        }
        rest = &after[1 + end + 1..];
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_chat_message() {
        let m = parse(
            r#"<message from="bskeeters\40s3590@chat.xtime.com/x" type="chat"><body>Parts are in &amp; ready</body>
            <xtmsg xmlns="http://www.xtime.com/chat"><xtctx namespace="RO" contextId="820367"/><notification/></xtmsg></message>"#,
        )
        .unwrap();
        assert_eq!(m.name, "message");
        assert_eq!(m.attr("from"), Some(r"bskeeters\40s3590@chat.xtime.com/x"));
        assert_eq!(m.child("body").map(|b| b.text()).as_deref(), Some("Parts are in & ready"));
        let ctx = m.descendants("xtctx");
        assert_eq!((ctx[0].attr("namespace"), ctx[0].attr("contextId")), (Some("RO"), Some("820367")));
        assert_eq!(m.descendants("notification").len(), 1);
    }

    #[test]
    fn reads_stream_pieces() {
        let f = parse(r#"<stream:features><mechanisms xmlns="x"><mechanism>SCRAM-SHA-1</mechanism><mechanism>PLAIN</mechanism></mechanisms></stream:features>"#)
            .unwrap();
        assert_eq!(f.name, "features");
        assert_eq!(
            f.descendants("mechanism").iter().map(|m| m.text()).collect::<Vec<_>>(),
            ["SCRAM-SHA-1", "PLAIN"]
        );
        assert_eq!(
            parse(r#"<open xmlns="urn:ietf:params:xml:ns:xmpp-framing" to="chat.xtime.com" version="1.0"/>"#)
                .unwrap()
                .name,
            "open"
        );
        assert_eq!(parse("<a><b></a>"), None);
        assert_eq!(parse("not xml"), None);
        assert_eq!(decode("&#x41;&#66;&lt;&nope;"), "AB<&nope;");
        assert_eq!(escape(r#"a<"b">&'"#), "a&lt;&quot;b&quot;&gt;&amp;&apos;");
    }
}
