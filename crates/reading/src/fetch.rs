//! The linked article, fetched only when you ask (the `L` key, or Later on
//! a link), through the same guards as a remote LLM endpoint
//! (`mxr_llm::is_loopback_endpoint`): no request leaves for this machine or
//! a private network, by its name, its address or a redirect.
//!
//! - Only `http(s)` URLs with a host and no userinfo; tracking parameters
//!   are stripped first.
//! - The host is resolved here and every address must be public; the
//!   request is pinned to the checked address, so a DNS answer can't
//!   change between the check and the connection.
//! - Redirects are never followed by the HTTP client. Each hop is checked
//!   like the first, at most `max_redirects` of them.
//! - No system proxy: a proxy would resolve the name itself and skip the
//!   check.
//! - The body is read up to `max_bytes` and refused past it; only HTML is
//!   accepted.
//!
//! Every host contacted is returned, so the client can say which sites
//! learned that you clicked.

use crate::urls::clean_url;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Duration;
use url::Url;

#[derive(Debug, Clone)]
pub struct FetchPolicy {
    pub max_bytes: usize,
    pub max_redirects: usize,
    pub timeout: Duration,
    pub user_agent: String,
    /// Tests only: 127.0.0.1 counts as public, so a local mock server can
    /// stand in for the web. Every other private range stays refused.
    #[cfg(test)]
    allow_loopback_for_tests: bool,
}

impl Default for FetchPolicy {
    fn default() -> Self {
        Self {
            max_bytes: 5 * 1024 * 1024,
            max_redirects: 5,
            timeout: Duration::from_secs(20),
            user_agent: "Mozilla/5.0 (compatible; mxr reader)".to_string(),
            #[cfg(test)]
            allow_loopback_for_tests: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FetchError {
    #[error("only web links can be fetched")]
    NotWeb,
    #[error("{0} is on this machine or a private network, so mxr won't fetch it")]
    PrivateAddress(String),
    #[error("couldn't find {0}")]
    Resolve(String),
    #[error("{0} redirected too many times")]
    TooManyRedirects(String),
    #[error("{0} redirected without saying where")]
    BadRedirect(String),
    #[error("the page is larger than {0} MB")]
    TooLarge(usize),
    #[error("{host} answered {status}")]
    Status { host: String, status: u16 },
    #[error("{host} sent {content_type}, not a web page")]
    NotHtml { host: String, content_type: String },
    #[error("couldn't reach {host}: {message}")]
    Http { host: String, message: String },
}

/// A fetched page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fetched {
    /// Where the page was in the end, after redirects.
    pub final_url: Url,
    /// Every host contacted, in order, without repeats.
    pub contacted: Vec<String>,
    pub html: String,
}

/// Whether an address is on the public internet: not loopback, private,
/// link-local, shared (CGNAT), documentation, benchmarking, multicast,
/// reserved or unspecified, including IPv4 inside IPv6.
pub fn is_public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => is_public_v4(v4),
        IpAddr::V6(v6) => {
            if let Some(v4) = v6.to_ipv4_mapped() {
                return is_public_v4(v4);
            }
            let segments = v6.segments();
            let unique_local = segments[0] & 0xfe00 == 0xfc00;
            let link_local = segments[0] & 0xffc0 == 0xfe80;
            let documentation = segments[0] == 0x2001 && segments[1] == 0x0db8;
            // RFC 8215 local-use NAT64 translates into whatever network the
            // operator chose, so nothing behind it counts as public.
            let local_nat64 = segments[..3] == [0x64, 0xff9b, 1];
            // Teredo hides the client's IPv4 address; never trust it.
            let teredo = segments[0] == 0x2001 && segments[1] == 0;
            !(v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_multicast()
                || unique_local
                || link_local
                || documentation
                || local_nat64
                || teredo
                || embedded_v4(&segments).is_some_and(|v4| !is_public_v4(v4)))
        }
    }
}

