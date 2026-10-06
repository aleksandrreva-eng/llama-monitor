//! HTTP probing: the async half of the adapter layer.
//!
//! An [`Adapter`](super::adapters::Adapter) declares *what* to fetch and how to
//! read it; this module owns *how* the fetching happens (client policy, auth,
//! per-probe error classification). Keeping the two apart is what makes the
//! parsers pure and unit-testable without a server.

use std::collections::HashMap;
use std::time::Duration;

use reqwest::RequestBuilder;

use crate::config::Settings;

/// One HTTP probe an adapter wants performed.
#[derive(Debug, Clone)]
pub struct Probe {
    /// Stable id the adapter uses to look the response up again.
    pub id: &'static str,
    pub url: String,
    /// `true` when a failure means "this server is not usable at all" — the poll
    /// then fails as a whole and the UI shows "server unavailable". `false`
    /// (the common case) means the endpoint is a best-effort extra.
    pub required: bool,
}

impl Probe {
    pub fn required(id: &'static str, url: String) -> Self {
        Probe {
            id,
            url,
            required: true,
        }
    }

    pub fn optional(id: &'static str, url: String) -> Self {
        Probe {
            id,
            url,
            required: false,
        }
    }
}

/// Outcome of a single probe.
#[derive(Debug, Clone)]
pub enum ProbeOutcome {
    /// 2xx, body captured as text.
    Ok(String),
    /// The endpoint answered with a non-2xx status.
    Status(u16),
    /// The request never completed (DNS, connect, timeout).
    Transport(String),
}

impl ProbeOutcome {
    pub fn body(&self) -> Option<&str> {
        match self {
            ProbeOutcome::Ok(text) => Some(text),
            _ => None,
        }
    }

    pub fn status(&self) -> Option<u16> {
        match self {
            ProbeOutcome::Status(code) => Some(*code),
            _ => None,
        }
    }

    pub fn is_ok(&self) -> bool {
        matches!(self, ProbeOutcome::Ok(_))
    }

    /// `true` when the server definitively does not serve this endpoint.
    ///
    /// 501 is what llama.cpp answers for `/metrics` without `--metrics`; 404 is
    /// what a build without the endpoint answers. Both are *definitive* — unlike
    /// a connection error or a 502 during a model reload, they will not fix
    /// themselves, so only these produce a diagnostic.
    pub fn is_missing(&self) -> bool {
        matches!(self.status(), Some(404) | Some(501))
    }
}

/// All probe outcomes for one poll, keyed by probe id.
#[derive(Debug, Default)]
pub struct ProbeReport {
    outcomes: HashMap<&'static str, ProbeOutcome>,
}

impl ProbeReport {
    /// Build a report from raw outcomes.
    ///
    /// Test-only: the poll runner builds the struct directly from the map it is
    /// already holding, so this exists so adapter tests can hand-write a report
    /// without performing any I/O.
    #[cfg(test)]
    pub fn from_outcomes(outcomes: HashMap<&'static str, ProbeOutcome>) -> Self {
        ProbeReport { outcomes }
    }

    pub fn get(&self, id: &str) -> Option<&ProbeOutcome> {
        self.outcomes.get(id)
    }

    /// Body of a successful probe, if any.
    pub fn body(&self, id: &str) -> Option<&str> {
        self.get(id).and_then(|o| o.body())
    }

    /// `true` when the probe answered 2xx.
    pub fn is_ok(&self, id: &str) -> bool {
        self.get(id).map(|o| o.is_ok()).unwrap_or(false)
    }

    /// `true` when the server definitively lacks the endpoint (404/501).
    pub fn is_missing(&self, id: &str) -> bool {
        self.get(id).map(|o| o.is_missing()).unwrap_or(false)
    }
}

/// Attach an `Authorization: Bearer <key>` header when an API key is present.
pub fn auth(req: RequestBuilder, api_key: Option<&str>) -> RequestBuilder {
    match api_key {
        Some(key) if !key.is_empty() => req.bearer_auth(key),
        _ => req,
    }
}

