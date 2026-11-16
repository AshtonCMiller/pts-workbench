//! A secure WebSocket (wss:) connection, for Xtime's chat server (XMPP over WebSocket). Text messages only, which is
//! all XMPP uses. It goes through the same proxy as the app's other requests ([`crate::proxy`]), asking it for a
//! tunnel; the connection is encrypted with the same trusted certificates reqwest uses.
//!
//! ```ignore
//! let mut ws = WebSocket::connect(&url, Some("xmpp"), Some("https://…"), user_agent).await?;
//! ws.send("<open …/>").await?;
//! while let Some(text) = ws.next().await { … }
//! ws.close().await;
//! ```

use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_rustls::client::TlsStream;
use tokio_tungstenite::WebSocketStream;
use tokio_tungstenite::tungstenite::{Message, client::IntoClientRequest, http::HeaderValue};

use crate::{NetError, Result, Url};

/// How long connecting (through the proxy, encryption, the WebSocket handshake) may take
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);

pub struct WebSocket {
    inner: WebSocketStream<TlsStream<TcpStream>>,
}

fn err(what: &str, e: impl std::fmt::Display) -> NetError {
    NetError::WebSocket(format!("{what}: {e}"))
}

impl WebSocket {
    /// Connects to a wss: address. `protocol`: the subprotocol asked for ("xmpp"); `origin`: the page it's for.
    pub async fn connect(url: &Url, protocol: Option<&str>, origin: Option<&str>, user_agent: &str) -> Result<Self> {
        tokio::time::timeout(CONNECT_TIMEOUT, Self::open(url, protocol, origin, user_agent))
            .await
            .map_err(|_| NetError::WebSocket("timed out connecting".into()))?
    }

    async fn open(url: &Url, protocol: Option<&str>, origin: Option<&str>, user_agent: &str) -> Result<Self> {
        if url.scheme() != "wss" {
            return Err(NetError::BadUrl(url.to_string()));
        }
        let host = url.host_str().ok_or_else(|| NetError::BadUrl(url.to_string()))?.to_string();
        let port = url.port().unwrap_or(443);

        // the connection: straight there, or a tunnel through the proxy
        let tcp = match crate::proxy::https_proxy(&host) {
            None => TcpStream::connect((host.as_str(), port))
                .await
                .map_err(|e| err("couldn't connect", e))?,
            Some((ph, pp)) => {
                let mut s = TcpStream::connect((ph.as_str(), pp))
                    .await
                    .map_err(|e| err("couldn't reach the proxy", e))?;
                tunnel(&mut s, &host, port).await?;
                s
            }
        };

        // encrypted, trusting the same certificates as the app's other requests
        let mut roots = rustls::RootCertStore::empty();
        roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        let config = rustls::ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
            .with_safe_default_protocol_versions()
            .map_err(|e| err("encryption", e))?
            .with_root_certificates(roots)
            .with_no_client_auth();
        let name = rustls::pki_types::ServerName::try_from(host.clone()).map_err(|e| err("bad server name", e))?;
        let tls = tokio_rustls::TlsConnector::from(Arc::new(config))
            .connect(name, tcp)
            .await
            .map_err(|e| err("couldn't connect securely", e))?;

        // the WebSocket handshake
        let mut req = url.as_str().into_client_request().map_err(|e| err("bad address", e))?;
        let headers = req.headers_mut();
        // (header names in lowercase: http's fixed names must be)
        let mut put = |k: &'static str, v: &str| {
            if let Ok(v) = HeaderValue::from_str(v) {
                headers.insert(k, v);
            }
        };
        if let Some(p) = protocol {
            put("sec-websocket-protocol", p);
        }
        if let Some(o) = origin {
            put("origin", o);
        }
        put("user-agent", user_agent);
        let (inner, _) = tokio_tungstenite::client_async(req, tls)
            .await
            .map_err(|e| err("not a WebSocket answer", e))?;
        Ok(Self { inner })
    }

    pub async fn send(&mut self, text: &str) -> Result<()> {
        self.inner
            .send(Message::Text(text.to_string()))
            .await
            .map_err(|e| err("couldn't send", e))
    }

    /// The next text message; none once the connection has closed. (Pings are answered on their own.)
    pub async fn next(&mut self) -> Option<Result<String>> {
        loop {
            match self.inner.next().await? {
                Ok(Message::Text(t)) => return Some(Ok(t.to_string())),
                Ok(Message::Binary(b)) => return Some(Ok(String::from_utf8_lossy(&b).into_owned())),
                Ok(Message::Close(_)) => return None,
                Ok(_) => continue, // ping, pong, a frame piece
                Err(e) => return Some(Err(err("connection lost", e))),
            }
        }
    }

    /// Closes the connection (politely; gives up after a second)
    pub async fn close(mut self) {
        let _ = tokio::time::timeout(Duration::from_secs(1), self.inner.close(None)).await;
    }
}

/// Asks the proxy for a tunnel to host:port (HTTP CONNECT)
async fn tunnel(s: &mut TcpStream, host: &str, port: u16) -> Result<()> {
    let ask = format!("CONNECT {host}:{port} HTTP/1.1\r\nHost: {host}:{port}\r\n\r\n");
    s.write_all(ask.as_bytes()).await.map_err(|e| err("couldn't reach the proxy", e))?;
    let mut head = Vec::new();
    let mut byte = [0u8; 1];
    // one byte at a time, so nothing past the proxy's answer is taken (it belongs to the encrypted connection)
    while !head.ends_with(b"\r\n\r\n") {
        if head.len() > 16 * 1024 || s.read(&mut byte).await.map_err(|e| err("the proxy", e))? == 0 {
            return Err(NetError::WebSocket("the proxy closed the connection".into()));
        }
        head.push(byte[0]);
    }
    let first = String::from_utf8_lossy(&head).lines().next().unwrap_or("").to_string();
    if !proxy_said_yes(&first) {
        return Err(NetError::WebSocket(format!("the proxy refused ({first})")));
    }
    Ok(())
}

/// "HTTP/1.1 200 Connection established"
fn proxy_said_yes(status_line: &str) -> bool {
    let mut parts = status_line.split_whitespace();
    parts.next().is_some_and(|v| v.starts_with("HTTP/1.")) && parts.next() == Some("200")
}

#[cfg(test)]
mod tests {
    #[test]
    fn reads_the_proxys_answer() {
        assert!(super::proxy_said_yes("HTTP/1.1 200 Connection established"));
        assert!(super::proxy_said_yes("HTTP/1.0 200 OK"));
        assert!(!super::proxy_said_yes("HTTP/1.1 407 Proxy Authentication Required"));
    }
}