/// The IPv4 address an IPv6 address carries: the well-known NAT64 prefix
/// and the old IPv4-compatible form in the low 32 bits, 6to4 in bits 16 to
/// 48.
fn embedded_v4(segments: &[u16; 8]) -> Option<Ipv4Addr> {
    let v4 = |high: u16, low: u16| {
        Ipv4Addr::new((high >> 8) as u8, high as u8, (low >> 8) as u8, low as u8)
    };
    if segments[..6] == [0x64, 0xff9b, 0, 0, 0, 0] || segments[..6] == [0, 0, 0, 0, 0, 0] {
        Some(v4(segments[6], segments[7]))
    } else if segments[0] == 0x2002 {
        Some(v4(segments[1], segments[2]))
    } else {
        None
    }
}

fn is_public_v4(ip: Ipv4Addr) -> bool {
    let [a, b, c, _] = ip.octets();
    !(ip.is_loopback()
        || ip.is_private()
        || ip.is_link_local()
        || ip.is_broadcast()
        || ip.is_documentation()
        || ip.is_unspecified()
        || ip.is_multicast()
        || a == 0
        || (a == 100 && (64..128).contains(&b))
        || (a == 192 && b == 0 && c == 0)
        || (a == 198 && (18..20).contains(&b))
        || a >= 240)
}

/// The URL as it will be fetched, or why it won't be: web only, no
/// userinfo, tracking stripped, and never a loopback name.
pub fn checked_url(raw: &str) -> Result<Url, FetchError> {
    let url = clean_url(raw).ok_or(FetchError::NotWeb)?;
    if !url.username().is_empty() || url.password().is_some() {
        return Err(FetchError::NotWeb);
    }
    let host = url.host_str().ok_or(FetchError::NotWeb)?.to_string();
    if mxr_llm::is_loopback_endpoint(url.as_str())
        || host.eq_ignore_ascii_case("localhost")
        || host.to_ascii_lowercase().ends_with(".localhost")
        || host.to_ascii_lowercase().ends_with(".local")
        || host.to_ascii_lowercase().ends_with(".internal")
    {
        return Err(FetchError::PrivateAddress(host));
    }
    Ok(url)
}

impl FetchPolicy {
    fn check(&self, raw: &str) -> Result<Url, FetchError> {
        #[cfg(test)]
        if self.allow_loopback_for_tests {
            if let Some(url) = clean_url(raw).filter(|url| url.host_str() == Some("127.0.0.1")) {
                return Ok(url);
            }
        }
        checked_url(raw)
    }

    fn allows(&self, ip: IpAddr) -> bool {
        #[cfg(test)]
        if self.allow_loopback_for_tests && ip == IpAddr::V4(Ipv4Addr::LOCALHOST) {
            return true;
        }
        is_public_ip(ip)
    }

    /// The one address this hop may connect to.
    async fn resolve(&self, url: &Url) -> Result<SocketAddr, FetchError> {
        let host = url.host_str().ok_or(FetchError::NotWeb)?;
        let port = url.port_or_known_default().ok_or(FetchError::NotWeb)?;
        let addrs: Vec<SocketAddr> = match url.host() {
            Some(url::Host::Ipv4(ip)) => vec![SocketAddr::new(IpAddr::V4(ip), port)],
            Some(url::Host::Ipv6(ip)) => vec![SocketAddr::new(IpAddr::V6(ip), port)],
            _ => tokio::net::lookup_host((host, port))
                .await
                .map_err(|_| FetchError::Resolve(host.to_string()))?
                .collect(),
        };
        if addrs.is_empty() {
            return Err(FetchError::Resolve(host.to_string()));
        }
        // Every answer must be public: a name that also resolves to a
        // private address is refused, whichever one a retry would pick.
        if let Some(blocked) = addrs.iter().find(|addr| !self.allows(addr.ip())) {
            tracing::debug!(%host, ip = %blocked.ip(), "article fetch refused a private address");
            return Err(FetchError::PrivateAddress(host.to_string()));
        }
        Ok(addrs[0])
    }

