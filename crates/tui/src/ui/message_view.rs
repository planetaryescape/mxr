use crate::app::{
    ActivePane, AttachmentSummary, BodySource, BodyViewMetadata, BodyViewMode, BodyViewState,
};
use crate::terminal_images::{HtmlImageEntry, HtmlImageRenderState};
use crate::theme::Theme;
use html2text::render::RichAnnotation;
use mxr_core::id::MessageId;
use mxr_core::types::{Envelope, HtmlImageAssetStatus, HtmlImageSourceKind};
use ratatui::prelude::*;
use ratatui::widgets::*;
use ratatui_image::{Resize, StatefulImage};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct ThreadMessageBlock {
    pub envelope: Envelope,
    pub body_state: BodyViewState,
    pub labels: Vec<String>,
    pub attachments: Vec<AttachmentSummary>,
    pub selected: bool,
    pub bulk_selected: bool,
    pub has_unsubscribe: bool,
    pub signature_expanded: bool,
}

#[derive(Debug, Clone)]
enum RenderBlock {
    Text(Vec<Line<'static>>),
    Image(HtmlImageBlock),
}

#[derive(Debug, Clone)]
struct HtmlImageBlock {
    message_id: MessageId,
    source: String,
    label: String,
}

pub fn draw(
    frame: &mut Frame,
    area: Rect,
    messages: &[ThreadMessageBlock],
    scroll_offset: u16,
    active_pane: &ActivePane,
    theme: &Theme,
    html_images: &mut HashMap<MessageId, HashMap<String, HtmlImageEntry>>,
) {
    let is_focused = *active_pane == ActivePane::MessageView;
    let border_style = theme.border_style(is_focused);

    let title = if messages.len() > 1 {
        " Thread "
    } else {
        " Message "
    };
    let block = Block::bordered()
        .title(title)
        .border_type(BorderType::Rounded)
        .border_style(border_style);

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let mut blocks: Vec<RenderBlock> = Vec::new();

    for (index, message) in messages.iter().enumerate() {
        if index > 0 {
            blocks.push(RenderBlock::Text(vec![
                Line::from(""),
                Line::from(Span::styled(
                    "────────────────────────────────────────",
                    Style::default().fg(theme.text_muted),
                )),
                Line::from(""),
            ]));
        }

        let env = &message.envelope;
        let from = env.from.name.as_deref().unwrap_or(&env.from.email);
        let label_style = if message.selected {
            Style::default().fg(theme.accent).bold()
        } else {
            Style::default().fg(theme.text_muted)
        };
        let value_style = Style::default().fg(theme.text_primary);
        let mut text_lines: Vec<Line<'static>> = Vec::new();

        // Aligned headers with consistent label width
        let label_width = 10; // "Subject: " padded
        text_lines.push(Line::from(vec![
            Span::styled(format!("{:<label_width$}", "From:"), label_style),
            Span::styled(format!("{} <{}>", from, env.from.email), value_style),
        ]));
        if !env.to.is_empty() {
            let to_str = env
                .to
                .iter()
                .map(|a| {
                    a.name
                        .as_ref()
                        .map(|n| format!("{} <{}>", n, a.email))
                        .unwrap_or_else(|| a.email.clone())
                })
                .collect::<Vec<_>>()
                .join(", ");
            text_lines.push(Line::from(vec![
                Span::styled(format!("{:<label_width$}", "To:"), label_style),
                Span::styled(to_str, value_style),
            ]));
        }
        text_lines.push(Line::from(vec![
            Span::styled(format!("{:<label_width$}", "Date:"), label_style),
            Span::styled(env.date.format("%Y-%m-%d %H:%M").to_string(), value_style),
        ]));
        text_lines.push(Line::from(vec![
            Span::styled(format!("{:<label_width$}", "Subject:"), label_style),
            Span::styled(env.subject.clone(), value_style),
        ]));

        // Selection + label chips
        if message.bulk_selected || !message.labels.is_empty() {
            let mut chips: Vec<Span> = Vec::new();
            if message.bulk_selected {
                chips.push(Span::styled(
                    " Selected ",
                    Style::default()
                        .bg(theme.selection_bg)
                        .fg(theme.selection_fg)
                        .add_modifier(Modifier::BOLD),
                ));
                chips.push(Span::raw(" "));
            }
            for label in &message.labels {
                chips.push(Span::styled(
                    format!(" {} ", label),
                    Style::default()
                        .bg(Theme::label_color(label))
                        .fg(Color::Black),
                ));
                chips.push(Span::raw(" "));
            }
            text_lines.push(Line::from(chips));
        }

        if message.has_unsubscribe {
            text_lines.push(Line::from(vec![
                Span::styled(format!("{:<label_width$}", "List:"), label_style),
                Span::styled(
                    " unsubscribe ",
                    Style::default().bg(theme.warning).fg(Color::Black).bold(),
                ),
            ]));
        }

        // Attachments
        if !message.attachments.is_empty() {
            text_lines.push(Line::from(vec![Span::styled(
                format!("{:<label_width$}", "Attach:"),
                label_style,
            )]));
            for attachment in &message.attachments {
                text_lines.push(Line::from(vec![
                    Span::raw(" ".repeat(label_width)),
                    Span::styled(
                        attachment.filename.clone(),
                        Style::default().fg(theme.success).bold(),
                    ),
                    Span::styled(
                        format!(" ({})", human_size(attachment.size_bytes)),
                        Style::default().fg(theme.text_muted),
                    ),
                ]));
            }
        }
        text_lines.push(Line::from(""));

        match &message.body_state {
            BodyViewState::Ready {
                raw,
                rendered,
                source,
                metadata,
            } => {
                text_lines.extend(body_metadata_lines(metadata, source, theme));
                if metadata.mode == BodyViewMode::Html && *source == BodySource::Html {
                    blocks.push(RenderBlock::Text(text_lines));
                    blocks.extend(render_html_blocks(
                        &message.envelope.id,
                        raw,
                        inner.width,
                        theme,
                        metadata.remote_content_enabled,
                    ));
                } else {
                    text_lines.extend(process_body_lines(
                        rendered,
                        theme,
                        message.signature_expanded,
                        metadata.reader_applied,
                    ));
                    blocks.push(RenderBlock::Text(text_lines));
                }
            }
            BodyViewState::Loading { preview } => {
                if let Some(preview) = preview {
                    text_lines.extend(process_body_lines(preview, theme, false, false));
                    text_lines.push(Line::from(""));
                }
                text_lines.push(Line::from(Span::styled(
                    "Loading...",
                    Style::default().fg(theme.text_muted),
                )));
                blocks.push(RenderBlock::Text(text_lines));
            }
            BodyViewState::Empty { preview } => {
                if let Some(preview) = preview {
                    text_lines.extend(process_body_lines(preview, theme, false, false));
                    text_lines.push(Line::from(""));
                }
                text_lines.push(Line::from(Span::styled(
                    "(no body available)",
                    Style::default().fg(theme.text_muted),
                )));
                blocks.push(RenderBlock::Text(text_lines));
            }
            BodyViewState::Error {
                message: err_msg,
                preview,
            } => {
                if let Some(preview) = preview {
                    text_lines.extend(process_body_lines(preview, theme, false, false));
                    text_lines.push(Line::from(""));
                }
                text_lines.push(Line::from(Span::styled(
                    format!("Error: {err_msg}"),
                    Style::default().fg(theme.error),
                )));
                blocks.push(RenderBlock::Text(text_lines));
            }
        }
    }

    if messages.is_empty() {
        blocks.push(RenderBlock::Text(vec![Line::from(Span::styled(
            "No message selected",
            Style::default().fg(theme.text_muted),
        ))]));
    }

    render_blocks(frame, inner, scroll_offset, blocks, theme, html_images);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{BodySource, BodyViewMetadata, BodyViewMode};
    use chrono::{TimeZone, Utc};
    use mxr_core::id::{AccountId, MessageId, ThreadId};
    use mxr_core::types::{Address, BodyPartSource, MessageFlags, UnsubscribeMethod};
    use mxr_test_support::render_to_string;

    fn envelope() -> Envelope {
        Envelope {
            id: MessageId::new(),
            account_id: AccountId::new(),
            provider_id: "msg-1".into(),
            thread_id: ThreadId::new(),
            message_id_header: None,
            in_reply_to: None,
            references: vec![],
            from: Address {
                name: Some("Alice".into()),
                email: "alice@example.com".into(),
            },
            to: vec![],
            cc: vec![],
            bcc: vec![],
            subject: "Selection".into(),
            date: Utc.with_ymd_and_hms(2024, 3, 15, 9, 30, 0).unwrap(),
            flags: MessageFlags::READ,
            snippet: "snippet".into(),
            has_attachments: false,
            size_bytes: 1024,
            unsubscribe: UnsubscribeMethod::None,
            label_provider_ids: vec![],
        }
    }

    #[test]
    fn selected_messages_render_visible_chip() {
        let block = ThreadMessageBlock {
            envelope: envelope(),
            body_state: BodyViewState::Ready {
                raw: "hello".into(),
                rendered: "hello".into(),
                source: BodySource::Plain,
                metadata: crate::app::BodyViewMetadata::default(),
            },
            labels: vec!["INBOX".into()],
            attachments: vec![],
            selected: true,
            bulk_selected: true,
            has_unsubscribe: false,
            signature_expanded: false,
        };

        let snapshot = render_to_string(70, 18, |frame| {
            let mut html_images = HashMap::new();
            draw(
                frame,
                Rect::new(0, 0, 70, 18),
                &[block],
                0,
                &ActivePane::MessageView,
                &Theme::default(),
                &mut html_images,
            );
        });

        assert!(snapshot.contains("Selected"));
    }

    #[test]
    fn raw_body_rendering_preserves_quotes_and_signature_markers() {
        let raw = "Hello\n> quoted line\n-- \nSignature";
        let block = ThreadMessageBlock {
            envelope: envelope(),
            body_state: BodyViewState::Ready {
                raw: raw.into(),
                rendered: raw.into(),
                source: BodySource::Plain,
                metadata: BodyViewMetadata {
                    mode: BodyViewMode::Text,
                    provenance: Some(BodyPartSource::Exact),
                    reader_applied: false,
                    ..BodyViewMetadata::default()
                },
            },
            labels: vec![],
            attachments: vec![],
            selected: true,
            bulk_selected: false,
            has_unsubscribe: false,
            signature_expanded: false,
        };

        let rendered = render_to_string(80, 18, |frame| {
            let mut html_images = HashMap::new();
            draw(
                frame,
                Rect::new(0, 0, 80, 18),
                &[block],
                0,
                &ActivePane::MessageView,
                &Theme::default(),
                &mut html_images,
            );
        });

        assert!(rendered.contains("> quoted line"));
        assert!(rendered.contains("-- "));
        assert!(!rendered.contains("signature ("));
    }

    #[test]
    fn html_body_rendering_labels_inline_embedded_and_remote_images() {
        let html = concat!(
            "<p>Hello</p>",
            "<img alt=\"Logo\" src=\"cid:logo@example.com\">",
            "<img alt=\"Badge\" src=\"data:image/png;base64,AAAA\">",
            "<img alt=\"Hero\" src=\"https://example.com/hero.png\">"
        );
        let block = ThreadMessageBlock {
            envelope: envelope(),
            body_state: BodyViewState::Ready {
                raw: html.into(),
                rendered: html.into(),
                source: BodySource::Html,
                metadata: BodyViewMetadata {
                    mode: BodyViewMode::Html,
                    provenance: Some(BodyPartSource::Exact),
                    inline_images: true,
                    remote_content_available: true,
                    remote_content_enabled: false,
                    ..BodyViewMetadata::default()
                },
            },
            labels: vec![],
            attachments: vec![],
            selected: true,
            bulk_selected: false,
            has_unsubscribe: false,
            signature_expanded: false,
        };

        let rendered = render_to_string(120, 30, |frame| {
            let mut html_images = HashMap::new();
            draw(
                frame,
                Rect::new(0, 0, 120, 30),
                &[block],
                0,
                &ActivePane::MessageView,
                &Theme::default(),
                &mut html_images,
            );
        });

        assert!(rendered.contains("Logo"));
        assert!(rendered.contains("Badge"));
        assert!(rendered.contains("Hero"));
        assert!(rendered.contains("remote:off"));
    }
}

fn body_metadata_lines(
    metadata: &BodyViewMetadata,
    source: &BodySource,
    theme: &Theme,
) -> Vec<Line<'static>> {
    let mut chips = Vec::new();
    chips.push(body_chip(
        match metadata.mode {
            BodyViewMode::Text => "text",
            BodyViewMode::Html => "html",
        },
        theme,
        theme.accent,
    ));
    chips.push(body_chip(
        match source {
            BodySource::Plain => "plain",
            BodySource::Html => "html-part",
            BodySource::Snippet => "snippet",
        },
        theme,
        theme.success,
    ));
    if let Some(provenance) = metadata.provenance {
        chips.push(body_chip(
            match provenance {
                mxr_core::types::BodyPartSource::Exact => "source:exact",
                mxr_core::types::BodyPartSource::DerivedFromPlain => "source:plain-derived",
                mxr_core::types::BodyPartSource::DerivedFromHtml => "source:html-derived",
            },
            theme,
            theme.text_muted,
        ));
    }
    if metadata.reader_applied {
        chips.push(body_chip("reader", theme, theme.warning));
    }
    if metadata.flowed {
        chips.push(body_chip("flowed", theme, theme.link_fg));
    }
    if metadata.inline_images {
        chips.push(body_chip("inline-images", theme, theme.success));
    }
    if metadata.mode == BodyViewMode::Html && metadata.remote_content_available {
        chips.push(body_chip(
            if metadata.remote_content_enabled {
                "remote:on"
            } else {
                "remote:off"
            },
            theme,
            if metadata.remote_content_enabled {
                theme.success
            } else {
                theme.warning
            },
        ));
    }
    if let (Some(original), Some(cleaned)) = (metadata.original_lines, metadata.cleaned_lines) {
        chips.push(body_chip(
            &format!("reader:{cleaned}/{original}"),
            theme,
            theme.text_muted,
        ));
    }

