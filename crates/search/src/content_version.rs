use std::io;
use std::path::{Path, PathBuf};

/// Version of the text the lexical index stores for each body. Bump it when
/// the indexing text changes, so existing indexes rebuild once on startup.
pub const LEXICAL_CONTENT_VERSION: u32 = 1;

/// The marker sits next to the index directory, not inside it, so Tantivy
/// never sees it and a wipe of the index leaves it in place.
fn marker_path(index_path: &Path) -> PathBuf {
    let name = index_path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    index_path.with_file_name(format!("{name}.content-version"))
}

pub fn lexical_content_is_current(index_path: &Path) -> bool {
    std::fs::read_to_string(marker_path(index_path))
        .is_ok_and(|version| version.trim() == LEXICAL_CONTENT_VERSION.to_string())
}

/// Call only after the index holds a complete rebuild at the current version.
pub fn record_lexical_content_version(index_path: &Path) -> io::Result<()> {
    std::fs::write(
        marker_path(index_path),
        format!("{LEXICAL_CONTENT_VERSION}\n"),
    )
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap fixture setup for direct failures"
    )]

    use super::*;

    #[test]
    fn missing_marker_is_not_current_until_recorded() {
        let dir = tempfile::tempdir().unwrap();
        let index_path = dir.path().join("search_index");
        std::fs::create_dir_all(&index_path).unwrap();

        assert!(!lexical_content_is_current(&index_path));
        record_lexical_content_version(&index_path).unwrap();
        assert!(lexical_content_is_current(&index_path));
    }

    #[test]
    fn marker_survives_wiping_the_index_directory() {
        let dir = tempfile::tempdir().unwrap();
        let index_path = dir.path().join("search_index");
        std::fs::create_dir_all(&index_path).unwrap();
        record_lexical_content_version(&index_path).unwrap();

        std::fs::remove_dir_all(&index_path).unwrap();

        assert!(lexical_content_is_current(&index_path));
    }

    #[test]
    fn other_version_is_not_current() {
        let dir = tempfile::tempdir().unwrap();
        let index_path = dir.path().join("search_index");
        std::fs::write(marker_path(&index_path), "0\n").unwrap();

        assert!(!lexical_content_is_current(&index_path));
    }
}