    fn client(&self, url: &Url, addr: SocketAddr) -> Result<reqwest::Client, FetchError> {
        let host = url.host_str().unwrap_or_default().to_string();
        let mut builder = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .no_proxy()
            .timeout(self.timeout)
            .user_agent(self.user_agent.clone());
        if matches!(url.host(), Some(url::Host::Domain(_))) {
            builder = builder.resolve(&host, addr);
        }
        builder.build().map_err(|error| FetchError::Http {
            host,
            message: error.to_string(),
        })
    }
}

/// Fetch the page at `raw`, following at most `policy.max_redirects`
/// redirects, each checked like the first.
pub async fn fetch_page(raw: &str, policy: &FetchPolicy) -> Result<Fetched, FetchError> {
    let mut url = policy.check(raw)?;
    let mut contacted: Vec<String> = Vec::new();
    for _ in 0..=policy.max_redirects {
        let host = url.host_str().unwrap_or_default().to_ascii_lowercase();
        let addr = policy.resolve(&url).await?;
        if !contacted.contains(&host) {
            contacted.push(host.clone());
        }
        let response = policy
            .client(&url, addr)?
            .get(url.clone())
            .header(reqwest::header::ACCEPT, "text/html,application/xhtml+xml")
            .send()
            .await
            .map_err(|error| FetchError::Http {
                host: host.clone(),
                message: error.without_url().to_string(),
            })?;
        let status = response.status();
        if status.is_redirection() {
            let location = response
                .headers()
                .get(reqwest::header::LOCATION)
                .and_then(|value| value.to_str().ok())
                .and_then(|value| url.join(value).ok())
                .ok_or_else(|| FetchError::BadRedirect(host.clone()))?;
            url = policy.check(location.as_str())?;
            continue;
        }
        if !status.is_success() {
            return Err(FetchError::Status {
                host,
                status: status.as_u16(),
            });
        }
        // Only HTML is read. A declared type must be exactly text/html or
        // application/xhtml+xml; with none, the body itself must look like
        // HTML.
        let declared = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .map(|value| {
                value
                    .split(';')
                    .next()
                    .unwrap_or_default()
                    .trim()
                    .to_ascii_lowercase()
            });
        if let Some(content_type) = &declared {
            if !HTML_TYPES.contains(&content_type.as_str()) {
                return Err(FetchError::NotHtml {
                    host,
                    content_type: content_type.clone(),
                });
            }
        }
        let html = read_capped(response, policy.max_bytes, &host).await?;
        if declared.is_none() && !looks_like_html(&html) {
            return Err(FetchError::NotHtml {
                host,
                content_type: "no declared type, and not HTML".to_string(),
            });
        }
        return Ok(Fetched {
            final_url: url,
            contacted,
            html,
        });
    }
    Err(FetchError::TooManyRedirects(
        contacted.first().cloned().unwrap_or_default(),
    ))
}

const HTML_TYPES: &[&str] = &["text/html", "application/xhtml+xml"];

/// The start of a body with no declared type, as browsers sniff it: an
/// HTML doctype or one of the tags a page opens with.
fn looks_like_html(body: &str) -> bool {
    let start = body
        .trim_start_matches('\u{feff}')
        .trim_start()
        .chars()
        .take(64)
        .collect::<String>()
        .to_ascii_lowercase();
    [
        "<!doctype html",
        "<html",
        "<head",
        "<body",
        "<!--",
        "<p",
        "<div",
        "<article",
        "<title",
    ]
    .iter()
    .any(|tag| start.starts_with(tag))
}