    if chips.is_empty() {
        Vec::new()
    } else {
        vec![Line::from(chips), Line::from("")]
    }
}

fn body_chip(label: &str, theme: &Theme, fg: Color) -> Span<'static> {
    Span::styled(
        format!(" {label} "),
        Style::default().fg(fg).bg(theme.hint_bar_bg),
    )
}

fn process_body_lines(
    raw: &str,
    theme: &Theme,
    signature_expanded: bool,
    reader_applied: bool,
) -> Vec<Line<'static>> {
    if !reader_applied {
        return raw
            .lines()
            .map(|line| {
                if line.is_empty() {
                    Line::from("")
                } else {
                    style_line_with_links(line, theme)
                }
            })
            .collect();
    }

    let mut lines: Vec<Line<'static>> = Vec::new();
    let mut quote_buffer: Vec<String> = Vec::new();
    let mut in_signature = false;
    let mut signature_lines: Vec<String> = Vec::new();
    let mut consecutive_blanks: u32 = 0;

    for line in raw.lines() {
        // Signature detection
        if line == "-- " || line == "--" {
            flush_quotes(&mut quote_buffer, &mut lines, theme);
            in_signature = true;
            continue;
        }

        // Blank line collapsing
        if line.trim().is_empty() {
            if in_signature {
                signature_lines.push(String::new());
                continue;
            }
            flush_quotes(&mut quote_buffer, &mut lines, theme);
            consecutive_blanks += 1;
            if consecutive_blanks <= 2 {
                lines.push(Line::from(""));
            }
            continue;
        }
        consecutive_blanks = 0;

        if in_signature {
            signature_lines.push(line.to_string());
            continue;
        }

        // Quote detection
        if line.starts_with('>') {
            quote_buffer.push(line.to_string());
            continue;
        }

        // Regular line — flush any pending quotes first
        flush_quotes(&mut quote_buffer, &mut lines, theme);
        lines.push(style_line_with_links(line, theme));
    }

    // Flush remaining
    flush_quotes(&mut quote_buffer, &mut lines, theme);

    if !signature_lines.is_empty() {
        if signature_expanded {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "-- signature --",
                Style::default()
                    .fg(theme.signature_fg)
                    .add_modifier(Modifier::ITALIC),
            )));
            for line in signature_lines {
                lines.push(Line::from(Span::styled(
                    line,
                    Style::default().fg(theme.signature_fg),
                )));
            }
        } else {
            let count = signature_lines.len();
            lines.push(Line::from(Span::styled(
                format!("-- signature ({} lines, press S to expand) --", count),
                Style::default()
                    .fg(theme.text_muted)
                    .add_modifier(Modifier::ITALIC),
            )));
        }
    }

    lines
}

