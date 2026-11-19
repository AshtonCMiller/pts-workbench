//! Which proxy a secure connection goes through, the way the rest of the app's requests find it (reqwest looks in the
//! same places): the HTTPS_PROXY / ALL_PROXY settings first (NO_PROXY respected), then on Windows the system's proxy
//! (Settings › Network › Proxy, "Use a proxy server"). Only plain HTTP proxies (asked for a tunnel with CONNECT); an
//! automatic setup script isn't read.

/// The proxy for a secure connection to `host`: its host and port; none for a direct connection
pub fn https_proxy(host: &str) -> Option<(String, u16)> {
    let env = |k: &str| std::env::var(k).ok().filter(|v| !v.trim().is_empty());
    if let Some(v) = ["HTTPS_PROXY", "https_proxy", "ALL_PROXY", "all_proxy"].iter().find_map(|k| env(k)) {
        let skip = env("NO_PROXY").or_else(|| env("no_proxy")).unwrap_or_default();
        return if bypassed(host, &skip) { None } else { parse_proxy(&v) };
    }
    system_proxy(host)
}

/// "http://user:pw@proxy.lan:3128/" → ("proxy.lan", 3128). Only HTTP proxies (no SOCKS); the port is 80 when none.
pub fn parse_proxy(v: &str) -> Option<(String, u16)> {
    let v = v.trim();
    let rest = match v.split_once("://") {
        Some((scheme, rest)) if scheme.eq_ignore_ascii_case("http") || scheme.eq_ignore_ascii_case("https") => rest,
        Some(_) => return None,
        None => v,
    };
    let rest = rest.split('/').next().unwrap_or("");
    let rest = rest.rsplit_once('@').map(|(_, h)| h).unwrap_or(rest);
    let (host, port) = match rest.rsplit_once(':') {
        Some((h, p)) if !rest.ends_with(']') => (h, p.parse().ok()?),
        _ => (rest, 80),
    };
    let host = host.trim_start_matches('[').trim_end_matches(']');
    (!host.is_empty()).then(|| (host.to_string(), port))
}

/// Whether `host` is on a list of addresses that skip the proxy ("*.lan, .corp, localhost, 10.*, <local>"; commas or
/// semicolons between them)
pub fn bypassed(host: &str, list: &str) -> bool {
    let host = host.to_ascii_lowercase();
    list.split([',', ';'])
        .map(|e| e.trim().to_ascii_lowercase())
        .filter(|e| !e.is_empty())
        .any(|e| {
            if e == "*" {
                true
            } else if e == "<local>" {
                !host.contains('.')
            } else if let Some(suffix) = e.strip_prefix('.') {
                host.ends_with(&format!(".{suffix}")) || host == suffix
            } else {
                glob(&e, &host)
            }
        })
}

/// `*` matches anything (any number of characters)
fn glob(pattern: &str, s: &str) -> bool {
    let parts: Vec<&str> = pattern.split('*').collect();
    if parts.len() == 1 {
        return pattern == s;
    }
    let mut rest = s;
    for (i, p) in parts.iter().enumerate() {
        if i == 0 {
            match rest.strip_prefix(p) {
                Some(r) => rest = r,
                None => return false,
            }
        } else if i == parts.len() - 1 {
            return rest.ends_with(p);
        } else {
            match rest.find(p) {
                Some(at) => rest = &rest[at + p.len()..],
                None => return false,
            }
        }
    }
    true
}

/// "proxy:8080", or "http=proxy:8080;https=proxy:8443" (the https one for secure connections)
pub fn parse_system_server(v: &str) -> Option<(String, u16)> {
    if !v.contains('=') {
        return parse_proxy(v);
    }
    let pick = |scheme: &str| {
        v.split(';').find_map(|part| {
            let (k, val) = part.split_once('=')?;
            k.trim().eq_ignore_ascii_case(scheme).then(|| val.trim().to_string())
        })
    };
    parse_proxy(&pick("https").or_else(|| pick("http"))?)
}

#[cfg(windows)]
fn system_proxy(host: &str) -> Option<(String, u16)> {
    use windows_sys::Win32::Foundation::ERROR_SUCCESS;
    use windows_sys::Win32::System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_DWORD, RRF_RT_REG_SZ, RegGetValueW};

    let wide = |s: &str| s.encode_utf16().chain(std::iter::once(0)).collect::<Vec<u16>>();
    let key = wide(r"Software\Microsoft\Windows\CurrentVersion\Internet Settings");
    let dword = |name: &str| -> Option<u32> {
        let value = wide(name);
        let mut data: u32 = 0;
        let mut size = std::mem::size_of::<u32>() as u32;
        // SAFETY: both names end in 0, and `data` is a u32 with its size in `size`
        let status = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                key.as_ptr(),
                value.as_ptr(),
                RRF_RT_REG_DWORD,
                std::ptr::null_mut(),
                (&mut data as *mut u32).cast(),
                &mut size,
            )
        };
        (status == ERROR_SUCCESS).then_some(data)
    };
    let string = |name: &str| -> Option<String> {
        let value = wide(name);
        let mut buf = vec![0u16; 2048];
        let mut size = (buf.len() * 2) as u32;
        // SAFETY: both names end in 0, and `buf` is `size` bytes long
        let status = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                key.as_ptr(),
                value.as_ptr(),
                RRF_RT_REG_SZ,
                std::ptr::null_mut(),
                buf.as_mut_ptr().cast(),
                &mut size,
            )
        };
        if status != ERROR_SUCCESS {
            return None;
        }
        let len = (size as usize / 2).min(buf.len());
        let s = String::from_utf16_lossy(&buf[..len]);
        Some(s.trim_end_matches('\0').to_string())
    };
    if dword("ProxyEnable")? == 0 {
        return None;
    }
    let server = string("ProxyServer")?;
    if bypassed(host, &string("ProxyOverride").unwrap_or_default()) {
        return None;
    }
    parse_system_server(&server)
}

#[cfg(not(windows))]
fn system_proxy(_host: &str) -> Option<(String, u16)> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_proxy_settings() {
        assert_eq!(parse_proxy("http://user:pw@proxy.lan:3128/"), Some(("proxy.lan".into(), 3128)));
        assert_eq!(parse_proxy("proxy.lan"), Some(("proxy.lan".into(), 80)));
        assert_eq!(parse_proxy("socks5://p:1080"), None);
        assert_eq!(parse_system_server("proxy:8080"), Some(("proxy".into(), 8080)));
        assert_eq!(parse_system_server("http=a:1;https=b:2"), Some(("b".into(), 2)));
        assert_eq!(parse_system_server("ftp=a:1;http=c:3"), Some(("c".into(), 3)));
    }

    #[test]
    fn skips_the_proxy_for_listed_hosts() {
        assert!(bypassed("chat.xtime.com", "localhost, *.xtime.com"));
        assert!(bypassed("printer", "<local>"));
        assert!(!bypassed("chat.xtime.com", "<local>;10.*"));
        assert!(bypassed("10.1.2.3", "<local>;10.*"));
        assert!(bypassed("a.corp", ".corp"));
        assert!(!bypassed("xcorp", ".corp"));
        assert!(bypassed("anything", "*"));
    }
}
