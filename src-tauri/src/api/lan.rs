//! LAN discovery: scan the local network for inference servers.
//!
//! Best-effort and non-intrusive: it only opens TCP connections to a handful of
//! well-known inference ports and, for those that answer, sends a couple of tiny
//! HTTP probes to recognise the engine (llama.cpp / vLLM / Ollama / Cloud).
//! No new crate is required — the local IPv4 is discovered with a UDP "connect"
//! that never sends a packet, and the probes run on the existing tokio runtime.

use serde::Serialize;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use tokio::net::TcpStream;
use tokio::sync::Semaphore;
use tokio::time::{timeout, Duration};

use crate::config::ServerKind;

/// A server found during a LAN scan, ready to be turned into a [`ServerProfile`].
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveredServer {
    /// Base URL without any `/v1` suffix, e.g. `http://192.168.1.50:8080`.
    pub url: String,
    /// Detected engine kind (drives which adapter the monitor will use).
    pub kind: ServerKind,
    /// Human-readable label for the UI, e.g. `llama.cpp @ 192.168.1.50:8080`.
    pub label: String,
}

/// Ports that inference servers commonly listen on, in scan order.
const SCAN_PORTS: &[u16] = &[8080, 11434, 8000, 443, 80, 1234];

/// Find the local IPv4 address used for outbound traffic.
///
/// A `UDP` socket is bound and "connected" to an external address; the kernel
/// fills in the local source address without sending anything. Returns `None`
/// when there is no usable route (then only loopback is scanned).
fn local_ip_v4() -> Option<Ipv4Addr> {
    let sock = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    sock.connect("8.8.8.8:80").ok()?;
    match sock.local_addr().ok()? {
        SocketAddr::V4(a) => {
            let ip = *a.ip();
            if ip.is_unspecified() || ip.is_loopback() {
                None
            } else {
                Some(ip)
            }
        }
        _ => None,
    }
}

/// Hosts to scan: loopback is always included, plus the /24 of the local IP
/// (typical home/office subnet). Falls back to loopback-only when the local IP
/// cannot be determined.
fn candidate_hosts() -> Vec<Ipv4Addr> {
    let mut hosts = vec![Ipv4Addr::LOCALHOST];
    if let Some(ip) = local_ip_v4() {
        let o = ip.octets();
        if o[0] != 127 {
            for last in 1..=254u32 {
                hosts.push(Ipv4Addr::new(o[0], o[1], o[2], last as u8));
            }
        }
    }
    hosts
}

/// TCP connect probe with a bounded timeout. Returns `true` if the port is open.
async fn tcp_open(host: Ipv4Addr, port: u16) -> bool {
    let addr = SocketAddr::new(IpAddr::V4(host), port);
    matches!(
        timeout(Duration::from_millis(200), TcpStream::connect(addr)).await,
        Ok(Ok(_))
    )
}

/// HTTP GET with a short timeout; returns the body text on a 2xx response.
async fn http_text(client: &reqwest::Client, url: &str) -> Option<String> {
    match timeout(Duration::from_millis(800), client.get(url).send()).await {
        Ok(Ok(resp)) if resp.status().is_success() => resp.text().await.ok(),
        _ => None,
    }
}

/// Recognise the engine kind at `host:port` by probing a few endpoints.
async fn identify(host: Ipv4Addr, port: u16) -> Option<DiscoveredServer> {
    let url = format!("http://{host}:{port}");
    // LAN addresses must never go through an external proxy.
    let client = reqwest::Client::builder()
        .timeout(Duration::from_millis(800))
        .pool_max_idle_per_host(0)
        .no_proxy()
        .build()
        .ok()?;

    // 1. Ollama: `/api/ps` returns a JSON array of running models.
    if let Some(body) = http_text(&client, &format!("{url}/api/ps")).await {
        if serde_json::from_str::<serde_json::Value>(&body)
            .ok()
            .is_some_and(|v| v.is_array())
        {
            return Some(make(host, port, ServerKind::Ollama, &url));
        }
    }
    // 2. Prometheus metrics: `vllm:` → vLLM, `llamacpp:` → llama.cpp.
    if let Some(body) = http_text(&client, &format!("{url}/metrics")).await {
        if body.contains("vllm:") {
            return Some(make(host, port, ServerKind::Vllm, &url));
        }
        if body.contains("llamacpp:") {
            return Some(make(host, port, ServerKind::Local, &url));
        }
    }
    // 3. llama.cpp health endpoint (server root).
    if http_text(&client, &format!("{url}/health")).await.is_some() {
        return Some(make(host, port, ServerKind::Local, &url));
    }
    // 4. OpenAI-compatible `/v1/models` (vLLM also serves this; vLLM was already
    //    ruled out via `/metrics` above, so treat the rest as generic Cloud).
    if http_text(&client, &format!("{url}/v1/models")).await.is_some() {
        return Some(make(host, port, ServerKind::Cloud, &url));
    }
    // Open port but unrecognised: still report it as a generic local server so
    // the user can add it manually (most likely a llama.cpp we couldn't classify).
    Some(make(host, port, ServerKind::Local, &url))
}

/// Build a [`DiscoveredServer`] with a readable label.
fn make(host: Ipv4Addr, port: u16, kind: ServerKind, url: &str) -> DiscoveredServer {
    DiscoveredServer {
        url: url.to_string(),
        kind,
        label: format!("{} @ {}:{}", kind.label(), host, port),
    }
}

/// Scan the LAN for inference servers and return what was found.
///
/// Spawns a bounded number of concurrent TCP probes (one per candidate host ×
/// port) and, for each open port, identifies the engine. The whole scan is
/// capped at `timeout_ms` (default 9 s); whatever has been found so far is
/// returned when the budget is exceeded.
#[tauri::command]
#[allow(non_snake_case)]
pub async fn scan_lan(timeoutMs: Option<u64>) -> Vec<DiscoveredServer> {
    let hosts = candidate_hosts();
    let budget = Duration::from_millis(timeoutMs.unwrap_or(9000).max(2000));
    let semaphore = std::sync::Arc::new(Semaphore::new(200));
    let mut set = tokio::task::JoinSet::new();
    let mut results: Vec<DiscoveredServer> = Vec::new();

    for host in hosts {
        for &port in SCAN_PORTS {
            let sem = semaphore.clone();
            set.spawn(async move {
                let permit = match sem.acquire().await {
                    Ok(p) => p,
                    Err(_) => return None,
                };
                if tcp_open(host, port).await {
                    // Release the TCP permit before the (slower) HTTP identify.
                    drop(permit);
                    identify(host, port).await
                } else {
                    None
                }
            });
        }
    }

    let collect = async {
        while let Some(res) = set.join_next().await {
            if let Ok(Some(server)) = res {
                if !results.iter().any(|s| s.url == server.url) {
                    results.push(server);
                }
            }
        }
    };

    if timeout(budget, collect).await.is_err() {
        set.abort_all();
    }
    results
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovered_server_serializes_to_camel_case_and_lowercase_kind() {
        let s = DiscoveredServer {
            url: "http://192.168.1.50:8080".to_string(),
            kind: ServerKind::Local,
            label: "llama.cpp @ 192.168.1.50:8080".to_string(),
        };
        let json = serde_json::to_string(&s).unwrap();
        assert!(json.contains("\"url\":"));
        assert!(json.contains("\"kind\":\"local\""));
        assert!(json.contains("\"label\":"));
    }

    #[test]
    fn candidate_hosts_always_includes_loopback() {
        let hosts = candidate_hosts();
        assert!(hosts.contains(&Ipv4Addr::LOCALHOST));
    }
}