fn render_blocks(
    frame: &mut Frame,
    area: Rect,
    scroll_offset: u16,
    blocks: Vec<RenderBlock>,
    theme: &Theme,
    html_images: &mut HashMap<MessageId, HashMap<String, HtmlImageEntry>>,
) {
    if area.is_empty() {
        return;
    }

    let mut y = area.y;
    let mut remaining_scroll = scroll_offset;

    for block in blocks {
        if y >= area.bottom() {
            break;
        }

        let block_height = render_block_height(&block, area.width, area.height, html_images);
        if block_height == 0 {
            continue;
        }
        if remaining_scroll >= block_height {
            remaining_scroll -= block_height;
            continue;
        }

        let visible_height = (block_height - remaining_scroll).min(area.bottom() - y);
        let block_area = Rect::new(area.x, y, area.width, visible_height);

        match block {
            RenderBlock::Text(lines) => {
                render_text_block(frame, block_area, &lines, remaining_scroll)
            }
            RenderBlock::Image(image) => render_image_block(
                frame,
                block_area,
                &image,
                remaining_scroll,
                block_height,
                theme,
                html_images,
            ),
        }

        y += visible_height;
        remaining_scroll = 0;
    }
}

fn render_block_height(
    block: &RenderBlock,
    width: u16,
    viewport_height: u16,
    html_images: &HashMap<MessageId, HashMap<String, HtmlImageEntry>>,
) -> u16 {
    if width == 0 {
        return 0;
    }

    match block {
        RenderBlock::Text(lines) => Paragraph::new(lines.clone())
            .wrap(Wrap { trim: false })
            .line_count(width) as u16,
        RenderBlock::Image(image) => {
            image_block_total_height(image, width, viewport_height, html_images)
        }
    }
}

