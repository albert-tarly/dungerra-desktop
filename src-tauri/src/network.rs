//! LAN helpers: parsing a host address typed by the player, probing a host
//! before joining, and listing this computer's addresses for sharing.

use std::io::{Read, Write};
use std::net::{IpAddr, TcpStream, ToSocketAddrs};
use std::time::Duration;

use serde::Deserialize;
use tauri::Url;

/// Port used when the player types a host without one; also the default
/// port offered for hosting.
pub const DEFAULT_LAN_PORT: u16 = 8080;
const PROBE_TIMEOUT: Duration = Duration::from_secs(3);

/// Turns "192.168.1.5", "192.168.1.5:9000", "http://host:9000/" or
/// "[fe80::1]:9000" into the base URL of the game server.
pub fn parse_host_address(input: &str) -> Result<Url, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err("Enter the host's address, for example 192.168.1.5:8080".into());
    }
    let with_scheme = if trimmed.contains("://") {
        trimmed.to_string()
    } else {
        format!("http://{trimmed}")
    };
    let mut url =
        Url::parse(&with_scheme).map_err(|_| format!("{trimmed:?} is not a valid address"))?;
    if url.scheme() != "http" {
        return Err("Only http:// addresses are supported for LAN games".into());
    }
    if url.host_str().is_none_or(str::is_empty) {
        return Err(format!("{trimmed:?} has no host"));
    }
    // Url drops an explicit :80 (the http default), so only fill in our
    // default port when the player did not type one
    let typed_port_80 = trimmed.trim_end_matches('/').ends_with(":80");
    if url.port().is_none() && !typed_port_80 {
        let _ = url.set_port(Some(DEFAULT_LAN_PORT));
    }
    url.set_path("/");
    url.set_query(None);
    url.set_fragment(None);
    Ok(url)
}

#[derive(Deserialize)]
struct VersionResponse {
    game_version: String,
}

/// Checks that a Dungerra server answers at `url` and returns its game version.
pub fn probe(url: &Url) -> Result<String, String> {
    let host = url.host_str().unwrap_or_default();
    let port = url.port_or_known_default().unwrap_or(DEFAULT_LAN_PORT);
    let unreachable = |detail: String| {
        format!("Cannot reach {host}:{port} ({detail}). Check the address, and that the host is hosting and on the same network.")
    };

    let addr = (
        url.host_str().unwrap_or_default().trim_matches(['[', ']']),
        port,
    )
        .to_socket_addrs()
        .map_err(|e| unreachable(e.to_string()))?
        .next()
        .ok_or_else(|| unreachable("no address found".into()))?;
    let mut stream =
        TcpStream::connect_timeout(&addr, PROBE_TIMEOUT).map_err(|e| unreachable(e.to_string()))?;
    stream.set_read_timeout(Some(PROBE_TIMEOUT)).ok();
    stream.set_write_timeout(Some(PROBE_TIMEOUT)).ok();

    // One tiny request; HTTP/1.0 makes the server close the connection after
    // the response, so reading to the end is enough
    let request = format!(
        "GET /api/v1/version HTTP/1.0\r\nHost: {host}:{port}\r\nAccept: application/json\r\n\r\n"
    );
    stream
        .write_all(request.as_bytes())
        .map_err(|e| unreachable(e.to_string()))?;
    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .map_err(|e| unreachable(e.to_string()))?;

    let not_dungerra = || {
        format!("{host}:{port} answered, but it is not a Dungerra game server (or a version too old for desktop play).")
    };
    let (head, body) = response.split_once("\r\n\r\n").ok_or_else(not_dungerra)?;
    let status_ok = head
        .lines()
        .next()
        .is_some_and(|status| status.split_whitespace().nth(1) == Some("200"));
    if !status_ok {
        return Err(not_dungerra());
    }
    let version: VersionResponse = serde_json::from_str(body.trim()).map_err(|_| not_dungerra())?;
    Ok(version.game_version)
}

/// This computer's IPv4 LAN addresses, the likely one first.
pub fn lan_addresses() -> Vec<IpAddr> {
    let mut addresses = Vec::new();
    if let Ok(primary) = local_ip_address::local_ip() {
        addresses.push(primary);
    }
    if let Ok(interfaces) = local_ip_address::list_afinet_netifas() {
        for (_, ip) in interfaces {
            if !addresses.contains(&ip) {
                addresses.push(ip);
            }
        }
    }
    addresses.retain(|ip| match ip {
        IpAddr::V4(v4) => !v4.is_loopback() && !v4.is_link_local() && !v4.is_unspecified(),
        IpAddr::V6(_) => false, // typing IPv6 addresses is no fun; LANs have IPv4
    });
    addresses
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parsed(input: &str) -> String {
        parse_host_address(input)
            .map(|u| u.to_string())
            .unwrap_or_else(|e| format!("ERR {e}"))
    }

    #[test]
    fn parses_host_addresses() {
        assert_eq!(parsed("192.168.1.5"), "http://192.168.1.5:8080/");
        assert_eq!(parsed(" 192.168.1.5:9000 "), "http://192.168.1.5:9000/");
        assert_eq!(
            parsed("http://192.168.1.5:9000/lobby?x=1"),
            "http://192.168.1.5:9000/"
        );
        assert_eq!(parsed("192.168.1.5:80"), "http://192.168.1.5/");
        assert_eq!(parsed("my-pc.local"), "http://my-pc.local:8080/");
        assert_eq!(parsed("[fe80::1]:9000"), "http://[fe80::1]:9000/");
        assert!(parsed("").starts_with("ERR"));
        assert!(parsed("https://example.com").starts_with("ERR"));
        assert!(parsed("192.168.1.5:99999").starts_with("ERR"));
    }

    #[test]
    fn lan_addresses_exclude_loopback() {
        assert!(lan_addresses().iter().all(|ip| !ip.is_loopback()));
    }
}
