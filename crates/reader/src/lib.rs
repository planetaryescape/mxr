mod boilerplate;
mod html;
mod html_quote;
mod new_text;
mod pipeline;
mod quotes;
mod signatures;
mod tracking;

pub use html::{guard_html2text, html_to_plain_text, strip_html_tags};
pub use html_quote::{split_html_quote, HtmlParts};
pub use new_text::{
    authored_text, new_text, plain_text, EarlierMessage, NewText, Trimmed, ONLY_QUOTED_TEXT,
};
pub use pipeline::{clean, ReaderConfig, ReaderOutput};
pub use quotes::QuotedBlock;