fn render_text_block(frame: &mut Frame, area: Rect, lines: &[Line<'static>], scroll: u16) {
    if area.is_empty() {
        return;
    }

    let paragraph = Paragraph::new(lines.to_vec())
        .wrap(Wrap { trim: false })
        .scroll((scroll, 0));
    frame.render_widget(paragraph, area);
}

fn image_block_total_height(
    image: &HtmlImageBlock,
    width: u16,
    viewport_height: u16,
    html_images: &HashMap<MessageId, HashMap<String, HtmlImageEntry>>,
) -> u16 {
    let caption_height = u16::from(!image.label.trim().is_empty());
    let image_height = html_images
        .get(&image.message_id)
        .and_then(|assets| assets.get(&image.source))
        .map(|entry| entry.height_for(width, max_image_height(viewport_height)))
        .unwrap_or(3);
    image_height + caption_height
}

fn render_image_block(
    frame: &mut Frame,
    area: Rect,
    image: &HtmlImageBlock,
    scroll: u16,
    total_height: u16,
    theme: &Theme,
    html_images: &mut HashMap<MessageId, HashMap<String, HtmlImageEntry>>,
) {
    let Some(entry) = html_images
        .get_mut(&image.message_id)
        .and_then(|assets| assets.get_mut(&image.source))
    else {
        render_text_block(
            frame,
            area,
            &image_placeholder_lines(image, None, false, theme),
            scroll,
        );
        return;
    };

    let caption_height = u16::from(!image.label.trim().is_empty());
    let image_height =
        entry.height_for(area.width, max_image_height(total_height.max(area.height)));
    let fully_visible = scroll == 0 && area.height >= total_height;

    if fully_visible {
        if let Some(protocol) = entry.ready_protocol_mut() {
            if image_height > 0 {
                let image_area =
                    Rect::new(area.x, area.y, area.width, image_height.min(area.height));
                frame.render_widget(Clear, image_area);
                frame.render_widget(
                    Block::new().style(Style::default().bg(theme.hint_bar_bg)),
                    image_area,
                );
                frame.render_stateful_widget(
                    StatefulImage::default().resize(Resize::Fit(None)),
                    image_area,
                    protocol,
                );
            }

            if caption_height > 0 && area.height > image_height {
                let caption_area =
                    Rect::new(area.x, area.y + image_height, area.width, caption_height);
                render_text_block(
                    frame,
                    caption_area,
                    &[Line::from(Span::styled(
                        image.label.clone(),
                        Style::default()
                            .fg(theme.text_muted)
                            .add_modifier(Modifier::ITALIC),
                    ))],
                    0,
                );
            }
            return;
        }
    }

    render_text_block(
        frame,
        area,
        &image_placeholder_lines(image, Some(entry), !fully_visible, theme),
        scroll,
    );
}

fn max_image_height(viewport_height: u16) -> u16 {
    viewport_height.saturating_sub(4).clamp(6, 18)
}

fn render_html_blocks(
    message_id: &MessageId,
    html: &str,
    width: u16,
    theme: &Theme,
    remote_content_enabled: bool,
) -> Vec<RenderBlock> {
    let width = usize::from(width.max(20));
    let render_tree = match html2text::parse(html.as_bytes()) {
        Ok(render_tree) => render_tree,
        Err(_) => {
            return vec![RenderBlock::Text(process_body_lines(
                html, theme, false, false,
            ))];
        }
    };
    let tagged_lines = match html2text::config::rich().render_to_lines(render_tree, width) {
        Ok(lines) => lines,
        Err(_) => {
            return vec![RenderBlock::Text(process_body_lines(
                html, theme, false, false,
            ))];
        }
    };

    let mut blocks = Vec::new();
    let mut text_lines = Vec::new();

    for line in tagged_lines {
        for block in rich_line_to_blocks(message_id, line, theme, remote_content_enabled) {
            match block {
                RenderBlock::Text(line) => text_lines.extend(line),
                RenderBlock::Image(image) => {
                    if !text_lines.is_empty() {
                        blocks.push(RenderBlock::Text(std::mem::take(&mut text_lines)));
                    }
                    blocks.push(RenderBlock::Image(image));
                }
            }
        }
    }

    if !text_lines.is_empty() {
        blocks.push(RenderBlock::Text(text_lines));
    }

    blocks
}

fn rich_line_to_blocks(
    message_id: &MessageId,
    line: html2text::render::TaggedLine<Vec<RichAnnotation>>,
    theme: &Theme,
    remote_content_enabled: bool,
) -> Vec<RenderBlock> {
    let tagged = line.tagged_strings().collect::<Vec<_>>();
    let image_specs = tagged
        .iter()
        .filter_map(|tagged| {
            tagged.tag.iter().find_map(|annotation| match annotation {
                RichAnnotation::Image(source) => Some(HtmlImageBlock {
                    message_id: message_id.clone(),
                    source: source.clone(),
                    label: tagged.s.trim().to_string(),
                }),
                _ => None,
            })
        })
        .collect::<Vec<_>>();

    let has_non_image_text = tagged.iter().any(|tagged| {
        let text = tagged.s.trim();
        !text.is_empty()
            && !tagged
                .tag
                .iter()
                .any(|annotation| matches!(annotation, RichAnnotation::Image(_)))
    });

    if !image_specs.is_empty() && !has_non_image_text {
        image_specs.into_iter().map(RenderBlock::Image).collect()
    } else {
        let spans = tagged
            .into_iter()
            .filter_map(|tagged| {
                let span = rich_span(tagged.s.clone(), &tagged.tag, theme, remote_content_enabled);
                (!span.content.is_empty()).then_some(span)
            })
            .collect::<Vec<_>>();

        if spans.is_empty() {
            vec![RenderBlock::Text(vec![Line::from("")])]
        } else {
            vec![RenderBlock::Text(vec![Line::from(spans)])]
        }
    }
}

fn rich_span(
    mut text: String,
    annotations: &[RichAnnotation],
    theme: &Theme,
    remote_content_enabled: bool,
) -> Span<'static> {
    let mut style = Style::default().fg(theme.text_primary);

    for annotation in annotations {
        match annotation {
            RichAnnotation::Default => {}
            RichAnnotation::Link(url) => {
                style = style.fg(theme.link_fg).add_modifier(Modifier::UNDERLINED);
                if text.is_empty() {
                    text = url.clone();
                }
            }
            RichAnnotation::Image(src) => {
                style = style
                    .fg(theme.success)
                    .add_modifier(Modifier::ITALIC | Modifier::BOLD);
                text = image_placeholder(&text, src, remote_content_enabled);
            }
            RichAnnotation::Emphasis => {
                style = style.add_modifier(Modifier::ITALIC);
            }
            RichAnnotation::Strong => {
                style = style.add_modifier(Modifier::BOLD);
            }
            RichAnnotation::Strikeout => {
                style = style.add_modifier(Modifier::CROSSED_OUT);
            }
            RichAnnotation::Code => {
                style = style
                    .bg(theme.selection_bg)
                    .fg(theme.selection_fg)
                    .add_modifier(Modifier::BOLD);
            }
            RichAnnotation::Preformat(_) => {
                style = style.bg(theme.hint_bar_bg);
            }
            RichAnnotation::Colour(colour) => {
                style = style.fg(Color::Rgb(colour.r, colour.g, colour.b));
            }
            RichAnnotation::BgColour(colour) => {
                style = style.bg(Color::Rgb(colour.r, colour.g, colour.b));
            }
            _ => {}
        }
    }

    Span::styled(text, style)
}

