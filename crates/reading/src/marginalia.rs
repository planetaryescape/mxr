//! Marginalia, a separate reading app: send it an item, read the highlights
//! made on it back. mxr knows about Marginalia; Marginalia's API is general
//! and knows nothing about mxr.
//!
//! The API takes a personal token (`mrg_…`) as a bearer token:
//! `POST /api/v1/articles` adds an article, idempotent on its `externalId`,
//! and `GET /api/v1/articles/{id}/highlights` lists its highlights. Nothing
//! here runs unless the user connected Marginalia and asked.

use crate::extract::{Paragraph, ParagraphKind};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use url::Url;

/// Marginalia refuses larger requests.
pub const MAX_REQUEST_BYTES: usize = 2_000_000;
const TIMEOUT: Duration = Duration::from_secs(30);
/// Never an article: a GET on it answers 404 with a good token and 401
/// with a bad one, so connecting can check the token without side effects.
const PROBE_ARTICLE_ID: &str = "mxr-connection-check";

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum MarginaliaError {
    #[error("{0}")]
    InvalidUrl(String),
    #[error("Marginalia didn't accept the token: create a new one under Account, Connected apps")]
    Unauthorized,
    #[error("Marginalia has no such article; it may have been deleted there")]
    NotFound,
    #[error("this item is too large to send to Marginalia")]
    TooLarge,
    #[error("Marginalia is busy: too many requests in the last minute; try again shortly")]
    RateLimited,
    #[error("couldn't reach {host}: {message}")]
    Http { host: String, message: String },
    #[error("Marginalia answered {status}: {message}")]
    Server { status: u16, message: String },
    #[error("Marginalia's answer wasn't understood: {0}")]
    Decode(String),
}

/// The site, as entered: `https://reader.example.com`. Only `https`, except
/// on this machine, where `http` serves a local copy. The path is dropped.
pub fn normalize_base_url(input: &str) -> Result<Url, MarginaliaError> {
    let mut url = Url::parse(input.trim())
        .map_err(|_| MarginaliaError::InvalidUrl(format!("\"{input}\" isn't a web address")))?;
    let local = matches!(
        url.host_str(),
        Some("localhost" | "127.0.0.1" | "[::1]" | "::1")
    );
    match url.scheme() {
        "https" => {}
        "http" if local => {}
        _ => {
            return Err(MarginaliaError::InvalidUrl(
                "Marginalia's address must start with https://".into(),
            ))
        }
    }
    if url.host_str().is_none_or(str::is_empty) {
        return Err(MarginaliaError::InvalidUrl(format!(
            "\"{input}\" has no host"
        )));
    }
    url.set_path("/");
    url.set_query(None);
    url.set_fragment(None);
    Ok(url)
}

/// An article to add, its HTML already extracted.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArticleIn {
    pub external_id: String,
    pub title: String,
    pub html: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    /// Milliseconds since the Unix epoch.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub published_at: Option<i64>,
}

