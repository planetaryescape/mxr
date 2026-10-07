//! Links as Reading shows and fetches them: tracking parameters stripped,
//! a redirect that names its target unwrapped, and click trackers marked so
//! the domain shown is never mistaken for the article's.

use url::Url;

/// Query parameters that only identify the click, never the page.
const TRACKING_PARAMS: &[&str] = &[
    "fbclid",
    "gclid",
    "dclid",
    "msclkid",
    "yclid",
    "igshid",
    "mc_cid",
    "mc_eid",
    "_hsenc",
    "_hsmi",
    "mkt_tok",
    "ref_src",
    "oly_enc_id",
    "oly_anon_id",
    "vero_id",
    "vero_conv",
    "ck_subscriber_id",
    "sc_cid",
    "__s",
    "_kx",
    "trk",
    "trkcampaign",
    "elqtrackid",
    "elqtrack",
];

/// Parameters a redirect uses to carry its destination.
const REDIRECT_PARAMS: &[&str] = &[
    "url",
    "u",
    "q",
    "target",
    "redirect",
    "redirect_url",
    "redirect_uri",
    "dest",
    "destination",
    "link",
];

/// An `http(s)` link with tracking parameters removed and, where the
/// redirect carries its target in the query, the target instead. `None`
/// for anything else (`mailto:`, `javascript:`, fragments, unparseable).
pub fn clean_url(raw: &str) -> Option<Url> {
    let url = parse_http(raw)?;
    let url = unwrap_redirect(url);
    Some(strip_tracking(url))
}

fn parse_http(raw: &str) -> Option<Url> {
    let url = Url::parse(raw.trim()).ok()?;
    matches!(url.scheme(), "http" | "https")
        .then_some(url)
        .filter(|url| url.host_str().is_some())
}

/// One level of unwrapping: a click tracker that names its destination in
/// a query parameter (Google's `/url?q=`, Outlook safe links, many ESPs).
fn unwrap_redirect(url: Url) -> Url {
    let target = url
        .query_pairs()
        .filter(|(key, _)| REDIRECT_PARAMS.contains(&key.to_ascii_lowercase().as_str()))
        .find_map(|(_, value)| parse_http(&value));
    target.unwrap_or(url)
}

fn strip_tracking(mut url: Url) -> Url {
    let kept: Vec<(String, String)> = url
        .query_pairs()
        .filter(|(key, _)| {
            let key = key.to_ascii_lowercase();
            !key.starts_with("utm_") && !TRACKING_PARAMS.contains(&key.as_str())
        })
        .map(|(key, value)| (key.into_owned(), value.into_owned()))
        .collect();
    if kept.is_empty() {
        url.set_query(None);
    } else {
        url.query_pairs_mut().clear().extend_pairs(kept);
    }
    url
}

/// The host as people say it: `www.` dropped, lowercase.
pub fn display_domain(url: &Url) -> String {
    let host = url.host_str().unwrap_or_default().to_ascii_lowercase();
    host.strip_prefix("www.").unwrap_or(&host).to_string()
}

/// A click tracker whose destination can't be read without following it:
/// Substack, beehiiv, Mailchimp, ConvertKit, SendGrid and the like. The
/// domain shown for these is the tracker's, so clients say "via".
pub fn is_click_tracker(url: &Url) -> bool {
    let host = url.host_str().unwrap_or_default().to_ascii_lowercase();
    let path = url.path().to_ascii_lowercase();
    const HOST_PREFIXES: &[&str] = &[
        "click.", "clicks.", "link.", "links.", "track.", "trk.", "email.", "e.", "go.", "l.",
        "lnk.", "r.", "url",
    ];
    const HOSTS: &[&str] = &[
        "t.co",
        "lnkd.in",
        "bit.ly",
        "mailchi.mp",
        "ow.ly",
        "buff.ly",
        "tinyurl.com",
        "cl.exct.net",
    ];
    const HOST_SUFFIXES: &[&str] = &[
        ".list-manage.com",
        ".ct.sendgrid.net",
        ".hubspotlinks.com",
        ".awstrack.me",
        ".convertkit-mail.com",
        ".convertkit-mail2.com",
        ".mlsend.com",
        ".beehiiv.com",
        ".rs6.net",
        ".mcsv.net",
    ];
    const PATHS: &[&str] = &[
        "/redirect",
        "/track/click",
        "/ss/c/",
        "/ls/click",
        "/cl0/",
        "/c/",
        "/click",
    ];
    HOSTS.contains(&host.as_str())
        || HOST_SUFFIXES.iter().any(|suffix| host.ends_with(suffix))
        || (HOST_PREFIXES.iter().any(|prefix| host.starts_with(prefix)) && opaque_token(url.path()))
        || PATHS.iter().any(|prefix| path.starts_with(prefix)) && opaque_token(url.path())
}

