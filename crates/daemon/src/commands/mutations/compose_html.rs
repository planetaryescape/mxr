//! Reading and assembling an HTML compose body from CLI arguments.
//!
//! The one invariant everything here serves: the caller's HTML reaches the
//! draft byte-for-byte. Nothing in this module reformats, wraps, minifies, or
//! sanitises it. The only transformation that can touch the document at all is
//! an explicitly requested `--signature-html` append, and that is opt-in.

use anyhow::{bail, Context};
use mxr_core::types::InlineAsset;
use std::collections::HashSet;
use std::io::Read;
use std::path::{Path, PathBuf};

/// A fully assembled HTML body, ready to become a `DraftContent::Html`.
#[derive(Debug)]
pub struct HtmlComposeInput {
    pub html: String,
    /// Caller-supplied `text/plain` alternative. `None` means the outbound
    /// builder generates one; it never rewrites the HTML to do so.
    pub text: Option<String>,
    pub inline_assets: Vec<InlineAsset>,
}

/// Arguments that select and shape an HTML body.
pub struct HtmlComposeArgs {
    pub html_file: Option<PathBuf>,
    pub html_stdin: bool,
    pub text_file: Option<PathBuf>,
    pub inline: Vec<String>,
    pub signature_html: Option<PathBuf>,
}

impl HtmlComposeArgs {
    pub fn is_html_mode(&self) -> bool {
        self.html_file.is_some() || self.html_stdin
    }
}

/// Assemble the HTML body, or `None` when this is an ordinary markdown compose.
///
/// Validates before returning so `--dry-run` and `--check` report the same
/// problems a real save would, without anything being created.
pub fn read_html_input(args: &HtmlComposeArgs) -> anyhow::Result<Option<HtmlComposeInput>> {
    if !args.is_html_mode() {
        return Ok(None);
    }

    let mut html = if let Some(path) = &args.html_file {
        read_to_string(path).with_context(|| format!("reading --html-file {}", path.display()))?
    } else {
        let mut buffer = String::new();
        std::io::stdin()
            .read_to_string(&mut buffer)
            .context("reading HTML body from stdin")?;
        buffer
    };

    if html.trim().is_empty() {
        bail!("HTML body is empty");
    }

    if let Some(path) = &args.signature_html {
        let signature = read_to_string(path)
            .with_context(|| format!("reading --signature-html {}", path.display()))?;
        html = append_signature(&html, &signature);
    }

    let text = args
        .text_file
        .as_ref()
        .map(|path| {
            read_to_string(path).with_context(|| format!("reading --text-file {}", path.display()))
        })
        .transpose()?;

    // A caller who passes `--text-file` means to supply the alternative, so an
    // empty one is a mistake — and a mistake that survives all the way to the
    // reader. Consumers prefer a supplied alternative over the document, and
    // `Some("")` is still "supplied", so the message renders blank despite
    // carrying a perfectly good HTML document. Omitting the flag is how you ask
    // mxr to generate the alternative.
    if text.as_ref().is_some_and(|text| text.trim().is_empty()) {
        bail!(
            "--text-file is empty; a blank plain-text alternative renders as an empty \
             message. Omit it to have mxr generate one from the HTML."
        );
    }

    let inline_assets = parse_inline_assets(&args.inline)?;

    // Report before anything is created. The daemon re-checks at its own
    // boundary; this copy exists so the CLI can fail fast with the same
    // message and a non-zero exit.
    if let Err(error) = mxr_outbound::html::validate_html(&html) {
        bail!("{error}");
    }

    Ok(Some(HtmlComposeInput {
        html,
        text,
        inline_assets,
    }))
}

/// Append an HTML signature immediately before the closing `</body>`.
///
/// Falls back to appending at the end of the document when there is no
/// `</body>` — a fragment is still a valid thing to send. Matches the last
/// occurrence so a `</body>` mentioned inside a conditional comment earlier in
/// the document does not capture the signature.
fn append_signature(html: &str, signature: &str) -> String {
    let signature = signature.trim_end();
    match html.to_ascii_lowercase().rfind("</body>") {
        Some(index) => format!("{}{signature}{}", &html[..index], &html[index..]),
        None => format!("{html}{signature}"),
    }
}