fn image_placeholder_lines(
    image: &HtmlImageBlock,
    entry: Option<&HtmlImageEntry>,
    clipped: bool,
    theme: &Theme,
) -> Vec<Line<'static>> {
    let label = if image.label.trim().is_empty() {
        "image"
    } else {
        image.label.trim()
    };

    let (headline, detail) = match entry {
        Some(_entry) if clipped => (
            format!("[image: {label}]"),
            Some("scroll to reveal full image".to_string()),
        ),
        Some(entry) => match entry.asset.status {
            HtmlImageAssetStatus::Blocked => (
                format!("[remote image blocked: {label}]"),
                entry.asset.detail.clone(),
            ),
            HtmlImageAssetStatus::Missing => (
                format!("[inline image missing: {label}]"),
                entry.asset.detail.clone(),
            ),
            HtmlImageAssetStatus::Unsupported => (
                format!("[unsupported image: {label}]"),
                entry.asset.detail.clone(),
            ),
            HtmlImageAssetStatus::Failed => (
                format!("[image unavailable: {label}]"),
                entry.asset.detail.clone().or_else(|| match &entry.render {
                    HtmlImageRenderState::Failed(message) => Some(message.clone()),
                    HtmlImageRenderState::Pending | HtmlImageRenderState::Ready(_) => None,
                }),
            ),
            HtmlImageAssetStatus::Ready => match &entry.render {
                HtmlImageRenderState::Pending => (
                    format!("[loading image: {label}]"),
                    entry.asset.detail.clone(),
                ),
                HtmlImageRenderState::Ready(_) => (
                    format!("[image: {label}]"),
                    Some(image_source_label(entry.asset.kind).to_string()),
                ),
                HtmlImageRenderState::Failed(message) => (
                    format!("[image unavailable: {label}]"),
                    Some(message.clone()),
                ),
            },
        },
        None => (
            format!("[image unavailable: {label}]"),
            Some("image asset not loaded".into()),
        ),
    };

    let mut lines = vec![Line::from(Span::styled(
        headline,
        Style::default()
            .fg(theme.success)
            .add_modifier(Modifier::ITALIC | Modifier::BOLD),
    ))];
    if let Some(detail) = detail.filter(|detail| !detail.trim().is_empty()) {
        lines.push(Line::from(Span::styled(
            detail,
            Style::default().fg(theme.text_muted),
        )));
    }
    if !image.label.trim().is_empty() {
        lines.push(Line::from(Span::styled(
            image.label.clone(),
            Style::default()
                .fg(theme.text_muted)
                .add_modifier(Modifier::ITALIC),
        )));
    }
    while lines.len() < 3 {
        lines.push(Line::from(""));
    }
    lines
}