impl ArticleIn {
    /// The request body's size, to check before anything is sent.
    pub fn request_bytes(&self) -> usize {
        serde_json::to_vec(self).map_or(usize::MAX, |body| body.len())
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SentArticle {
    pub article_id: String,
    pub read_url: String,
    /// False when Marginalia already had it from an earlier send.
    pub created: bool,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Highlight {
    pub id: String,
    /// `highlight` or `underline`.
    pub kind: String,
    pub text: String,
    pub paragraph_index: Option<u32>,
    /// Milliseconds since the Unix epoch.
    pub created_at: i64,
}

#[derive(Debug, Deserialize)]
struct HighlightsOut {
    highlights: Vec<Highlight>,
}

#[derive(Debug, Deserialize)]
struct ErrorOut {
    #[serde(default)]
    error: Option<String>,
}

pub struct MarginaliaClient {
    base: Url,
    token: String,
    http: reqwest::Client,
}

impl MarginaliaClient {
    pub fn new(base_url: &str, token: &str) -> Result<Self, MarginaliaError> {
        let base = normalize_base_url(base_url)?;
        let token = token.trim();
        if token.is_empty() {
            return Err(MarginaliaError::Unauthorized);
        }
        let http = reqwest::Client::builder()
            .timeout(TIMEOUT)
            .user_agent(concat!("mxr/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|error| MarginaliaError::Http {
                host: base.host_str().unwrap_or_default().to_string(),
                message: error.to_string(),
            })?;
        Ok(Self {
            base,
            token: token.to_string(),
            http,
        })
    }

    pub fn host(&self) -> String {
        self.base.host_str().unwrap_or_default().to_string()
    }

    fn endpoint(&self, path: &str) -> Result<Url, MarginaliaError> {
        self.base
            .join(path)
            .map_err(|error| MarginaliaError::InvalidUrl(error.to_string()))
    }

    fn http_error(&self, error: &reqwest::Error) -> MarginaliaError {
        MarginaliaError::Http {
            host: self.host(),
            // without_url: the request URL can carry an article ID
            message: error.to_string(),
        }
    }

    async fn status_error(response: reqwest::Response) -> MarginaliaError {
        let status = response.status().as_u16();
        match status {
            401 | 403 => MarginaliaError::Unauthorized,
            404 => MarginaliaError::NotFound,
            413 => MarginaliaError::TooLarge,
            429 => MarginaliaError::RateLimited,
            _ => {
                let message = response
                    .json::<ErrorOut>()
                    .await
                    .ok()
                    .and_then(|body| body.error)
                    .unwrap_or_else(|| "no explanation".to_string());
                MarginaliaError::Server { status, message }
            }
        }
    }

    /// Checks the token without changing anything in the library.
    pub async fn verify(&self) -> Result<(), MarginaliaError> {
        match self.highlights(PROBE_ARTICLE_ID).await {
            Ok(_) | Err(MarginaliaError::NotFound) => Ok(()),
            Err(error) => Err(error),
        }
    }

    pub async fn send(&self, article: &ArticleIn) -> Result<SentArticle, MarginaliaError> {
        if article.request_bytes() > MAX_REQUEST_BYTES {
            return Err(MarginaliaError::TooLarge);
        }
        let response = self
            .http
            .post(self.endpoint("api/v1/articles")?)
            .bearer_auth(&self.token)
            .json(article)
            .send()
            .await
            .map_err(|error| self.http_error(&error.without_url()))?;
        if !response.status().is_success() {
            return Err(Self::status_error(response).await);
        }
        let sent = response
            .json::<SentArticle>()
            .await
            .map_err(|error| MarginaliaError::Decode(error.without_url().to_string()))?;
        self.check_read_url(&sent.read_url)?;
        Ok(sent)
    }

    /// The read URL is opened in the user's browser or desktop, so a wrong
    /// or compromised server must not be able to hand back a `javascript:`,
    /// `file:` or other-site address. `Url::origin` compares scheme, host
    /// and port with case and default ports normalised.
    fn check_read_url(&self, read_url: &str) -> Result<(), MarginaliaError> {
        let same_site = Url::parse(read_url).is_ok_and(|url| {
            matches!(url.scheme(), "http" | "https") && url.origin() == self.base.origin()
        });
        if same_site {
            Ok(())
        } else {
            Err(MarginaliaError::Decode(
                "the reading address isn't on the connected Marginalia site".into(),
            ))
        }
    }

    pub async fn highlights(&self, article_id: &str) -> Result<Vec<Highlight>, MarginaliaError> {
        let mut url = self.endpoint("api/v1/articles/")?;
        url.path_segments_mut()
            .map_err(|()| MarginaliaError::InvalidUrl("Marginalia's address".into()))?
            .pop_if_empty()
            .push(article_id)
            .push("highlights");
        let response = self
            .http
            .get(url)
            .bearer_auth(&self.token)
            .send()
            .await
            .map_err(|error| self.http_error(&error.without_url()))?;
        if !response.status().is_success() {
            return Err(Self::status_error(response).await);
        }
        response
            .json::<HighlightsOut>()
            .await
            .map(|out| out.highlights)
            .map_err(|error| MarginaliaError::Decode(error.without_url().to_string()))
    }
}

fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(c),
        }
    }
    out
}