/// Parse repeated `--inline NAME=PATH` arguments.
fn parse_inline_assets(raw: &[String]) -> anyhow::Result<Vec<InlineAsset>> {
    let mut seen: HashSet<&str> = HashSet::new();
    let mut assets = Vec::with_capacity(raw.len());

    for entry in raw {
        let Some((cid, path)) = entry.split_once('=') else {
            bail!("--inline expects CID=PATH, got `{entry}`");
        };
        let cid = cid.trim();
        let path = path.trim();

        if path.is_empty() {
            bail!("--inline `{entry}` has no path");
        }
        mxr_outbound::attachments::validate_cid(cid)?;
        if !seen.insert(cid) {
            bail!(
                "{}",
                mxr_outbound::attachments::InlineAssetError::DuplicateCid(cid.to_string())
            );
        }

        let path = expand_tilde(path);
        if !path.exists() {
            bail!(
                "--inline `{cid}` points at a missing file: {}",
                path.display()
            );
        }

        assets.push(InlineAsset {
            cid: cid.to_string(),
            path,
        });
    }

    Ok(assets)
}

/// Every `cid:` reference in the HTML that no `--inline` provides.
///
/// Reported as a warning rather than an error: a template may legitimately
/// reference an asset the recipient's client resolves some other way, and
/// refusing the send would be mxr overruling the author.
pub fn unresolved_cid_references(html: &str, assets: &[InlineAsset]) -> Vec<String> {
    let provided: HashSet<&str> = assets.iter().map(|asset| asset.cid.as_str()).collect();
    let mut missing: Vec<String> = Vec::new();

    for (index, _) in html.match_indices("cid:") {
        let rest = &html[index + 4..];
        let end = rest
            .find(|c: char| !(c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | '+')))
            .unwrap_or(rest.len());
        let cid = &rest[..end];
        if !cid.is_empty() && !provided.contains(cid) && !missing.iter().any(|seen| seen == cid) {
            missing.push(cid.to_string());
        }
    }

    missing
}

fn read_to_string(path: &Path) -> std::io::Result<String> {
    std::fs::read_to_string(expand_tilde(&path.display().to_string()))
}

