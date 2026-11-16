//! A short connection to Xtime's chat server, as Xtime's page makes to send a message or mark messages read: XMPP
//! over a WebSocket (the Electron app's `xmppSend` in `chat.js`).
//!
//! open the stream → sign in (SASL: SCRAM with the chat token as the password) → open it again → bind → send the
//! messages → wait a moment for an error → close.
//!
//! The sign-in ([`Scram`]) and the messages ([`message_stanza`], [`read_stanza`]) are worked out here without the
//! server (tested); [`deliver`] runs the conversation with it.

use std::time::Duration;

use hmac::{Hmac, Mac};
use sha1::Sha1;
use sha2::{Digest, Sha256, Sha512};
use workbench_net::{Url, WebSocket};

use crate::chat::{b64_decode, escape_node};
use crate::xml::{self, escape};
use crate::{Error, Result};

/// Xtime's chat domain (every chat address ends in it)
pub const DOMAIN: &str = "chat.xtime.com";

/// How long the whole conversation with the server may take, and how long to wait after sending for an error
const TIMEOUT: Duration = Duration::from_secs(20);
const SETTLE: Duration = Duration::from_millis(2500);

// ---------- base64 and random ids ----------

pub fn b64_encode(data: &[u8]) -> String {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32;
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(A[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// A random id, shaped like a UUID (what Xtime's page uses for message ids)
pub fn uuid() -> String {
    let mut b = [0u8; 16];
    if getrandom::getrandom(&mut b).is_err() {
        // no system randomness (never seen): the time, which is unique enough for a message id
        let t = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        b = t.to_le_bytes();
    }
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    let h: String = b.iter().map(|x| format!("{x:02x}")).collect();
    format!("{}-{}-{}-{}-{}", &h[0..8], &h[8..12], &h[12..16], &h[16..20], &h[20..32])
}

// ---------- the sign-in: SCRAM (RFC 5802) ----------

#[derive(Debug, Clone, Copy, PartialEq)]
enum Hash {
    Sha1,
    Sha256,
    Sha512,
}

impl Hash {
    fn digest(self, d: &[u8]) -> Vec<u8> {
        match self {
            Hash::Sha1 => Sha1::digest(d).to_vec(),
            Hash::Sha256 => Sha256::digest(d).to_vec(),
            Hash::Sha512 => Sha512::digest(d).to_vec(),
        }
    }

    fn hmac(self, key: &[u8], d: &[u8]) -> Vec<u8> {
        fn run<M: Mac + hmac::digest::KeyInit>(key: &[u8], d: &[u8]) -> Vec<u8> {
            let mut m = <M as hmac::digest::KeyInit>::new_from_slice(key).expect("HMAC takes any key");
            m.update(d);
            m.finalize().into_bytes().to_vec()
        }
        match self {
            Hash::Sha1 => run::<Hmac<Sha1>>(key, d),
            Hash::Sha256 => run::<Hmac<Sha256>>(key, d),
            Hash::Sha512 => run::<Hmac<Sha512>>(key, d),
        }
    }

    fn pbkdf2(self, password: &[u8], salt: &[u8], rounds: u32) -> Vec<u8> {
        match self {
            Hash::Sha1 => {
                let mut out = [0u8; 20];
                pbkdf2::pbkdf2_hmac::<Sha1>(password, salt, rounds, &mut out);
                out.to_vec()
            }
            Hash::Sha256 => {
                let mut out = [0u8; 32];
                pbkdf2::pbkdf2_hmac::<Sha256>(password, salt, rounds, &mut out);
                out.to_vec()
            }
            Hash::Sha512 => {
                let mut out = [0u8; 64];
                pbkdf2::pbkdf2_hmac::<Sha512>(password, salt, rounds, &mut out);
                out.to_vec()
            }
        }
    }
}

/// The sign-in methods Xtime's server may offer, best first (as Xtime's page picks)
pub const MECHANISMS: [&str; 4] = ["SCRAM-SHA-512", "SCRAM-SHA-256", "SCRAM-SHA-1", "PLAIN"];

/// One SCRAM sign-in: [`Scram::start`] is sent first, [`Scram::respond`] answers the server's challenge, and
/// [`Scram::verify`] checks the server's proof at the end
pub struct Scram {
    hash: Hash,
    nonce: String,
    first_bare: String,
    password: String,
    /// What the server should prove at the end
    server_signature: Option<Vec<u8>>,
}

impl Scram {
    /// For a "SCRAM-SHA-…" method; `nonce`: random text (no commas)
    pub fn new(mechanism: &str, user: &str, password: &str, nonce: &str) -> Option<Self> {
        let hash = match mechanism {
            "SCRAM-SHA-512" => Hash::Sha512,
            "SCRAM-SHA-256" => Hash::Sha256,
            "SCRAM-SHA-1" => Hash::Sha1,
            _ => return None,
        };
        let user = user.replace('=', "=3D").replace(',', "=2C");
        Some(Self {
            hash,
            nonce: nonce.to_string(),
            first_bare: format!("n={user},r={nonce}"),
            password: password.to_string(),
            server_signature: None,
        })
    }

    /// The first message (base64)
    pub fn start(&self) -> String {
        b64_encode(format!("n,,{}", self.first_bare).as_bytes())
    }

    /// The answer to the server's challenge (both base64)
    pub fn respond(&mut self, challenge: &str) -> std::result::Result<String, String> {
        let c = String::from_utf8(b64_decode(challenge).ok_or("the challenge isn't base64")?).map_err(|_| "the challenge isn't text")?;
        let field = |k: char| c.split(',').find_map(|kv| kv.strip_prefix(k)?.strip_prefix('='));
        let r = field('r').ok_or("no nonce in the challenge")?;
        if !r.starts_with(&self.nonce) {
            return Err("the server's nonce doesn't match".into());
        }
        let salt = b64_decode(field('s').ok_or("no salt in the challenge")?).ok_or("the salt isn't base64")?;
        let rounds: u32 = field('i')
            .and_then(|i| i.parse().ok())
            .ok_or("no iteration count in the challenge")?;
        let h = self.hash;
        let salted = h.pbkdf2(self.password.as_bytes(), &salt, rounds);
        let client_key = h.hmac(&salted, b"Client Key");
        let stored_key = h.digest(&client_key);
        let without_proof = format!("c=biws,r={r}");
        let auth_message = format!("{},{c},{without_proof}", self.first_bare);
        let signature = h.hmac(&stored_key, auth_message.as_bytes());
        let proof: Vec<u8> = client_key.iter().zip(&signature).map(|(a, b)| a ^ b).collect();
        let server_key = h.hmac(&salted, b"Server Key");
        self.server_signature = Some(h.hmac(&server_key, auth_message.as_bytes()));
        Ok(b64_encode(format!("{without_proof},p={}", b64_encode(&proof)).as_bytes()))
    }

    /// Whether the server's last word ("v=…", base64; may be empty) proves it knew the password too. Nothing to
    /// check (empty) counts as fine: some servers send it with the challenge's answer only.
    pub fn verify(&self, success: &str) -> bool {
        let Some(text) = b64_decode(success).and_then(|b| String::from_utf8(b).ok()) else {
            return success.trim().is_empty();
        };
        match (text.strip_prefix("v="), &self.server_signature) {
            (Some(v), Some(want)) => b64_decode(v).as_deref() == Some(want.as_slice()),
            (None, _) => text.trim().is_empty(),
            (Some(_), None) => false,
        }
    }
}

// ---------- the messages sent ----------

/// One chat message to a conversation, as Xtime's page sends it: to `{channel}+{dealer}@chat.xtime.com`, with the
/// conversation's context (`ctx`: namespace, id) and its channel. `text`: what to send (with mentions as Xtime's
/// markup); `id`: a new message id.
pub fn message_stanza(channel: &str, dealer: &str, ctx: &[(String, String)], text: &str, id: &str) -> String {
    let mut s = format!(
        r#"<message xmlns="jabber:client" to="{}" type="chat" id="{}"><body>{}</body><xtmsg xmlns="http://www.xtime.com/chat">"#,
        escape(&format!("{channel}+{dealer}@{DOMAIN}")),
        escape(id),
        escape(text)
    );
    for (ns, cid) in ctx {
        s.push_str(&format!(r#"<xtctx namespace="{}" contextId="{}"/>"#, escape(ns), escape(cid)));
    }
    s.push_str(&format!(
        r#"<xtchannel xmlns="http://www.xtime.com/channel"><channel dealercode="{}" channel="{}"/></xtchannel></xtmsg></message>"#,
        escape(dealer),
        escape(channel)
    ));
    s
}

/// "Read" for one message (its archive id), as Xtime's page sends when you open a conversation
pub fn read_stanza(archive_id: &str, channel: &str, dealer: &str) -> String {
    format!(
        concat!(
            r#"<message xmlns="jabber:client"><xtreceived xmlns="urn:xtime:chatstates" id="{}" state="read"/>"#,
            r#"<xtmsg xmlns="http://www.xtime.com/chat"><xtchannel xmlns="http://www.xtime.com/xtchannel">"#,
            r#"<channel dealercode="{}" channel="{}"/></xtchannel></xtmsg></message>"#
        ),
        escape(archive_id),
        escape(dealer),
        escape(channel)
    )
}

/// Where the chat server's WebSocket is, from what Xtime's login service says ("https://host/http-bind", maybe in
/// quotes, or a wss: address)
pub fn websocket_address(server: &str) -> Option<String> {
    let s = server.trim().trim_matches('"');
    if s.starts_with("wss://") {
        return Some(s.to_string());
    }
    let host = s.strip_prefix("https://")?.split("/http-bind").next()?;
    (!host.is_empty() && !host.contains('/')).then(|| format!("wss://{host}:443/ws"))
}

// ---------- the conversation with the server ----------

#[derive(Debug, Clone, Copy, PartialEq)]
enum Step {
    Open,
    Auth,
    Restart,
    Bind,
}

/// Why sending didn't work
#[derive(Debug)]
pub enum Refused {
    /// The server turned the sign-in down (the chat token: get a new one and try again once)
    SignIn,
    /// Anything else (couldn't connect, the server ended it, a message came back as an error)
    Other(Error),
}

impl From<Error> for Refused {
    fn from(e: Error) -> Self {
        Refused::Other(e)
    }
}

fn fail(m: impl Into<String>) -> Refused {
    Refused::Other(Error::Format(m.into()))
}

/// Connects to `server` (a wss: address), signs in as `user` (your Xtime username) with the chat `token`, sends the
/// `stanzas`, waits a moment for an error about any of `watch` (message ids), and closes.
pub async fn deliver(
    server: &str,
    origin: &str,
    user_agent: &str,
    user: &str,
    token: &str,
    stanzas: &[String],
    watch: &[String],
) -> std::result::Result<(), Refused> {
    let url = Url::parse(server).map_err(|_| fail("Xtime's chat server address isn't readable"))?;
    let mut ws = WebSocket::connect(&url, Some("xmpp"), Some(origin), user_agent)
        .await
        .map_err(|e| fail(format!("Couldn't reach Xtime's chat server ({e})")))?;
    let r = tokio::time::timeout(TIMEOUT, talk(&mut ws, user, token, stanzas, watch))
        .await
        .unwrap_or_else(|_| Err(fail("Xtime's chat server didn't answer")));
    let _ = ws.send(r#"<close xmlns="urn:ietf:params:xml:ns:xmpp-framing"/>"#).await;
    ws.close().await;
    r
}

async fn talk(ws: &mut WebSocket, user: &str, token: &str, stanzas: &[String], watch: &[String]) -> std::result::Result<(), Refused> {
    let local = escape_node(&user.to_lowercase());
    let open = format!(r#"<open xmlns="urn:ietf:params:xml:ns:xmpp-framing" to="{DOMAIN}" version="1.0"/>"#);
    put(ws, open.clone()).await?;
    let mut step = Step::Open;
    let mut scram: Option<Scram> = None;
    loop {
        let text = match ws.next().await {
            Some(Ok(t)) => t,
            Some(Err(e)) => return Err(Refused::Other(e.into())),
            None => return Err(fail("Xtime's chat server closed the connection")),
        };
        let Some(el) = xml::parse(&text) else { continue };
        match el.name.as_str() {
            "open" => {}
            "close" => return Err(fail("Xtime's chat server ended the connection")),
            "features" if step == Step::Open => {
                let offered: Vec<String> = el.descendants("mechanism").iter().map(|m| m.text().trim().to_string()).collect();
                let Some(mech) = MECHANISMS.iter().find(|m| offered.iter().any(|o| o == *m)) else {
                    return Err(fail(format!("Xtime's chat sign-in isn't supported ({})", offered.join(", "))));
                };
                let first = match Scram::new(mech, &local, token, &uuid().replace('-', "")) {
                    Some(s) => {
                        let f = s.start();
                        scram = Some(s);
                        f
                    }
                    None => b64_encode(format!("\0{local}\0{token}").as_bytes()), // PLAIN
                };
                step = Step::Auth;
                put(
                    ws,
                    format!(r#"<auth xmlns="urn:ietf:params:xml:ns:xmpp-sasl" mechanism="{mech}">{first}</auth>"#),
                )
                .await?;
            }
            "features" if step == Step::Restart => {
                step = Step::Bind;
                let resource = format!("pts-workbench-{}", &uuid()[..8]);
                put(
                    ws,
                    format!(
                        r#"<iq xmlns="jabber:client" type="set" id="bind1"><bind xmlns="urn:ietf:params:xml:ns:xmpp-bind"><resource>{resource}</resource></bind></iq>"#
                    ),
                )
                .await?;
            }
            "challenge" => {
                let s = scram
                    .as_mut()
                    .ok_or_else(|| fail("Xtime's chat server asked an unexpected question"))?;
                let answer = s.respond(el.text().trim()).map_err(|e| fail(format!("Chat sign-in: {e}")))?;
                put(
                    ws,
                    format!(r#"<response xmlns="urn:ietf:params:xml:ns:xmpp-sasl">{answer}</response>"#),
                )
                .await?;
            }
            "success" => {
                if let Some(s) = &scram {
                    if !s.verify(el.text().trim()) {
                        return Err(fail("Xtime's chat server couldn't prove who it is"));
                    }
                }
                step = Step::Restart;
                put(ws, open.clone()).await?;
            }
            "failure" => return Err(Refused::SignIn),
            "iq" if el.attr("id") == Some("bind1") => {
                if el.attr("type") != Some("result") {
                    return Err(fail("Xtime's chat server refused the connection"));
                }
                for s in stanzas {
                    put(ws, s.clone()).await?;
                }
                // nothing comes back for a good message; an error would come back quickly
                return match tokio::time::timeout(SETTLE, watch_errors(ws, watch)).await {
                    Ok(Some(why)) => Err(fail(format!("Xtime's chat didn't take the message ({why})"))),
                    _ => Ok(()),
                };
            }
            _ => {}
        }
    }
}

async fn put(ws: &mut WebSocket, s: String) -> std::result::Result<(), Refused> {
    ws.send(&s).await.map_err(|e| Refused::Other(e.into()))
}

/// Waits for an error about one of the messages sent: why, if one comes
async fn watch_errors(ws: &mut WebSocket, watch: &[String]) -> Option<String> {
    loop {
        let text = ws.next().await?.ok()?;
        let Some(el) = xml::parse(&text) else { continue };
        if let Some(why) = error_about(&el, watch) {
            return Some(why);
        }
    }
}

/// A message that came back as an error, about one of `watch`: why
fn error_about(el: &xml::Element, watch: &[String]) -> Option<String> {
    if el.name != "message" || el.attr("type") != Some("error") || !watch.iter().any(|w| Some(w.as_str()) == el.attr("id")) {
        return None;
    }
    let text = el.descendants("text").first().map(|t| t.text()).filter(|t| !t.trim().is_empty());
    let kind = el
        .descendants("error")
        .first()
        .and_then(|e| e.children().next())
        .map(|c| c.name.clone());
    Some(text.or(kind).unwrap_or_else(|| "error".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signs_in_with_scram() {
        // RFC 5802 (SHA-1) and RFC 7677 (SHA-256)
        let mut s = Scram::new("SCRAM-SHA-1", "user", "pencil", "fyko+d2lbbFgONRv9qkxdawL").unwrap();
        assert_eq!(s.start(), b64_encode(b"n,,n=user,r=fyko+d2lbbFgONRv9qkxdawL"));
        let ch = b64_encode(b"r=fyko+d2lbbFgONRv9qkxdawL3rfcNHYJY1ZVvWVs7j,s=QSXCR+Q6sek8bf92,i=4096");
        let answer = s.respond(&ch).unwrap();
        assert_eq!(
            String::from_utf8(b64_decode(&answer).unwrap()).unwrap(),
            "c=biws,r=fyko+d2lbbFgONRv9qkxdawL3rfcNHYJY1ZVvWVs7j,p=v0X8v3Bz2T0CJGbJQyF0X+HI4Ts="
        );
        assert!(s.verify(&b64_encode(b"v=rmF9pqV8S7suAoZWja4dJRkFsKQ=")));
        assert!(!s.verify(&b64_encode(b"v=AAAA")));

        let mut s = Scram::new("SCRAM-SHA-256", "user", "pencil", "rOprNGfwEbeRWgbNEkqO").unwrap();
        let ch = b64_encode(b"r=rOprNGfwEbeRWgbNEkqO%hvYDpWUa2RaTCAfuxFIlj)hNlF$k0,s=W22ZaJ0SNY7soEsUEjb6gQ==,i=4096");
        let answer = String::from_utf8(b64_decode(&s.respond(&ch).unwrap()).unwrap()).unwrap();
        assert!(answer.ends_with(",p=dHzbZapWIk4jUhN+Ute9ytag9zjfMHgsqmmiz7AndVQ="));
        assert!(s.verify(&b64_encode(b"v=6rriTRBi23WpRR/wtup+mMhUZUn/dB5nLTJRsjl95G4=")));

        let mut s = Scram::new("SCRAM-SHA-512", "user", "pencil", "rOprNGfwEbeRWgbNEkqO").unwrap();
        let answer = String::from_utf8(b64_decode(&s.respond(&ch).unwrap()).unwrap()).unwrap();
        assert!(answer.ends_with(",p=gMGXRcevScNtxZ6/8lQYpGtnsNAc3mGcmNomv+xnoOMw+3R2xNJdMNnzMlTN8PPC6wdp6dybEmDYXYTxwnYPJQ=="));
        // someone else's nonce
        let mut s = Scram::new("SCRAM-SHA-1", "user", "pencil", "abc").unwrap();
        assert!(s.respond(&b64_encode(b"r=xyz,s=QSXCR+Q6sek8bf92,i=1")).is_err());
    }

    #[test]
    fn encodes_base64_and_ids() {
        assert_eq!(b64_encode(b""), "");
        assert_eq!(b64_encode(b"f"), "Zg==");
        assert_eq!(b64_encode(b"fo"), "Zm8=");
        assert_eq!(b64_encode(b"foo"), "Zm9v");
        assert_eq!(b64_decode(&b64_encode(&[0, 255, 7, 9])).unwrap(), [0, 255, 7, 9]);
        let id = uuid();
        assert_eq!((id.len(), &id[14..15]), (36, "4"));
        assert_ne!(uuid(), uuid());
    }

    #[test]
    fn writes_messages_like_xtime() {
        let ctx = vec![
            ("RO".to_string(), "820367".to_string()),
            ("DEALER_CODE".to_string(), "F1".to_string()),
        ];
        let s = message_stanza("3590-820367-1", "F1", &ctx, "Parts <in> & ready", "m1");
        assert!(s.starts_with(r#"<message xmlns="jabber:client" to="3590-820367-1+F1@chat.xtime.com" type="chat" id="m1"><body>Parts &lt;in&gt; &amp; ready</body>"#));
        assert!(s.contains(r#"<xtctx namespace="RO" contextId="820367"/>"#));
        assert!(s.ends_with(r#"<channel dealercode="F1" channel="3590-820367-1"/></xtchannel></xtmsg></message>"#));
        let parsed = xml::parse(&s).unwrap();
        assert_eq!(parsed.child("body").unwrap().text(), "Parts <in> & ready");
        let r = read_stanza("101", "3590-820367-1", "F1");
        assert_eq!(xml::parse(&r).unwrap().descendants("xtreceived")[0].attr("state"), Some("read"));
    }

    #[test]
    fn finds_the_server_and_its_errors() {
        assert_eq!(
            websocket_address("\"https://ejd.xtime.com/http-bind\"").as_deref(),
            Some("wss://ejd.xtime.com:443/ws")
        );
        assert_eq!(websocket_address("wss://x/ws").as_deref(), Some("wss://x/ws"));
        assert_eq!(websocket_address("nonsense"), None);
        let e = xml::parse(r#"<message type="error" id="m1"><error type="cancel"><item-not-found xmlns="x"/></error></message>"#).unwrap();
        assert_eq!(error_about(&e, &["m1".into()]).as_deref(), Some("item-not-found"));
        assert_eq!(error_about(&e, &["m2".into()]), None);
    }
}
