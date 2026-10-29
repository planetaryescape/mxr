mod boilerplate;
mod html;
mod html_limits;
mod html_quote;
mod new_text;
mod pipeline;
mod quotes;
mod searchable;
mod signatures;
mod tracking;

pub use html::{
    clamp_table_spans, guard_html2text, html_to_plain_text, in_guarded_render, strip_html_tags,
    MAX_TABLE_SPAN,
};
pub use html_limits::{bounded_html, MAX_HTML_BYTES, MAX_TABLE_CELLS, MAX_TABLE_COLUMNS};
pub use html_quote::{split_html_quote, split_html_quote_keeping_signature, HtmlParts};
pub use new_text::{
    authored_text, new_text, plain_text, EarlierMessage, NewText, Trimmed, ONLY_QUOTED_TEXT,
};
pub use pipeline::{clean, ReaderConfig, ReaderOutput};
pub use quotes::QuotedBlock;
pub use searchable::{body_match_parts, searchable_text, SearchableText};