/// Reader text as plain HTML, for an issue whose own HTML the cleaner
/// couldn't keep.
pub fn paragraphs_html(paragraphs: &[Paragraph]) -> String {
    let mut out = String::new();
    let mut in_list = false;
    for paragraph in paragraphs {
        let is_item = paragraph.kind == ParagraphKind::ListItem;
        if is_item && !in_list {
            out.push_str("<ul>");
        } else if !is_item && in_list {
            out.push_str("</ul>");
        }
        in_list = is_item;
        let text = escape(&paragraph.text);
        match paragraph.kind {
            ParagraphKind::Heading => out.push_str(&format!("<h2>{text}</h2>")),
            ParagraphKind::Text => out.push_str(&format!("<p>{text}</p>")),
            ParagraphKind::ListItem => out.push_str(&format!("<li>{text}</li>")),
            ParagraphKind::Quote => out.push_str(&format!("<blockquote>{text}</blockquote>")),
        }
    }
    if in_list {
        out.push_str("</ul>");
    }
    out
}

#[cfg(test)]
mod tests {
    #![expect(clippy::unwrap_used, reason = "tests unwrap fixtures")]

    use super::*;
    use wiremock::matchers::{body_partial_json, header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn article() -> ArticleIn {
        ArticleIn {
            external_id: "mxr:abc:0".into(),
            title: "The quiet death of the three-pane layout".into(),
            html: "<p>Why every reader shipped the same window.</p>".into(),
            url: Some("https://example.com/three-pane".into()),
            author: Some("Long Reads Weekly".into()),
            published_at: Some(1_760_000_000_000),
        }
    }

    #[test]
    fn base_urls_need_https_except_on_this_machine() {
        assert_eq!(
            normalize_base_url("https://reader.example.com/read/abc?x=1")
                .unwrap()
                .as_str(),
            "https://reader.example.com/"
        );
        assert!(normalize_base_url("http://localhost:3000").is_ok());
        assert!(normalize_base_url("http://127.0.0.1:3000").is_ok());
        for bad in [
            "http://reader.example.com",
            "ftp://reader.example.com",
            "reader.example.com",
            "",
        ] {
            assert!(normalize_base_url(bad).is_err(), "{bad}");
        }
    }

    #[tokio::test]
    async fn sends_the_article_with_the_token() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v1/articles"))
            .and(header("authorization", "Bearer mrg_test"))
            .and(body_partial_json(serde_json::json!({
                "externalId": "mxr:abc:0",
                "title": "The quiet death of the three-pane layout",
                "url": "https://example.com/three-pane",
                "publishedAt": 1_760_000_000_000_i64,
            })))
            .respond_with(ResponseTemplate::new(201).set_body_json(serde_json::json!({
                "articleId": "k57",
                "readUrl": format!("{}/read/k57", server.uri()),
                "created": true,
            })))
            .expect(1)
            .mount(&server)
            .await;
        let client = MarginaliaClient::new(&server.uri(), "mrg_test").unwrap();
        assert_eq!(
            client.send(&article()).await.unwrap(),
            SentArticle {
                article_id: "k57".into(),
                read_url: format!("{}/read/k57", server.uri()),
                created: true,
            }
        );
    }

    async fn send_answering(
        read_url: &str,
        server: &MockServer,
    ) -> Result<SentArticle, MarginaliaError> {
        Mock::given(method("POST"))
            .and(path("/api/v1/articles"))
            .respond_with(ResponseTemplate::new(201).set_body_json(serde_json::json!({
                "articleId": "k57",
                "readUrl": read_url,
                "created": true,
            })))
            .mount(server)
            .await;
        MarginaliaClient::new(&server.uri(), "mrg_test")
            .unwrap()
            .send(&article())
            .await
    }