/// A path segment that reads as an id, not words: sixteen or more
/// characters with at least four digits ("u001.OYBRJNT0z5pe6Uzq",
/// "ed69ce43-9ba2-4eb6"), unlike "local-first-mail".
fn opaque_token(path: &str) -> bool {
    path.split('/').any(|segment| {
        segment.len() >= 16 && segment.chars().filter(char::is_ascii_digit).count() >= 4
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clean(raw: &str) -> String {
        clean_url(raw).map(|u| u.to_string()).unwrap_or_default()
    }

    #[test]
    fn tracking_parameters_go_and_the_page_stays() {
        assert_eq!(
            clean("https://example.com/post?utm_source=news&utm_medium=email&id=4&fbclid=x"),
            "https://example.com/post?id=4"
        );
        assert_eq!(
            clean("https://example.com/a?mc_cid=1&mc_eid=2"),
            "https://example.com/a"
        );
        assert_eq!(clean("https://example.com/a#part"), "https://example.com/a#part");
    }

    #[test]
    fn a_redirect_that_names_its_target_is_unwrapped() {
        assert_eq!(
            clean("https://www.google.com/url?q=https://sqlite.org/releaselog/3_51.html&sa=D"),
            "https://sqlite.org/releaselog/3_51.html"
        );
        assert_eq!(
            clean("https://eur01.safelinks.protection.outlook.com/?url=https%3A%2F%2Fexample.org%2Fx%3Futm_source%3Da&data=1"),
            "https://example.org/x"
        );
    }

    #[test]
    fn only_web_links_are_kept() {
        for raw in ["mailto:a@b.c", "javascript:alert(1)", "#top", "ftp://x.y/z", "not a url"] {
            assert!(clean_url(raw).is_none(), "{raw}");
        }
    }

    #[test]
    fn opaque_trackers_are_marked_and_plain_links_are_not() {
        let tracked = [
            "https://substack.com/redirect/ed69ce43-9ba2-4eb6-a359-e1a2b5281f8f?j=abc",
            "https://link.mail.beehiiv.com/ss/c/u001.OYBRJNT0z5pe6Uzq-udvEq",
            "https://example.us7.list-manage.com/track/click?u=1&id=2&e=3",
            "https://t.co/abc123",
        ];
        for raw in tracked {
            let url = Url::parse(raw).expect(raw);
            assert!(is_click_tracker(&url), "{raw}");
        }
        for raw in [
            "https://sqlite.org/releaselog/3_51.html",
            "https://www.example.com/2026/10/local-first-mail",
            "https://blog.rust-lang.org/2026/10/01/Rust-1.95.html",
            "https://links.demo.mxr.local/articles/local-first-mail",
            "https://link.example.com/2026/10/why-sync-engines-need-tombstones",
        ] {
            let url = Url::parse(raw).expect(raw);
            assert!(!is_click_tracker(&url), "{raw}");
        }
    }

    #[test]
    fn display_domain_drops_www() {
        let url = Url::parse("https://WWW.Example.com/x").expect("url");
        assert_eq!(display_domain(&url), "example.com");
    }
}
