use mxr_protocol::{ComposeProposalData, TextChangeKindData};
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap};
use std::collections::BTreeMap;
use std::sync::OnceLock;

const CONTEXT: &str = "compose-assist-proof";

struct ProofRenderOptions<'a> {
    note_input: Option<&'a str>,
    version: (usize, usize),
    scroll_offset: u16,
    theme: &'a crate::theme::Theme,
    no_color: bool,
}

struct ProofTextContext<'a> {
    proposal: &'a ComposeProposalData,
    theme: &'a crate::theme::Theme,
    no_color: bool,
}

pub fn draw(
    frame: &mut Frame,
    area: Rect,
    proposal: &ComposeProposalData,
    note_input: Option<&str>,
    version: (usize, usize),
    scroll_offset: u16,
    theme: &crate::theme::Theme,
) -> u16 {
    draw_with_color_mode(
        frame,
        area,
        proposal,
        ProofRenderOptions {
            note_input,
            version,
            scroll_offset,
            theme,
            no_color: std::env::var_os("NO_COLOR").is_some(),
        },
    )
}

pub fn draw_model_picker(
    frame: &mut Frame,
    area: Rect,
    models: &mxr_protocol::AssistantModelsData,
    selected_index: usize,
    theme: &crate::theme::Theme,
) {
    let popup = super::centered_rect(66, 58, area);
    frame.render_widget(Clear, popup);
    let block = Block::default()
        .title(" Choose a compose model · ↑/↓ move · Enter select · Esc cancel ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.accent))
        .style(Style::default().bg(theme.modal_bg));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);
    let mut items = vec![ListItem::new(format!(
        "Use compose default{}",
        models
            .current
            .as_deref()
            .map(|model| format!(" · {model}"))
            .unwrap_or_default()
    ))
    .style(Style::default().fg(theme.text_primary))];
    items.extend(models.choices.iter().map(|choice| {
        let location = if choice.cloud {
            "cloud"
        } else {
            "this machine"
        };
        let availability = choice
            .unavailable_reason
            .as_deref()
            .map_or("available", |reason| reason);
        let current = if models.current.as_deref() == Some(choice.id.as_str()) {
            " · default"
        } else {
            ""
        };
        let line = format!(
            "{} · {} · {}{} — {}",
            choice.label, location, choice.sees, current, availability
        );
        let style = if choice.available {
            Style::default().fg(theme.text_primary)
        } else {
            Style::default().fg(theme.text_muted)
        };
        ListItem::new(line).style(style)
    }));
    let selected = Some(selected_index.min(items.len() - 1));
    let list = List::new(items)
        .highlight_style(
            Style::default()
                .bg(theme.selection_bg)
                .fg(theme.text_primary)
                .bold(),
        )
        .highlight_symbol("› ");
    let mut state = ListState::default().with_selected(selected);
    frame.render_stateful_widget(list, inner, &mut state);
}

fn draw_with_color_mode(
    frame: &mut Frame,
    area: Rect,
    proposal: &ComposeProposalData,
    options: ProofRenderOptions<'_>,
) -> u16 {
    let ProofRenderOptions {
        note_input,
        version,
        scroll_offset,
        theme,
        no_color,
    } = options;
    let popup = super::centered_rect(84, 72, area);
    frame.render_widget(Clear, popup);
    let block = Block::default()
        .title(format!(
            " Writing proof {}/{} · {} accept · {} accept/edit ",
            version.0 + 1,
            version.1,
            key("ComposeProofAccept"),
            key("ComposeProofAcceptEdit")
        ))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.accent))
        .style(Style::default().bg(theme.modal_bg));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let mut lines = proof_lines(proposal, theme, no_color);
    if !proposal.gaps.is_empty() {
        let labels = proposal
            .gaps
            .iter()
            .map(|gap| gap.label.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        lines.push(Line::from(Span::styled(
            format!("Warning: fill in {labels}; sending remains blocked."),
            Style::default().fg(theme.warning),
        )));
    }
    if let Some(input) = note_input {
        lines.push(Line::from(format!(
            "Revise: {input}  (Enter submit · Escape cancel)"
        )));
    }
    lines.push(Line::from(Span::styled(
        proposal.provenance.summary_line(),
        Style::default().fg(theme.text_muted),
    )));
    lines.push(Line::from(Span::styled(
        format!("Style {}/100", proposal.style_score),
        Style::default().fg(theme.text_muted),
    )));
    for mark in &proposal.style_marks {
        lines.push(Line::from(Span::styled(
            format!("{}: {}", mark.name, mark.context),
            Style::default().fg(theme.warning),
        )));
    }
    lines.push(Line::from(Span::styled(
        format!(
            "{} another model · {} revise · {} keep · {} previous · {} next · {}/{} scroll · {}/{} page · {}/{} top/end",
            key("ComposeProofModel"),
            key("ComposeProofRevise"),
            key("ComposeProofKeep"),
            key("ComposeProofPrevious"),
            key("ComposeProofNext"),
            key("ComposeProofScrollUp"),
            key("ComposeProofScrollDown"),
            key("ComposeProofPageUp"),
            key("ComposeProofPageDown"),
            key("ComposeProofScrollTop"),
            key("ComposeProofScrollBottom"),
        ),
        Style::default().fg(theme.text_muted),
    )));
    let paragraph = Paragraph::new(lines).wrap(Wrap { trim: false });
    let max_scroll = paragraph
        .line_count(inner.width)
        .saturating_sub(usize::from(inner.height));
    let scroll = usize::from(scroll_offset)
        .min(max_scroll)
        .min(u16::MAX as usize) as u16;
    frame.render_widget(paragraph.scroll((scroll, 0)), inner);
    scroll
}