    #[tokio::test]
    async fn a_read_url_off_the_connected_site_is_refused() {
        // One fresh server per case: the mock answers whatever was mounted first.
        for case in 0..10 {
            let server = MockServer::start().await;
            let port = Url::parse(&server.uri()).unwrap().port().unwrap();
            let bad = match case {
                0 => "javascript:alert(1)".to_string(),
                1 => "data:text/html,hi".to_string(),
                2 => "file:///etc/passwd".to_string(),
                3 => "https://evil.example/read/x".to_string(),
                // Same host, other scheme or port: a different origin.
                4 => format!("https://127.0.0.1:{port}/read/x"),
                5 => format!("http://127.0.0.1:{}/read/x", port.wrapping_add(1)),
                // Lookalikes that string-prefix checks would let through.
                6 => format!("http://127.0.0.1:{port}@evil.example/read/x"),
                7 => format!("http://127.0.0.1.evil.example:{port}/read/x"),
                8 => "/read/x".to_string(),
                _ => String::new(),
            };
            assert!(
                matches!(
                    send_answering(&bad, &server).await,
                    Err(MarginaliaError::Decode(_))
                ),
                "{bad}"
            );
        }
    }

    #[tokio::test]
    async fn a_read_url_on_the_connected_origin_is_accepted_however_it_is_spelled() {
        let server = MockServer::start().await;
        let port = Url::parse(&server.uri()).unwrap().port().unwrap();
        let sent = send_answering(&format!("HTTP://127.0.0.1:{port}/read/x?a=1#b"), &server)
            .await
            .unwrap();
        assert_eq!(sent.article_id, "k57");
    }

    #[test]
    fn origins_ignore_case_and_default_ports_but_not_trailing_dots() {
        let client = MarginaliaClient::new("https://Reader.Example.com", "t").unwrap();
        assert!(client
            .check_read_url("https://reader.example.com/r/1")
            .is_ok());
        assert!(client
            .check_read_url("HTTPS://READER.EXAMPLE.COM:443/r/1")
            .is_ok());
        assert!(client
            .check_read_url("https://reader.example.com:8443/r/1")
            .is_err());
        assert!(client
            .check_read_url("http://reader.example.com/r/1")
            .is_err());
        // Strict: a rooted spelling is a different string the server could
        // not need, so refuse it rather than reason about DNS.
        assert!(client
            .check_read_url("https://reader.example.com./r/1")
            .is_err());
    }

    #[tokio::test]
    async fn reads_highlights_back() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/articles/k57/highlights"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "articleId": "k57",
                "title": "t",
                "highlights": [{
                    "id": "h1", "kind": "highlight", "text": "the same window",
                    "paragraphIndex": 2, "createdAt": 1_760_000_000_000_i64,
                }],
            })))
            .mount(&server)
            .await;
        let client = MarginaliaClient::new(&server.uri(), "mrg_test").unwrap();
        let highlights = client.highlights("k57").await.unwrap();
        assert_eq!(highlights.len(), 1);
        assert_eq!(highlights[0].text, "the same window");
        assert_eq!(highlights[0].paragraph_index, Some(2));
    }

    #[tokio::test]
    async fn verify_reads_401_as_a_bad_token_and_404_as_a_good_one() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(header("authorization", "Bearer mrg_good"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(401))
            .mount(&server)
            .await;
        let good = MarginaliaClient::new(&server.uri(), "mrg_good").unwrap();
        let bad = MarginaliaClient::new(&server.uri(), "mrg_bad").unwrap();
        assert_eq!(good.verify().await, Ok(()));
        assert_eq!(bad.verify().await, Err(MarginaliaError::Unauthorized));
    }

    #[tokio::test]
    async fn refuses_an_oversized_article_before_sending() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(201))
            .expect(0)
            .mount(&server)
            .await;
        let client = MarginaliaClient::new(&server.uri(), "mrg_test").unwrap();
        let mut big = article();
        big.html = "x".repeat(MAX_REQUEST_BYTES);
        assert_eq!(client.send(&big).await, Err(MarginaliaError::TooLarge));
    }

    #[test]
    fn reader_text_becomes_escaped_html() {
        let html = paragraphs_html(&[
            Paragraph {
                kind: ParagraphKind::Heading,
                text: "A & B".into(),
            },
            Paragraph {
                kind: ParagraphKind::ListItem,
                text: "one".into(),
            },
            Paragraph {
                kind: ParagraphKind::ListItem,
                text: "<two>".into(),
            },
            Paragraph {
                kind: ParagraphKind::Text,
                text: "end".into(),
            },
        ]);
        assert_eq!(
            html,
            "<h2>A &amp; B</h2><ul><li>one</li><li>&lt;two&gt;</li></ul><p>end</p>"
        );
    }
}
