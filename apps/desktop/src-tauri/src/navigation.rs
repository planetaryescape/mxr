//! The webview may only show the bridge origin it got from `mxr web` and the
//! bundled down page. Everything else is refused, and web links go to the
//! default browser.

use tauri::Url;

pub const DOWN_PAGE: &str = "tauri://localhost/down.html";
pub const BROWSER_LAUNCH_PATH: &str = "/api/v1/remote/browser-launch";

#[derive(Debug, PartialEq, Eq)]
pub enum Nav {
    Allow,
    /// The down page's "Try again" link: start `mxr web` again.
    Retry,
    /// Hand to the default browser (or mail client) and stay put.
    External,
    Deny,
}

fn is_app_origin(url: &Url) -> bool {
    // `tauri://localhost` on macOS and Linux, `http://tauri.localhost` on Windows.
    (url.scheme() == "tauri" && url.host_str() == Some("localhost"))
        || (url.scheme() == "http" && url.host_str() == Some("tauri.localhost"))
}

fn same_origin(a: &Url, b: &Url) -> bool {
    a.scheme() == b.scheme()
        && a.host_str() == b.host_str()
        && a.port_or_known_default() == b.port_or_known_default()
}

pub fn classify(url: &Url, bridge: Option<&Url>) -> Nav {
    if is_app_origin(url) {
        if url.path() != "/down.html" {
            return Nav::Deny;
        }
        let retry = url
            .query_pairs()
            .any(|(k, v)| k == "action" && v == "retry");
        return if retry { Nav::Retry } else { Nav::Allow };
    }
    match url.scheme() {
        // Frames inside the web app (the sandboxed mail reader uses srcdoc).
        "about" if matches!(url.as_str(), "about:blank" | "about:srcdoc") => Nav::Allow,
        "http" | "https" => match bridge {
            Some(bridge) if same_origin(url, bridge) => Nav::Allow,
            _ => Nav::External,
        },
        "mailto" => Nav::External,
        _ => Nav::Deny,
    }
}

/// Only these schemes may leave the app, however the request arrived.
pub fn may_open_externally(url: &Url) -> bool {
    matches!(url.scheme(), "http" | "https" | "mailto")
}

/// Script that opens a new message over the current page. The web app listens
/// for `mxr:new-message` and sets `mxrNewMessageReady` once mounted
/// (apps/web/src/features/compose/useNewMessageEvent.ts). Until then, or when
/// the window shows the down page or another origin, it navigates to
/// `/compose/new`, which opens the same composer after a full load.
pub fn new_message_script(bridge: &Url) -> Option<String> {
    let fallback = bridge.join("compose/new").ok()?;
    let origin = bridge.origin().ascii_serialization();
    // JSON strings are valid JS string literals, so neither value can break out.
    let origin = serde_json::to_string(&origin).ok()?;
    let fallback = serde_json::to_string(fallback.as_str()).ok()?;
    Some(format!(
        "(function(o,t){{if(location.origin===o&&window.mxrNewMessageReady===true)\
         {{window.dispatchEvent(new Event('mxr:new-message'));}}else{{location.assign(t);}}}})({origin},{fallback});"
    ))
}