async fn read_capped(
    mut response: reqwest::Response,
    max_bytes: usize,
    host: &str,
) -> Result<String, FetchError> {
    let too_large = FetchError::TooLarge(max_bytes / (1024 * 1024));
    if response
        .content_length()
        .is_some_and(|length| length > max_bytes as u64)
    {
        return Err(too_large);
    }
    let mut body: Vec<u8> = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|error| FetchError::Http {
        host: host.to_string(),
        message: error.without_url().to_string(),
    })? {
        if body.len() + chunk.len() > max_bytes {
            return Err(too_large);
        }
        body.extend_from_slice(&chunk);
    }
    Ok(String::from_utf8_lossy(&body).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn local_policy() -> FetchPolicy {
        FetchPolicy {
            allow_loopback_for_tests: true,
            ..FetchPolicy::default()
        }
    }

    #[test]
    fn private_and_reserved_addresses_are_not_public() {
        for ip in [
            "127.0.0.1",
            "10.1.2.3",
            "172.16.0.1",
            "192.168.1.1",
            "169.254.169.254",
            "100.64.0.1",
            "0.0.0.0",
            "255.255.255.255",
            "224.0.0.1",
            "198.18.0.1",
            "192.0.2.1",
            "::1",
            "::",
            "fc00::1",
            "fd12::1",
            "fe80::1",
            "::ffff:10.0.0.1",
            "::ffff:127.0.0.1",
            "64:ff9b::a00:1",
            "2001:db8::1",
            // RFC 8215 local-use NAT64, whatever it embeds.
            "64:ff9b:1::5db8:d822",
            "64:ff9b:1:abcd::1",
            // 6to4 carrying a private, loopback or link-local address.
            "2002:a00:1::1",
            "2002:7f00:1::1",
            "2002:a9fe:a9fe::1",
            "2002:c0a8:101::1",
            // Teredo: the client address is obfuscated, so never trusted.
            "2001:0:4136:e378:8000:63bf:3fff:fdd2",
        ] {
            let ip: IpAddr = ip.parse().expect(ip);
            assert!(!is_public_ip(ip), "{ip} should be private");
        }
        for ip in [
            "93.184.216.34",
            "1.1.1.1",
            "2606:4700:4700::1111",
            "64:ff9b::5db8:d822",
            "2002:5db8:d822::1",
        ] {
            let ip: IpAddr = ip.parse().expect(ip);
            assert!(is_public_ip(ip), "{ip} should be public");
        }
    }

    #[test]
    fn loopback_names_userinfo_and_non_web_links_are_refused_before_any_lookup() {
        for raw in [
            "http://localhost:8080/x",
            "http://LOCALHOST/x",
            "http://printer.local/x",
            "http://api.localhost/x",
            "http://metadata.internal/x",
            "http://127.0.0.1/x",
            "http://[::1]/x",
        ] {
            assert!(
                matches!(checked_url(raw), Err(FetchError::PrivateAddress(_))),
                "{raw}"
            );
        }
        for raw in [
            "http://user:pw@example.com/x",
            "ftp://example.com/x",
            "file:///etc/passwd",
        ] {
            assert_eq!(checked_url(raw), Err(FetchError::NotWeb), "{raw}");
        }
        assert_eq!(
            checked_url("https://example.com/a?utm_source=x&id=1").map(|u| u.to_string()),
            Ok("https://example.com/a?id=1".to_string())
        );
    }

    #[tokio::test]
    async fn a_private_address_is_refused_by_default() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_string("<p>hi</p>"))
            .expect(0)
            .mount(&server)
            .await;
        let err = fetch_page(&format!("{}/post", server.uri()), &FetchPolicy::default())
            .await
            .expect_err("loopback is refused");
        assert!(matches!(err, FetchError::PrivateAddress(_)), "{err}");
    }

    #[tokio::test]
    async fn a_page_is_fetched_with_its_hosts_named() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/post"))
            .respond_with(ResponseTemplate::new(200).set_body_raw(
                "<html><body><p>Hello</p></body></html>",
                "text/html; charset=utf-8",
            ))
            .mount(&server)
            .await;
        let got = fetch_page(
            &format!("{}/post?utm_source=mail", server.uri()),
            &local_policy(),
        )
        .await
        .expect("fetched");
        assert!(got.html.contains("Hello"));
        assert_eq!(got.contacted, ["127.0.0.1"]);
        assert_eq!(got.final_url.query(), None, "tracking stripped");
    }

    #[tokio::test]
    async fn a_redirect_to_a_private_network_is_never_followed() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/r"))
            .respond_with(
                ResponseTemplate::new(302).insert_header("location", "http://10.0.0.7/admin"),
            )
            .mount(&server)
            .await;
        let err = fetch_page(&format!("{}/r", server.uri()), &local_policy())
            .await
            .expect_err("refused");
        assert_eq!(err, FetchError::PrivateAddress("10.0.0.7".to_string()));
        let metadata = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(
                ResponseTemplate::new(301)
                    .insert_header("location", "http://169.254.169.254/latest/meta-data"),
            )
            .mount(&metadata)
            .await;
        let err = fetch_page(&metadata.uri(), &local_policy())
            .await
            .expect_err("refused");
        assert!(matches!(err, FetchError::PrivateAddress(_)));
    }

    #[tokio::test]
    async fn redirects_are_followed_hop_by_hop_up_to_the_limit() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/a"))
            .respond_with(ResponseTemplate::new(302).insert_header("location", "/b?utm_medium=x"))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/b"))
            .respond_with(ResponseTemplate::new(200).set_body_raw("<p>landed</p>", "text/html"))
            .mount(&server)
            .await;
        let got = fetch_page(&format!("{}/a", server.uri()), &local_policy())
            .await
            .expect("followed");
        assert_eq!(got.final_url.path(), "/b");
        assert!(got.html.contains("landed"));

        Mock::given(method("GET"))
            .and(path("/loop"))
            .respond_with(ResponseTemplate::new(302).insert_header("location", "/loop"))
            .mount(&server)
            .await;
        let err = fetch_page(&format!("{}/loop", server.uri()), &local_policy())
            .await
            .expect_err("loops stop");
        assert!(matches!(err, FetchError::TooManyRedirects(_)));
    }

    #[tokio::test]
    async fn only_html_types_pass_and_a_missing_type_is_sniffed() {
        let server = MockServer::start().await;
        let page = |route: &'static str, body: &'static str, content_type: Option<&'static str>| {
            let mut response = ResponseTemplate::new(200).set_body_bytes(body.as_bytes());
            if let Some(content_type) = content_type {
                response = response.insert_header("content-type", content_type);
            }
            Mock::given(method("GET"))
                .and(path(route))
                .respond_with(response)
        };
        page(
            "/xhtml",
            "<html><body><p>x</p></body></html>",
            Some("application/xhtml+xml; charset=utf-8"),
        )
        .mount(&server)
        .await;
        page(
            "/sneaky",
            "{\"html\": true}",
            Some("application/vnd.html-ish+json"),
        )
        .mount(&server)
        .await;
        page(
            "/untyped-html",
            "  <!DOCTYPE html><html><p>hi</p></html>",
            None,
        )
        .mount(&server)
        .await;
        page("/untyped-pdf", "%PDF-1.7 binary", None)
            .mount(&server)
            .await;
        let policy = local_policy();
        let url = |route: &str| format!("{}{route}", server.uri());
        assert!(fetch_page(&url("/xhtml"), &policy).await.is_ok());
        assert!(fetch_page(&url("/untyped-html"), &policy).await.is_ok());
        assert!(matches!(
            fetch_page(&url("/sneaky"), &policy).await,
            Err(FetchError::NotHtml { .. })
        ));
        assert!(matches!(
            fetch_page(&url("/untyped-pdf"), &policy).await,
            Err(FetchError::NotHtml { .. })
        ));
    }

    #[tokio::test]
    async fn a_page_over_the_size_cap_or_not_html_is_refused() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/big"))
            .respond_with(
                ResponseTemplate::new(200).set_body_raw("x".repeat(2 * 1024 * 1024), "text/html"),
            )
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/pdf"))
            .respond_with(ResponseTemplate::new(200).set_body_raw("%PDF-1.7", "application/pdf"))
            .mount(&server)
            .await;
        let policy = FetchPolicy {
            max_bytes: 1024 * 1024,
            ..local_policy()
        };
        let err = fetch_page(&format!("{}/big", server.uri()), &policy)
            .await
            .expect_err("too big");
        assert_eq!(err, FetchError::TooLarge(1));
        let err = fetch_page(&format!("{}/pdf", server.uri()), &policy)
            .await
            .expect_err("not html");
        assert!(matches!(err, FetchError::NotHtml { .. }));
    }
}