fn image_placeholder(label: &str, src: &str, remote_content_enabled: bool) -> String {
    let trimmed = label.trim();
    let remote = src.starts_with("http://") || src.starts_with("https://");
    let descriptor = if trimmed.is_empty() { "image" } else { trimmed };

    if remote && !remote_content_enabled {
        format!("[remote image blocked: {descriptor}]")
    } else if src.starts_with("cid:") {
        format!("[inline image: {descriptor}]")
    } else if src.starts_with("data:") {
        format!("[embedded image: {descriptor}]")
    } else if remote {
        format!("[remote image: {descriptor}]")
    } else {
        format!("[image: {descriptor}]")
    }
}

fn image_source_label(kind: HtmlImageSourceKind) -> &'static str {
    match kind {
        HtmlImageSourceKind::Cid => "inline attachment",
        HtmlImageSourceKind::DataUri => "embedded image",
        HtmlImageSourceKind::Remote => "remote image",
        HtmlImageSourceKind::ContentLocation => "content-location image",
        HtmlImageSourceKind::File => "attachment image",
    }
}

fn human_size(size_bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * 1024;

    if size_bytes >= MB {
        format!("{:.1} MB", size_bytes as f64 / MB as f64)
    } else if size_bytes >= KB {
        format!("{:.1} KB", size_bytes as f64 / KB as f64)
    } else {
        format!("{size_bytes} B")
    }
}

