//! html2text's styled render of an HTML body, run in the HTML helper process.
//!
//! The helper answers in this module's own types, not html2text's, because
//! those are what cross the pipe and what the message view draws from.

use html2text::render::{RichAnnotation, TaggedLine};
use mxr_html_worker::{AnyJob, Job};
use serde::{Deserialize, Serialize};

/// What a run of text is marked as. Mirrors the annotations the message view
/// styles; anything html2text adds later becomes `Other` and draws plain.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum Annotation {
    Default,
    Link(String),
    Image(String),
    Emphasis,
    Strong,
    Strikeout,
    Code,
    Preformat(bool),
    Colour(u8, u8, u8),
    BgColour(u8, u8, u8),
    Other,
}

/// A run of text with one set of annotations.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Piece {
    pub text: String,
    pub annotations: Vec<Annotation>,
}

pub(crate) type RichLine = Vec<Piece>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum RichRender {
    Lines(Vec<RichLine>),
    /// html2text rejected the document. The view shows the HTML source's text.
    Failed,
    /// Over the HTML budget, or html2text panicked: the view shows the tag
    /// strip. A helper that was lost gives the same answer.
    Strip,
}

pub(crate) static RICH_LINES: Job<usize, RichRender> =
    Job::new("tui.rich_lines", |html, width| render(html, *width));

/// Every parse this crate runs in the HTML helper process.
pub fn html_jobs() -> Vec<&'static dyn AnyJob> {
    vec![&RICH_LINES]
}

/// The bounded input keeps html2text off adversarial tables (see mxr-reader's
/// `html_limits`).
fn render(html: &str, width: usize) -> RichRender {
    let Some(bounded) = mxr_reader::bounded_html(html) else {
        return RichRender::Strip;
    };
    let rendered = mxr_reader::catch_html2text_panic(|| {
        let render_tree = html2text::parse(bounded.as_bytes()).ok()?;
        html2text::config::rich()
            .render_to_lines(render_tree, width)
            .ok()
    });
    match rendered {
        Some(Some(lines)) => RichRender::Lines(lines.into_iter().map(wire_line).collect()),
        Some(None) => RichRender::Failed,
        None => RichRender::Strip,
    }
}

fn wire_line(line: TaggedLine<Vec<RichAnnotation>>) -> RichLine {
    line.tagged_strings()
        .map(|tagged| Piece {
            text: tagged.s.clone(),
            annotations: tagged.tag.iter().map(wire_annotation).collect(),
        })
        .collect()
}

fn wire_annotation(annotation: &RichAnnotation) -> Annotation {
    match annotation {
        RichAnnotation::Default => Annotation::Default,
        RichAnnotation::Link(url) => Annotation::Link(url.clone()),
        RichAnnotation::Image(source) => Annotation::Image(source.clone()),
        RichAnnotation::Emphasis => Annotation::Emphasis,
        RichAnnotation::Strong => Annotation::Strong,
        RichAnnotation::Strikeout => Annotation::Strikeout,
        RichAnnotation::Code => Annotation::Code,
        RichAnnotation::Preformat(continuation) => Annotation::Preformat(*continuation),
        RichAnnotation::Colour(colour) => Annotation::Colour(colour.r, colour.g, colour.b),
        RichAnnotation::BgColour(colour) => Annotation::BgColour(colour.r, colour.g, colour.b),
        _ => Annotation::Other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_link_and_strong_text_keep_their_annotations_across_the_wire() {
        let RichRender::Lines(lines) = render(
            "<p>see <a href=\"https://example.com/x\">this</a> and <strong>that</strong></p>",
            60,
        ) else {
            std::panic::resume_unwind(Box::new("expected rendered lines"));
        };
        let pieces: Vec<&Piece> = lines.iter().flatten().collect();
        assert!(pieces.iter().any(|piece| piece.text.contains("this")
            && piece
                .annotations
                .contains(&Annotation::Link("https://example.com/x".to_string()))));
        assert!(pieces
            .iter()
            .any(|piece| piece.text.contains("that")
                && piece.annotations.contains(&Annotation::Strong)));
        let encoded = serde_json::to_vec(&RichRender::Lines(lines.clone())).expect("encodes");
        let decoded: RichRender = serde_json::from_slice(&encoded).expect("decodes");
        assert_eq!(decoded, RichRender::Lines(lines));
    }

    #[test]
    fn an_image_keeps_its_source_and_alt_text() {
        let RichRender::Lines(lines) = render("<img src=\"cid:logo\" alt=\"Logo\">", 60) else {
            std::panic::resume_unwind(Box::new("expected rendered lines"));
        };
        assert!(lines
            .iter()
            .flatten()
            .any(|piece| piece.text.contains("Logo")
                && piece
                    .annotations
                    .contains(&Annotation::Image("cid:logo".to_string()))));
    }

    #[test]
    fn a_document_over_the_budget_is_stripped_not_rendered() {
        let html = "<br>".repeat(mxr_reader::MAX_HTML_BYTES / 4 + 1);
        assert_eq!(render(&html, 60), RichRender::Strip);
    }
}
