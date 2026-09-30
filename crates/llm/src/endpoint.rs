//! Where an LLM request may go. Prompts can carry the user's mail and
//! history, so an endpoint counts as on this machine only when its host is
//! provably loopback, and the HTTP client never lets a request leave by
//! another route: no redirects for any endpoint (a 307/308 would resend the
//! POST body, prompt and all, to whatever host it names), and no system
//! proxy for a loopback one (`HTTP_PROXY` without a matching `NO_PROXY`
//! would carry "local" traffic off the machine).

use std::time::Duration;

/// Whether an LLM endpoint is on this machine: an http(s) URL whose host is
/// exactly `localhost`, a 127.0.0.0/8 address or `::1`, with no userinfo.
/// Anything else, unparseable included, counts as remote: a prefix check
/// would call `http://localhost@evil.example` or `http://localhost.evil.com`
/// local.
pub fn is_loopback_endpoint(base_url: &str) -> bool {
    let Ok(url) = url::Url::parse(base_url.trim()) else {
        return false;
    };
    if !matches!(url.scheme(), "http" | "https")
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return false;
    }
    match url.host() {
        // The parser lowercases domains; `localhost.` and `*.localhost` stay
        // remote, since only the exact name is sure to be loopback.
        Some(url::Host::Domain(domain)) => domain == "localhost",
        Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
        Some(url::Host::Ipv6(ip)) => {
            ip.is_loopback() || ip.to_ipv4_mapped().is_some_and(|ip| ip.is_loopback())
        }
        None => false,
    }
}

/// The HTTP client for one endpoint: never follows redirects, and skips
/// system proxies when the endpoint is loopback. An error here means TLS or
/// the platform couldn't start; the provider reports it on each request
/// rather than falling back to a client without these guards.
pub(crate) fn http_client(base_url: &str, timeout: Duration) -> Result<reqwest::Client, String> {
    let mut builder = reqwest::Client::builder()
        .timeout(timeout)
        .redirect(reqwest::redirect::Policy::none());
    if is_loopback_endpoint(base_url) {
        builder = builder.no_proxy();
    }
    builder
        .build()
        .map_err(|error| format!("couldn't start the HTTP client for the LLM: {error}"))
}

#[cfg(test)]
mod tests {
    use super::is_loopback_endpoint;

    #[test]
    fn only_a_loopback_host_counts_as_a_local_llm_endpoint() {
        let local = [
            "http://localhost:11434/v1",
            "HTTP://LOCALHOST:11434/v1",
            "https://localhost/v1",
            "http://localhost",
            "  http://localhost:1234/v1  ",
            "http://127.0.0.1:11434/v1",
            "http://127.1.2.3/v1",
            "http://[::1]:11434/v1",
            "http://[0:0:0:0:0:0:0:1]/v1",
            "http://[::ffff:127.0.0.1]/v1",
        ];
        for url in local {
            assert!(is_loopback_endpoint(url), "{url} should be local");
        }
        let remote = [
            "http://localhost@evil.example/v1",
            "http://user:pw@localhost:11434/v1",
            "http://localhost.evil.com/v1",
            "http://localhost./v1",
            "http://ollama.localhost/v1",
            "http://evil.example/localhost",
            "http://127.0.0.1.evil.example/v1",
            "http://0.0.0.0:11434/v1",
            "http://192.168.1.20:11434/v1",
            "http://10.0.0.5/v1",
            "http://[::]/v1",
            "http://[fe80::1]/v1",
            "https://api.openai.com/v1",
            "ftp://localhost/v1",
            "unix:///tmp/ollama.sock",
            "localhost:11434",
            "not a url",
            "",
        ];
        for url in remote {
            assert!(!is_loopback_endpoint(url), "{url} should be remote");
        }
    }
}