fn flush_quotes(buffer: &mut Vec<String>, lines: &mut Vec<Line<'static>>, theme: &Theme) {
    if buffer.is_empty() {
        return;
    }

    let quote_style = Style::default().fg(theme.quote_fg);

    if buffer.len() <= 3 {
        for line in buffer.drain(..) {
            let cleaned = line
                .trim_start_matches('>')
                .trim_start_matches(' ')
                .to_string();
            lines.push(Line::from(vec![
                Span::styled("│ ", Style::default().fg(theme.accent_dim)),
                Span::styled(cleaned, quote_style),
            ]));
        }
    } else {
        for line in &buffer[..2] {
            let cleaned = line
                .trim_start_matches('>')
                .trim_start_matches(' ')
                .to_string();
            lines.push(Line::from(vec![
                Span::styled("│ ", Style::default().fg(theme.accent_dim)),
                Span::styled(cleaned, quote_style),
            ]));
        }
        let hidden = buffer.len() - 2;
        lines.push(Line::from(Span::styled(
            format!("  ┆ ... {hidden} more quoted lines ..."),
            Style::default()
                .fg(theme.quote_fg)
                .add_modifier(Modifier::ITALIC),
        )));
        buffer.clear();
    }
}

/// Split a line into spans, highlighting URLs in link_fg with underline.
fn style_line_with_links(line: &str, theme: &Theme) -> Line<'static> {
    let link_style = Style::default()
        .fg(theme.link_fg)
        .add_modifier(Modifier::UNDERLINED);

    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut rest = line;

    while let Some(start) = rest.find("http://").or_else(|| rest.find("https://")) {
        // Text before the URL
        if start > 0 {
            spans.push(Span::raw(rest[..start].to_string()));
        }

        // Find end of URL (whitespace, angle bracket, or end of string)
        let url_rest = &rest[start..];
        let end = url_rest
            .find(|c: char| c.is_whitespace() || c == '>' || c == ')' || c == ']' || c == '"')
            .unwrap_or(url_rest.len());

        let url = &url_rest[..end];
        // Strip trailing punctuation that's probably not part of the URL
        let url_trimmed = url.trim_end_matches(['.', ',', ';', ':', '!', '?']);
        let trimmed_len = url_trimmed.len();

        spans.push(Span::styled(url_trimmed.to_string(), link_style));

        // Any trailing punctuation goes back as plain text
        if trimmed_len < end {
            spans.push(Span::raw(url_rest[trimmed_len..end].to_string()));
        }

        rest = &rest[start + end..];
    }

    // Remaining text after last URL
    if !rest.is_empty() {
        spans.push(Span::raw(rest.to_string()));
    }

    if spans.is_empty() {
        Line::from(line.to_string())
    } else {
        Line::from(spans)
    }
}