fn proof_keys() -> &'static BTreeMap<&'static str, &'static str> {
    static KEYS: OnceLock<BTreeMap<&'static str, &'static str>> = OnceLock::new();
    KEYS.get_or_init(|| {
        mxr_keymap::defaults()
            .actions
            .iter()
            .flat_map(|action| &action.tui)
            .filter(|binding| binding.contexts.includes(CONTEXT))
            .filter_map(|binding| {
                binding
                    .keys
                    .first()
                    .map(|key| (binding.name.as_str(), key.as_str()))
            })
            .collect()
    })
}

fn key(action: &str) -> &'static str {
    proof_keys().get(action).copied().unwrap_or("?")
}

fn proof_lines(
    proposal: &ComposeProposalData,
    theme: &crate::theme::Theme,
    no_color: bool,
) -> Vec<Line<'static>> {
    let context = ProofTextContext {
        proposal,
        theme,
        no_color,
    };
    let mut output = vec![Line::default()];
    let mut final_offset = 0usize;
    for change in &proposal.changes {
        let base_style = match change.kind {
            TextChangeKindData::Kept => Style::default().fg(theme.text_primary),
            TextChangeKindData::Added => Style::default().fg(theme.accent),
            TextChangeKindData::Removed => Style::default()
                .fg(theme.error)
                .add_modifier(Modifier::CROSSED_OUT),
        };
        let is_final_text = change.kind != TextChangeKindData::Removed;
        append_styled_text(
            &mut output,
            &change.text,
            change.kind,
            base_style,
            is_final_text.then_some(final_offset),
            &context,
        );
        if is_final_text {
            final_offset += change.text.len();
        }
    }
    output
}

fn append_styled_text(
    lines: &mut Vec<Line<'static>>,
    text: &str,
    kind: TextChangeKindData,
    base_style: Style,
    final_offset: Option<usize>,
    context: &ProofTextContext<'_>,
) {
    let mut start = 0usize;
    for (index, ch) in text.char_indices() {
        if ch == '\n' {
            if start < index {
                append_segment(
                    lines,
                    &text[start..index],
                    start,
                    kind,
                    base_style,
                    final_offset,
                    context,
                );
            }
            lines.push(Line::default());
            start = index + ch.len_utf8();
        }
    }
    if start < text.len() {
        append_segment(
            lines,
            &text[start..],
            start,
            kind,
            base_style,
            final_offset,
            context,
        );
    }
}

