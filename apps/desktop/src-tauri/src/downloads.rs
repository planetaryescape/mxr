//! Attachment saves. The web app builds a Blob and clicks `<a download>`;
//! WKWebView only follows that when the shell handles downloads, and it
//! needs a destination. Files go to the user's Downloads folder under the
//! name the page suggested, never overwriting an existing file.

use std::path::{Path, PathBuf};
use tauri::Url;

/// Only a download the web app started itself: a blob it created, or a file
/// served by the bridge. Anything else is refused.
pub fn is_from_bridge(url: &Url, bridge: Option<&Url>) -> bool {
    let Some(bridge) = bridge else { return false };
    let origin = if url.scheme() == "blob" {
        // `blob:http://host:port/<uuid>`: the origin is the inner URL's.
        match Url::parse(url.path()) {
            Ok(inner) => inner.origin(),
            Err(_) => return false,
        }
    } else {
        url.origin()
    };
    origin == bridge.origin()
}

/// `~/Downloads/<name>`, or `<name> (2).<ext>` and so on when taken. The page
/// suggests the name, so only its last path component is used.
pub fn destination(
    downloads: &Path,
    suggested: Option<&std::ffi::OsStr>,
    taken: impl Fn(&Path) -> bool,
) -> PathBuf {
    let name = suggested
        .map(Path::new)
        .and_then(Path::file_name)
        .map(Path::new)
        .filter(|name| !name.as_os_str().is_empty() && name.as_os_str() != "..")
        .unwrap_or_else(|| Path::new("attachment"));
    let first = downloads.join(name);
    if !taken(&first) {
        return first;
    }
    let stem = name
        .file_stem()
        .unwrap_or(name.as_os_str())
        .to_string_lossy();
    let ext = name.extension().map(|ext| ext.to_string_lossy());
    (2..)
        .map(|n| match &ext {
            Some(ext) => downloads.join(format!("{stem} ({n}).{ext}")),
            None => downloads.join(format!("{stem} ({n})")),
        })
        .find(|candidate| !taken(candidate))
        .expect("an unbounded range always yields a free name")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsStr;

    fn u(s: &str) -> Url {
        Url::parse(s).unwrap()
    }

    #[test]
    fn only_bridge_blobs_and_files_may_download() {
        let bridge = u("http://mxr.localhost:42829");
        for ok in [
            "blob:http://mxr.localhost:42829/6f1d",
            "http://mxr.localhost:42829/api/v1/export",
        ] {
            assert!(is_from_bridge(&u(ok), Some(&bridge)), "{ok}");
        }
        for bad in [
            "blob:http://evil.example/6f1d",
            "blob:https://mxr.localhost:42829/6f1d",
            "https://example.com/file.zip",
            "http://mxr.localhost:9/x",
            "blob:nonsense",
        ] {
            assert!(!is_from_bridge(&u(bad), Some(&bridge)), "{bad}");
        }
        assert!(!is_from_bridge(
            &u("blob:http://mxr.localhost:42829/x"),
            None
        ));
    }

    #[test]
    fn uses_the_suggested_name_inside_downloads() {
        let dest = destination(Path::new("/d"), Some(OsStr::new("a.pdf")), |_| false);
        assert_eq!(dest, PathBuf::from("/d/a.pdf"));
    }

    #[test]
    fn strips_directories_from_the_suggested_name() {
        let dest = destination(Path::new("/d"), Some(OsStr::new("../../etc/x.txt")), |_| {
            false
        });
        assert_eq!(dest, PathBuf::from("/d/x.txt"));
        let dest = destination(Path::new("/d"), Some(OsStr::new("/abs/y.txt")), |_| false);
        assert_eq!(dest, PathBuf::from("/d/y.txt"));
    }

    #[test]
    fn falls_back_when_there_is_no_usable_name() {
        for suggested in [None, Some(OsStr::new("")), Some(OsStr::new(".."))] {
            assert_eq!(
                destination(Path::new("/d"), suggested, |_| false),
                PathBuf::from("/d/attachment")
            );
        }
    }

    #[test]
    fn never_overwrites() {
        let taken = |p: &Path| p == Path::new("/d/a.pdf") || p == Path::new("/d/a (2).pdf");
        assert_eq!(
            destination(Path::new("/d"), Some(OsStr::new("a.pdf")), taken),
            PathBuf::from("/d/a (3).pdf")
        );
        let taken = |p: &Path| p == Path::new("/d/README");
        assert_eq!(
            destination(Path::new("/d"), Some(OsStr::new("README")), taken),
            PathBuf::from("/d/README (2)")
        );
    }
}