/// Exchange only on the selected launch page. The secret travels in a POST body,
/// and the script checks the live location again before creating the form.
pub fn browser_session_script(page: &Url, target: &Url, code: &str) -> Option<String> {
    if !same_origin(page, target)
        || page.path() != BROWSER_LAUNCH_PATH
        || page.query().is_some()
        || page.fragment().is_some()
        || target.scheme() != "https"
        || !crate::mxr_cli::valid_browser_code(code)
    {
        return None;
    }
    let origin = serde_json::to_string(&target.origin().ascii_serialization()).ok()?;
    let code = serde_json::to_string(code).ok()?;
    Some(format!(
        "(function(o,c){{if(location.origin!==o||location.pathname!=='/api/v1/remote/browser-launch'||location.search||location.hash)return;\
         var f=document.createElement('form');f.method='POST';f.action=o+'/api/v1/remote/browser-session';\
         var i=document.createElement('input');i.type='hidden';i.name='code';i.value=c;\
         f.appendChild(i);document.body.appendChild(f);f.submit();}})({origin},{code});"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn u(s: &str) -> Url {
        Url::parse(s).unwrap()
    }

    fn bridge() -> Url {
        u("http://mxr.localhost:42829")
    }

    #[test]
    fn the_bridge_origin_is_allowed_on_any_path() {
        for url in [
            "http://mxr.localhost:42829/",
            "http://mxr.localhost:42829/m/inbox?x=1#y",
            "http://mxr.localhost:42829/compose/new",
        ] {
            assert_eq!(classify(&u(url), Some(&bridge())), Nav::Allow, "{url}");
        }
    }

    #[test]
    fn the_selected_https_origin_is_allowed_and_redirects_leave_the_app() {
        let target = u("https://mail.example.test/");
        assert_eq!(
            classify(&u("https://mail.example.test/m/inbox"), Some(&target)),
            Nav::Allow
        );
        for url in [
            "https://other.example.test/",
            "http://mail.example.test/",
            "https://mail.example.test:444/",
        ] {
            assert_eq!(classify(&u(url), Some(&target)), Nav::External);
        }
    }

    #[test]
    fn browser_code_is_only_submitted_at_the_exact_selected_launch_page() {
        let target = u("https://mail.example.test/");
        let code = "a".repeat(64);
        assert!(
            browser_session_script(&target.join(BROWSER_LAUNCH_PATH).unwrap(), &target, &code)
                .is_some()
        );
        for page in [
            "https://other.example.test/api/v1/remote/browser-launch",
            "http://mail.example.test/api/v1/remote/browser-launch",
            "https://mail.example.test:444/api/v1/remote/browser-launch",
            "https://mail.example.test/",
            "https://mail.example.test/compose/new",
            "https://mail.example.test/api/v1/remote/browser-launch?redirect=other",
            "https://mail.example.test/api/v1/remote/browser-launch#unexpected",
            DOWN_PAGE,
        ] {
            assert!(
                browser_session_script(&u(page), &target, &code).is_none(),
                "{page}"
            );
        }
        assert!(browser_session_script(&target, &target, "invalid").is_none());
        assert!(browser_session_script(&bridge(), &bridge(), &code).is_none());
    }

    #[test]
    fn other_origins_go_to_the_browser() {
        for url in [
            "https://example.com/",
            "http://mxr.localhost:9999/",
            "http://evil.localhost:42829/",
            "https://mxr.localhost:42829/",
            "mailto:a@example.com",
        ] {
            assert_eq!(classify(&u(url), Some(&bridge())), Nav::External, "{url}");
        }
    }

    #[test]
    fn nothing_web_is_allowed_before_the_bridge_is_known() {
        assert_eq!(
            classify(&u("http://mxr.localhost:42829/"), None),
            Nav::External
        );
    }

    #[test]
    fn the_down_page_and_retry() {
        assert_eq!(classify(&u(DOWN_PAGE), None), Nav::Allow);
        assert_eq!(
            classify(&u("tauri://localhost/down.html?reason=missing"), None),
            Nav::Allow
        );
        assert_eq!(
            classify(&u("tauri://localhost/down.html?action=retry"), None),
            Nav::Retry
        );
    }

    #[test]
    fn bundled_files_other_than_the_down_page_are_refused() {
        assert_eq!(
            classify(&u("tauri://localhost/index.html"), None),
            Nav::Deny
        );
        assert_eq!(classify(&u("tauri://localhost/"), None), Nav::Deny);
    }

    #[test]
    fn dangerous_schemes_are_refused() {
        for url in [
            "file:///etc/passwd",
            "javascript:alert(1)",
            "data:text/html,hi",
            "blob:http://mxr.localhost:42829/x",
            "ftp://example.com/",
            "tel:123",
        ] {
            assert_eq!(classify(&u(url), Some(&bridge())), Nav::Deny, "{url}");
        }
    }

    #[test]
    fn frames_may_use_about_blank_and_srcdoc() {
        assert_eq!(classify(&u("about:blank"), Some(&bridge())), Nav::Allow);
        assert_eq!(classify(&u("about:srcdoc"), Some(&bridge())), Nav::Allow);
        assert_eq!(classify(&u("about:config"), Some(&bridge())), Nav::Deny);
    }

    #[test]
    fn only_web_and_mail_links_open_externally() {
        assert!(may_open_externally(&u("https://example.com")));
        assert!(may_open_externally(&u("http://example.com")));
        assert!(may_open_externally(&u("mailto:a@b.c")));
        assert!(!may_open_externally(&u("file:///Applications")));
        assert!(!may_open_externally(&u("javascript:1")));
    }

    #[test]
    fn the_new_message_script_prefers_the_in_page_event() {
        let script = new_message_script(&bridge()).unwrap();
        assert!(script.contains("\"http://mxr.localhost:42829\""));
        assert!(script.contains("\"http://mxr.localhost:42829/compose/new\""));
        assert!(script.contains("mxrNewMessageReady===true"));
        assert!(script.contains("mxr:new-message"));
        // The token fragment is not carried into the fallback URL.
        let with_fragment =
            new_message_script(&u("http://mxr.localhost:42829/#token=s3cret")).unwrap();
        assert!(!with_fragment.contains("s3cret"));
    }
}