fn expand_tilde(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest);
        }
    }
    PathBuf::from(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args() -> HtmlComposeArgs {
        HtmlComposeArgs {
            html_file: None,
            html_stdin: false,
            text_file: None,
            inline: Vec::new(),
            signature_html: None,
        }
    }

    #[test]
    fn markdown_compose_is_not_html_mode() {
        assert!(!args().is_html_mode());
        assert!(read_html_input(&args()).unwrap().is_none());
    }

    #[test]
    fn inline_assets_parse_as_cid_equals_path() {
        let file = std::env::temp_dir().join("mxr-inline-test.png");
        std::fs::write(&file, b"png").unwrap();
        let parsed = parse_inline_assets(&[format!("notto-logo={}", file.display())]).unwrap();
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].cid, "notto-logo");
        std::fs::remove_file(&file).ok();
    }

    #[test]
    fn inline_rejects_missing_equals_and_missing_file() {
        assert!(parse_inline_assets(&["notto-logo".to_string()]).is_err());
        assert!(parse_inline_assets(&["logo=/nope/missing.png".to_string()]).is_err());
    }

    #[test]
    fn inline_rejects_a_cid_that_could_forge_a_header() {
        let err = parse_inline_assets(&["bad\r\nX-Evil: 1=/tmp/x.png".to_string()]).unwrap_err();
        assert!(err.to_string().contains("invalid content id"), "{err}");
    }

    #[test]
    fn inline_rejects_duplicate_cids() {
        let file = std::env::temp_dir().join("mxr-inline-dup.png");
        std::fs::write(&file, b"png").unwrap();
        let err = parse_inline_assets(&[
            format!("logo={}", file.display()),
            format!("logo={}", file.display()),
        ])
        .unwrap_err();
        assert!(err.to_string().contains("duplicate content id"), "{err}");
        std::fs::remove_file(&file).ok();
    }

    #[test]
    fn signature_goes_before_the_closing_body_tag() {
        let html = "<html><body><p>hi</p></body></html>";
        let merged = append_signature(html, "<p>-- Notto</p>");
        assert_eq!(merged, "<html><body><p>hi</p><p>-- Notto</p></body></html>");
    }

    #[test]
    fn signature_appends_at_the_end_of_a_fragment() {
        assert_eq!(
            append_signature("<p>hi</p>", "<p>sig</p>"),
            "<p>hi</p><p>sig</p>"
        );
    }

    #[test]
    fn without_a_signature_flag_the_html_is_untouched() {
        // The whole point: no implicit mutation of a supplied document.
        let html = "<html><body><p>hi</p></body></html>";
        let mut a = args();
        let file = std::env::temp_dir().join("mxr-html-untouched.html");
        std::fs::write(&file, html).unwrap();
        a.html_file = Some(file.clone());
        let input = read_html_input(&a).unwrap().unwrap();
        assert_eq!(input.html, html);
        assert!(input.text.is_none());
        std::fs::remove_file(&file).ok();
    }

    #[test]
    fn dangerous_html_is_refused_before_anything_is_created() {
        let mut a = args();
        let file = std::env::temp_dir().join("mxr-html-dangerous.html");
        std::fs::write(&file, "<p>hi</p><script>alert(1)</script>").unwrap();
        a.html_file = Some(file.clone());
        let err = read_html_input(&a).unwrap_err();
        assert!(err.to_string().contains("<script>"), "{err}");
        assert!(err.to_string().contains("was not modified"), "{err}");
        std::fs::remove_file(&file).ok();
    }

    /// An empty `--text-file` is a mistake, not an instruction. Accepting it
    /// stores a draft with `body_text: Some("")`, and consumers that prefer a
    /// supplied alternative over the document then render a blank message for
    /// something that has a perfectly good HTML body.
    #[test]
    fn an_empty_text_alternative_is_refused_but_a_real_one_is_kept() {
        let dir = std::env::temp_dir();
        let html = dir.join("mxr-text-alt-doc.html");
        let text = dir.join("mxr-text-alt-alternative.txt");
        std::fs::write(&html, "<p>hi</p>").unwrap();
        std::fs::write(&text, "  \n\t\n").unwrap();
        let mut a = args();
        a.html_file = Some(html.clone());
        a.text_file = Some(text.clone());

        let err = read_html_input(&a).unwrap_err();
        assert!(err.to_string().contains("--text-file is empty"), "{err}");

        // Counterweight: a real alternative must still come through untouched,
        // or the check is just a ban on the flag.
        std::fs::write(&text, "hi").unwrap();
        let input = read_html_input(&a).unwrap().unwrap();
        assert_eq!(input.text.as_deref(), Some("hi"));

        std::fs::remove_file(&html).ok();
        std::fs::remove_file(&text).ok();
    }

    #[test]
    fn unresolved_cids_are_detected() {
        let html = r#"<img src="cid:logo"><img src="cid:banner">"#;
        let assets = vec![InlineAsset {
            cid: "logo".into(),
            path: PathBuf::from("/tmp/logo.png"),
        }];
        assert_eq!(unresolved_cid_references(html, &assets), vec!["banner"]);
    }

    /// Reported, but not a refusal: a `cid:` mxr cannot resolve may still be
    /// something the recipient's client can, and refusing would be mxr
    /// overruling the author.
    #[test]
    fn an_unresolved_cid_does_not_stop_the_body_being_assembled() {
        let html = r#"<html><body><img src="cid:missing-banner"></body></html>"#;
        let file = std::env::temp_dir().join("mxr-html-unresolved-cid.html");
        std::fs::write(&file, html).unwrap();
        let mut a = args();
        a.html_file = Some(file.clone());

        let input = read_html_input(&a)
            .expect("an unresolved cid must not be an error")
            .expect("html mode");

        assert_eq!(input.html, html);
        assert_eq!(
            unresolved_cid_references(&input.html, &input.inline_assets),
            vec!["missing-banner"]
        );
        std::fs::remove_file(&file).ok();
    }

    #[test]
    fn inline_rejects_an_entry_with_no_path() {
        let err = parse_inline_assets(&["logo=".to_string()]).unwrap_err();
        assert!(err.to_string().contains("no path"), "{err}");
    }

    #[test]
    fn fully_resolved_cids_report_nothing() {
        let html = r#"<img src="cid:logo">"#;
        let assets = vec![InlineAsset {
            cid: "logo".into(),
            path: PathBuf::from("/tmp/logo.png"),
        }];
        assert!(unresolved_cid_references(html, &assets).is_empty());
    }
}