/// Build the HTTP client for one poll.
///
/// NOTE: connection reuse is deliberately disabled.
///
/// Some llama.cpp builds return a spurious `404 Not Found` when a keep-alive
/// socket is reused (reproduced live: the same client + URL succeeded 5/10 times
/// with pooling and 10/10 with pooling off). Since we poll a handful of tiny
/// endpoints every few seconds, the cost of a fresh connection is negligible,
/// while a flaky 404 would silently blank the speeds and emit a misleading
/// "metrics disabled" diagnostic.
///
/// NOTE: for loopback servers proxies must be bypassed unconditionally.
/// A monitoring widget inherits whatever `HTTP_PROXY`/`HTTPS_PROXY` the parent
/// environment exports, and reqwest honours them even for loopback URLs — which
/// silently turns every poll into a CONNECT attempt against an unrelated proxy
/// (observed live: a sandbox-injected `http_proxy=http://127.0.0.1:60410` made
/// the health check never reach the llama.cpp server). A local monitor must
/// never depend on a proxy being correct, so `.no_proxy()` is explicit for
/// loopback. For remote and cloud servers we keep the system proxy instead.
pub fn build_client(settings: &Settings) -> anyhow::Result<reqwest::Client> {
    let mut builder = reqwest::Client::builder()
        .timeout(Duration::from_millis(settings.request_timeout_ms))
        .pool_max_idle_per_host(0);
    if settings
        .active_profile()
        .map(|p| p.is_loopback())
        .unwrap_or(true)
    {
        builder = builder.no_proxy();
    }
    Ok(builder.build()?)
}

/// Run every probe in order and classify each outcome.
///
/// A failing **required** probe aborts the whole poll with an error, which the
/// monitoring loop turns into a degraded state. Optional probes never abort:
/// their failure is visible only through the report.
pub async fn run_probes(
    client: &reqwest::Client,
    probes: &[Probe],
    api_key: Option<&str>,
) -> anyhow::Result<ProbeReport> {
    let mut outcomes = HashMap::new();

    for probe in probes {
        let outcome = match auth(client.get(&probe.url), api_key).send().await {
            Ok(resp) if resp.status().is_success() => match resp.text().await {
                Ok(text) => ProbeOutcome::Ok(text),
                Err(e) => ProbeOutcome::Transport(e.to_string()),
            },
            Ok(resp) => ProbeOutcome::Status(resp.status().as_u16()),
            Err(e) => ProbeOutcome::Transport(e.to_string()),
        };

        if probe.required {
            match &outcome {
                ProbeOutcome::Ok(_) => {}
                ProbeOutcome::Status(code) => anyhow::bail!("{} returned {code}", probe.id),
                ProbeOutcome::Transport(e) => anyhow::bail!("{} unreachable: {e}", probe.id),
            }
        }

        outcomes.insert(probe.id, outcome);
    }

    Ok(ProbeReport { outcomes })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(id: &'static str, outcome: ProbeOutcome) -> ProbeReport {
        let mut outcomes = HashMap::new();
        outcomes.insert(id, outcome);
        ProbeReport::from_outcomes(outcomes)
    }

    #[test]
    fn missing_detection_covers_the_definitive_cases_only() {
        assert!(ProbeOutcome::Status(501).is_missing());
        assert!(ProbeOutcome::Status(404).is_missing());
        // Transient conditions must NOT be reported as "endpoint missing",
        // otherwise every model reload would flash a misleading warning.
        assert!(!ProbeOutcome::Status(502).is_missing());
        assert!(!ProbeOutcome::Status(200).is_missing());
        assert!(!ProbeOutcome::Transport("connection refused".into()).is_missing());
        assert!(!ProbeOutcome::Ok(String::new()).is_missing());
    }

    #[test]
    fn report_helpers_read_outcomes() {
        let ok = report("metrics", ProbeOutcome::Ok("body".into()));
        assert!(ok.is_ok("metrics"));
        assert_eq!(ok.body("metrics"), Some("body"));
        assert!(!ok.is_missing("metrics"));

        let gone = report("metrics", ProbeOutcome::Status(501));
        assert!(!gone.is_ok("metrics"));
        assert_eq!(gone.body("metrics"), None);
        assert!(gone.is_missing("metrics"));

        // An unknown id is never "ok" and never "missing".
        assert!(!ok.is_ok("slots"));
        assert!(!ok.is_missing("slots"));
        assert_eq!(ok.body("slots"), None);
    }

    #[test]
    fn auth_adds_bearer_only_for_a_non_empty_key() {
        let req = || reqwest::Client::new().get("http://127.0.0.1:8080/health");
        assert!(auth(req(), Some("k"))
            .build()
            .unwrap()
            .headers()
            .contains_key("authorization"));
        assert!(!auth(req(), Some(""))
            .build()
            .unwrap()
            .headers()
            .contains_key("authorization"));
        assert!(!auth(req(), None)
            .build()
            .unwrap()
            .headers()
            .contains_key("authorization"));
    }
}
