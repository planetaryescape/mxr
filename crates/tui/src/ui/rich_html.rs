use html2text::render::{RichAnnotation, TaggedLine};
use mxr_html_worker::{AnyJob, Job};
use serde::{Deserialize, Serialize};

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Piece {
    pub text: String,
    pub annotations: Vec<Annotation>,
}

pub(crate) type RichLine = Vec<Piece>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum RichRender {
    Lines(Vec<RichLine>),
    Failed,
    /// Over the HTML budget, or html2text panicked: the view shows the tag
    /// strip. A helper that was lost gives the same answer.
    Strip,
}

pub(crate) static RICH_LINES: Job<usize, RichRender> =
    Job::new("tui.rich_lines", |html, width| render(html, *width));

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
    fn a_document_over_the_budget_is_stripped_not_rendered() {
        let html = "<br>".repeat(mxr_reader::MAX_HTML_BYTES / 4 + 1);
        assert_eq!(render(&html, 60), RichRender::Strip);
    }
}