fn append_segment(
    lines: &mut [Line<'static>],
    text: &str,
    text_offset: usize,
    kind: TextChangeKindData,
    base_style: Style,
    final_offset: Option<usize>,
    context: &ProofTextContext<'_>,
) {
    let mut segment_start = 0usize;
    let mut warning_active = false;
    for (index, _) in text
        .char_indices()
        .chain(std::iter::once((text.len(), '\0')))
    {
        let is_gap = final_offset.is_some_and(|offset| {
            let at = offset + text_offset + index;
            context
                .proposal
                .gaps
                .iter()
                .any(|gap| gap.start as usize <= at && at < gap.end as usize)
        });
        if index == text.len() || is_gap != warning_active {
            if segment_start < index {
                let value = &text[segment_start..index];
                let mut rendered = String::new();
                if context.no_color {
                    if warning_active {
                        rendered.push_str("[[");
                        rendered.push_str(value);
                        rendered.push_str("]]");
                    } else {
                        match kind {
                            TextChangeKindData::Added => {
                                rendered.push_str("{+");
                                rendered.push_str(value);
                                rendered.push_str("+}");
                            }
                            TextChangeKindData::Removed => {
                                rendered.push_str("[-");
                                rendered.push_str(value);
                                rendered.push_str("-]");
                            }
                            TextChangeKindData::Kept => rendered.push_str(value),
                        }
                    }
                } else {
                    rendered.push_str(value);
                }
                let style = if warning_active {
                    Style::default()
                        .fg(context.theme.warning)
                        .add_modifier(Modifier::UNDERLINED)
                } else {
                    base_style
                };
                if let Some(line) = lines.last_mut() {
                    line.spans.push(Span::styled(rendered, style));
                }
            }
            segment_start = index;
            warning_active = is_gap;
        }
    }
}

pub fn no_color_diff(proposal: &ComposeProposalData) -> String {
    proposal
        .changes
        .iter()
        .map(|change| match change.kind {
            TextChangeKindData::Kept => change.text.clone(),
            TextChangeKindData::Added => format!("{{+{}+}}", change.text),
            TextChangeKindData::Removed => format!("[-{}-]", change.text),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use mxr_protocol::{
        AiLocalityData, AssistActionData, DraftProvenanceData, GapData, TextChangeData,
    };
    use ratatui::backend::TestBackend;

    fn proposal() -> ComposeProposalData {
        ComposeProposalData {
            proposal_id: "p".into(),
            action: AssistActionData::Polish,
            base: "Hi there".into(),
            text: "Hello there".into(),
            changes: vec![
                TextChangeData {
                    kind: TextChangeKindData::Removed,
                    text: "Hi".into(),
                },
                TextChangeData {
                    kind: TextChangeKindData::Added,
                    text: "Hello".into(),
                },
                TextChangeData {
                    kind: TextChangeKindData::Kept,
                    text: " there".into(),
                },
            ],
            gaps: vec![],
            provenance: DraftProvenanceData {
                model: "test".into(),
                locality: AiLocalityData::Local,
                history_used: false,
                voice_examples: vec![],
                conversation: vec![],
                rewrite: None,
            },
            yours_changed: vec![],
            composer_version: "1".into(),
            style_marks: Vec::new(),
            style_score: 100,
            style_version: String::new(),
        }
    }

    fn rendered(no_color: bool, scroll: u16, proposal: &ComposeProposalData) -> TestBackend {
        let backend = TestBackend::new(100, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                let area = frame.area();
                draw_with_color_mode(
                    frame,
                    area,
                    proposal,
                    ProofRenderOptions {
                        note_input: None,
                        version: (0, 1),
                        scroll_offset: scroll,
                        theme: &crate::theme::Theme::default(),
                        no_color,
                    },
                );
            })
            .unwrap();
        terminal.backend().clone()
    }

    #[test]
    fn plain_text_proof_marks_removed_and_added_words() {
        assert_eq!(no_color_diff(&proposal()), "[-Hi-]{+Hello+} there");
    }

    #[test]
    fn test_backend_preserves_paragraphs_and_warns_on_gap_ranges() {
        let mut draft = proposal();
        draft.text = "Hello\nkept body\nsignoff".into();
        draft.changes = vec![TextChangeData {
            kind: TextChangeKindData::Kept,
            text: draft.text.clone(),
        }];
        draft.gaps.push(GapData {
            label: "meeting time".into(),
            start: 11,
            end: 15,
        });
        let backend = rendered(false, 0, &draft);
        let buffer = backend.buffer();
        let rows: Vec<String> = (0..buffer.area.height)
            .map(|y| {
                (0..buffer.area.width)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>()
            })
            .collect();
        assert!(rows.iter().any(|row| row.contains("Hello")));
        assert!(rows.iter().any(|row| row.contains("kept body")));
        assert!(rows.iter().any(|row| row.contains("signoff")));
        let gap_cell = (0..buffer.area.height)
            .flat_map(|y| (0..buffer.area.width).map(move |x| (x, y)))
            .find(|(x, y)| buffer[(*x, *y)].symbol() == "b")
            .unwrap();
        assert_eq!(buffer[gap_cell].fg, crate::theme::Theme::default().warning);

        let no_color_backend = rendered(true, 0, &draft);
        let no_color_rows: Vec<String> = (0..no_color_backend.buffer().area.height)
            .map(|y| {
                (0..no_color_backend.buffer().area.width)
                    .map(|x| no_color_backend.buffer()[(x, y)].symbol())
                    .collect::<String>()
            })
            .collect();
        assert!(no_color_rows
            .iter()
            .any(|row| row.contains("kept [[body]]")));
        assert!(no_color_rows.iter().any(|row| row.contains("meeting time")));
    }

    #[test]
    fn test_backend_scroll_reaches_the_end_of_a_long_proof() {
        let mut draft = proposal();
        draft.changes = vec![TextChangeData {
            kind: TextChangeKindData::Kept,
            text: (0..40)
                .map(|line| format!("draft line {line}"))
                .collect::<Vec<_>>()
                .join("\n"),
        }];
        draft.text = draft.changes[0].text.clone();
        let backend = rendered(false, u16::MAX, &draft);
        let rows: Vec<String> = (0..backend.buffer().area.height)
            .map(|y| {
                (0..backend.buffer().area.width)
                    .map(|x| backend.buffer()[(x, y)].symbol())
                    .collect::<String>()
            })
            .collect();
        assert!(rows.iter().any(|row| row.contains("draft line 39")));
        assert!(rows.iter().any(|row| row.contains("history not used")));
    }

    #[test]
    fn end_reaches_the_last_word_of_a_wrapped_paragraph() {
        let mut draft = proposal();
        draft.text = format!("{}FINAL_WORD_ANCHOR", "long draft text ".repeat(200));
        draft.changes = vec![TextChangeData {
            kind: TextChangeKindData::Kept,
            text: draft.text.clone(),
        }];
        let backend = rendered(false, u16::MAX, &draft);
        let buffer = backend.buffer();
        let text = (0..buffer.area.height)
            .map(|y| {
                (0..buffer.area.width)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n");
        assert!(text.contains("FINAL_WORD_ANCHOR"));
    }
}
