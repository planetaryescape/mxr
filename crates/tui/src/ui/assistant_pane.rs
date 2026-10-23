//! The assistant pane: the conversation, what waits for the user, and the
//! message box. Everything in it that came from email or from the model is
//! untrusted text: control characters are stripped and nothing in it is
//! followed or fetched.

use super::assistant_describe::{
    answered_with, plain_text, plain_text_with_links, preview_draft, preview_messages,
    readout_line, tool_words,
};
use super::sanitize::{one_line, strip_control_chars, truncate};
use crate::app::{AssistantFocus, AssistantPrompt, AssistantState};
use crate::theme::Theme;
use mxr_protocol::{
    AssistantCallStateData, AssistantItemData, AssistantOutcomeData, AssistantPendingData,
    AssistantProgressData, AssistantStatusData, AssistantStepTitleData,
};
use ratatui::prelude::*;
use ratatui::widgets::*;
use std::time::Instant;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

/// Approvals list this many messages, then "and N more".
const PREVIEW_ROWS: usize = 6;
/// The key that unfolds and folds a waiting approval's full message list.
const FULL_LIST_KEY: &str = "a";
/// After this long on one live phrase the running line names the model, so a
/// slow local one doesn't look stuck. Matches `SLOW_STEP_SECONDS` in
/// apps/web/src/features/assistant/wire.ts.
const SLOW_STEP_SECONDS: u64 = 20;
/// The message box grows to this many lines, then shows the newest.
const INPUT_ROWS: usize = 4;

const SUGGESTIONS: [&str; 3] = [
    "What needs a reply from me this week?",
    "Summarise what arrived today",
    "Find newsletters I never open and tidy them up",
];

/// Draws the pane and returns how far its transcript can scroll.
pub fn draw(
    frame: &mut Frame,
    area: Rect,
    state: &AssistantState,
    now: Instant,
    theme: &Theme,
) -> u16 {
    frame.render_widget(Clear, area);
    let focused = state.focus != AssistantFocus::Away;
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(if focused {
            theme.border_focused
        } else {
            theme.border_unfocused
        }))
        .title(title_line(state, theme))
        .style(Style::default().bg(theme.modal_bg));
    let inner = block.inner(area).inner(ratatui::layout::Margin::new(1, 0));
    frame.render_widget(block, area);
    if inner.width < 4 || inner.height < 4 {
        return 0;
    }
    let width = usize::from(inner.width).max(1);

    let mut footer = footer_lines(state, width, now, theme);
    let muted = Style::default().fg(theme.text_muted);
    push_wrapped(&mut footer, "", &hint_text(state), width, muted, muted);

    let mut card = card_lines(state, width, theme);
    let warnings = warning_lines(state, width, theme);
    // The card can take half the pane, or all the footer leaves while the
    // exact call is unfolded or a long approval waits (its list and the key
    // that opens the rest must show); the footer always shows. The warning
    // sits outside that room, so a long preview can't push it off while `y`
    // still approves.
    let card_room = if state.exact_call_for.is_some() || !long_approvals(state).is_empty() {
        usize::from(inner.height.saturating_sub(2)).saturating_sub(footer.len())
    } else {
        0
    }
    .max(usize::from(inner.height / 2))
    .saturating_sub(warnings.len());
    if card.len() > card_room {
        card.truncate(card_room.saturating_sub(1));
        card.push(Line::from(Span::styled(
            "…",
            Style::default().fg(theme.text_muted),
        )));
    }
    let bottom = (footer.len() + card.len() + warnings.len())
        .min(usize::from(inner.height.saturating_sub(2)));
    let chunks = Layout::vertical([
        Constraint::Min(1),
        Constraint::Length(u16::try_from(bottom).unwrap_or(u16::MAX)),
    ])
    .split(inner);

    let (body, max_scroll) = if let Some(models) = &state.models {
        let (lines, selected) = models_lines(models, width, theme);
        let height = usize::from(chunks[0].height);
        // Keep the highlighted row, with its reason, in view.
        let start = selected.end.saturating_sub(height).min(selected.start);
        (lines.into_iter().skip(start).take(height).collect(), 0)
    } else if let Some(list) = &state.list {
        (list_lines(list, width, theme), 0)
    } else {
        let lines = transcript_lines(state, width, theme);
        let height = usize::from(chunks[0].height);
        let max = lines.len().saturating_sub(height);
        let max_scroll = u16::try_from(max).unwrap_or(u16::MAX);
        let from_bottom = usize::from(state.scroll_from_bottom.min(max_scroll));
        let start = max - from_bottom;
        let visible: Vec<Line<'static>> = lines.into_iter().skip(start).take(height).collect();
        (visible, max_scroll)
    };
    frame.render_widget(Paragraph::new(body), chunks[0]);

    frame.render_widget(
        Paragraph::new(bottom_block(card, warnings, footer, chunks[1].height)),
        chunks[1],
    );
    max_scroll
}

/// The card, its warnings and the footer cut to `height` rows from the top.
/// The warnings go last: while `y` can approve a change that can't be
/// undone, its warning outlasts the card and the footer's input rows.
fn bottom_block(
    card: Vec<Line<'static>>,
    warnings: Vec<Line<'static>>,
    footer: Vec<Line<'static>>,
    height: u16,
) -> Vec<Line<'static>> {
    let height = usize::from(height);
    // The keys' last row always stays, so the pane still says what y does.
    let keys = usize::from(!footer.is_empty() && height > 1);
    let warnings: Vec<_> = warnings.into_iter().take(height - keys).collect();
    let footer_room = height - warnings.len();
    let footer_skip = footer.len().saturating_sub(footer_room);
    let card_room = footer_room - (footer.len() - footer_skip);
    let card_skip = card.len().saturating_sub(card_room);
    card.into_iter()
        .skip(card_skip)
        .chain(warnings)
        .chain(footer.into_iter().skip(footer_skip))
        .collect()
}

fn title_line(state: &AssistantState, theme: &Theme) -> Line<'static> {
    let title = state
        .conversation
        .as_ref()
        .map(|conversation| one_line(&conversation.title))
        .filter(|title| !title.trim().is_empty())
        .unwrap_or_else(|| "Assistant".into());
    let mut spans = vec![Span::styled(
        format!(" {} ", truncate(&title, 40)),
        Style::default().fg(theme.text_primary).bold(),
    )];
    if state.auto_approve() {
        spans.push(Span::styled(
            "· trusted ",
            Style::default().fg(theme.warning).bold(),
        ));
    }
    Line::from(spans)
}

/// The call ids of waiting approvals that touch more messages than the
/// card shows folded.
fn long_approvals(state: &AssistantState) -> Vec<&str> {
    state
        .pending()
        .iter()
        .filter_map(|pending| match pending {
            AssistantPendingData::Approval {
                call_id,
                preview: Some(preview),
                ..
            } if preview_messages(preview).len() > PREVIEW_ROWS => Some(call_id.as_str()),
            _ => None,
        })
        .collect()
}

/// The keys that work right now, in the pane's own words.
pub fn hint_text(state: &AssistantState) -> String {
    match &state.prompt {
        // A delete confirm can sit over the history list.
        Some(AssistantPrompt::ConfirmDelete(_)) => return "y delete · n keep".into(),
        _ if state.models.is_some() => {
            return "j/k move · Enter use this model · Esc back".into();
        }
        _ if state.list.is_some() => {
            return "j/k move · Enter open · D delete · Esc back".into();
        }
        Some(AssistantPrompt::RejectReason { .. }) => return "Enter reject · Esc back".into(),
        Some(AssistantPrompt::Other { .. }) => return "Enter save · Esc back".into(),
        None => {}
    }
    let stop = if state.running() {
        " · Ctrl-c stop"
    } else {
        ""
    };
    match state.focus {
        AssistantFocus::Input => format!("Enter send · Alt-Enter new line · Esc keys{stop}"),
        AssistantFocus::Pane => match state.pending().first() {
            Some(AssistantPendingData::Approval { .. }) => {
                let long = long_approvals(state);
                let list = if long.is_empty() {
                    ""
                } else if long
                    .iter()
                    .all(|call| state.full_list_calls.contains(*call))
                {
                    " · a show fewer"
                } else {
                    " · a show all"
                };
                format!("y approve · n reject · x exact call{list} · i write instead · Esc leave")
            }
            Some(AssistantPendingData::Question { questions, .. }) => {
                let other = questions.iter().any(|question| question.allow_other);
                let tab = if questions.len() > 1 {
                    " · Tab next"
                } else {
                    ""
                };
                format!(
                    "1-9 choose{} · Enter answer · s skip{tab}",
                    if other { " · o own answer" } else { "" }
                )
            }
            None => {
                let links = match state.reply_links().len() {
                    0 => String::new(),
                    1 => "1 open link · ".into(),
                    n => format!("1-{n} open link · "),
                };
                format!(
                    "{links}i write · N new · L history · M model · T trust · D delete · e steps{stop}"
                )
            }
        },
        AssistantFocus::Away => "@ to use the assistant".into(),
    }
}

/// Words a long text into lines no wider than `width`, keeping its breaks.
pub(crate) fn wrap(text: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let mut out = Vec::new();
    for paragraph in text.split('\n') {
        let mut line = String::new();
        let mut line_width = 0;
        for word in paragraph.split(' ') {
            let word_width = word.width();
            let gap = usize::from(!line.is_empty());
            if line_width + gap + word_width <= width {
                if gap == 1 {
                    line.push(' ');
                }
                line.push_str(word);
                line_width += gap + word_width;
                continue;
            }
            if !line.is_empty() {
                out.push(std::mem::take(&mut line));
                line_width = 0;
            }
            // A word wider than the line breaks wherever it must.
            for c in word.chars() {
                let w = c.width().unwrap_or(0);
                if line_width + w > width {
                    out.push(std::mem::take(&mut line));
                    line_width = 0;
                }
                line.push(c);
                line_width += w;
            }
        }
        out.push(line);
    }
    out
}

/// `text` wrapped under `prefix`, continuation lines indented to match.
fn push_wrapped(
    lines: &mut Vec<Line<'static>>,
    prefix: &str,
    text: &str,
    width: usize,
    prefix_style: Style,
    style: Style,
) {
    let indent = " ".repeat(prefix.width());
    let text = strip_control_chars(text).replace('\t', "  ");
    for (index, row) in wrap(&text, width.saturating_sub(prefix.width()))
        .into_iter()
        .enumerate()
    {
        let lead = if index == 0 { prefix } else { indent.as_str() };
        lines.push(Line::from(vec![
            Span::styled(lead.to_string(), prefix_style),
            Span::styled(row, style),
        ]));
    }
}

/// `value` as indented JSON, `lead` before its first line. Each line
/// wraps under its own indent so nesting stays readable when narrow.
fn push_json(
    lines: &mut Vec<Line<'static>>,
    lead: &str,
    value: &serde_json::Value,
    width: usize,
    style: Style,
) {
    let json = serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string());
    for (index, row) in json.lines().enumerate() {
        let text = row.trim_start();
        let indent = " ".repeat(row.len() - text.len());
        let prefix = if index == 0 {
            format!("{lead}{indent}")
        } else {
            indent
        };
        push_wrapped(lines, &prefix, text, width, style, style);
    }
}

/// The user's lines wear a bar, so a quote stays visibly theirs.
fn push_user(lines: &mut Vec<Line<'static>>, text: &str, width: usize, theme: &Theme) {
    let text = strip_control_chars(text);
    for row in wrap(&text, width.saturating_sub(2)) {
        lines.push(Line::from(vec![
            Span::styled("│ ", Style::default().fg(theme.text_secondary)),
            Span::styled(row, Style::default().fg(theme.text_primary).italic()),
        ]));
    }
}

fn state_glyph(state: AssistantCallStateData, theme: &Theme) -> Span<'static> {
    let (glyph, color) = match state {
        AssistantCallStateData::Running => ("◌", theme.accent),
        AssistantCallStateData::Done | AssistantCallStateData::Answered => ("✓", theme.text_muted),
        AssistantCallStateData::Approved | AssistantCallStateData::AutoApproved => {
            ("✓", theme.accent)
        }
        AssistantCallStateData::Waiting => ("◇", theme.warning),
        AssistantCallStateData::Failed => ("✕", theme.error),
        AssistantCallStateData::Declined | AssistantCallStateData::Cancelled => {
            ("⊘", theme.text_muted)
        }
        AssistantCallStateData::Interrupted => ("!", theme.warning),
    };
    Span::styled(format!("{glyph} "), Style::default().fg(color))
}

/// A step in words: the daemon's title for its state, or, from an older
/// daemon that sends none, the model's reason.
fn step_label(
    tool: &str,
    reason: &str,
    result: Option<&str>,
    state: AssistantCallStateData,
    title: Option<&AssistantStepTitleData>,
) -> String {
    if let Some(title) = title {
        let words = if state == AssistantCallStateData::Running {
            &title.running
        } else {
            &title.done
        };
        return match answered_with(result).filter(|_| tool == "ask_user") {
            Some(answer) => format!("{words}: {answer}"),
            None => words.clone(),
        };
    }
    if tool == "ask_user" {
        let reason = if reason.is_empty() {
            "Asked you a question"
        } else {
            reason
        };
        return match answered_with(result) {
            Some(answer) => format!("{reason}: {answer}"),
            None => reason.to_string(),
        };
    }
    if reason.is_empty() {
        tool_words(tool)
    } else {
        reason.to_string()
    }
}

/// The steps a reply's `failed_call_ids` names, in words. Small models
/// sometimes say they did what failed; the steps are mxr's record, so the
/// reply is followed by what they say.
fn failed_steps(items: &[AssistantItemData], failed_call_ids: &[String]) -> Vec<String> {
    items
        .iter()
        .filter_map(|item| match item {
            AssistantItemData::ToolCall {
                call_id,
                tool,
                reason,
                result,
                state,
                title,
                ..
            } if failed_call_ids.contains(call_id) => Some(step_label(
                tool,
                reason,
                result.as_deref(),
                *state,
                title.as_ref(),
            )),
            _ => None,
        })
        .collect()
}

/// The conversation as lines: what the user wrote, the replies, the steps
/// between (folded once settled), and how the last turn ended.
pub(crate) fn transcript_lines(
    state: &AssistantState,
    width: usize,
    theme: &Theme,
) -> Vec<Line<'static>> {
    let mut lines: Vec<Line<'static>> = Vec::new();
    let muted = Style::default().fg(theme.text_muted);
    let Some(conversation) = state.conversation.as_ref().filter(|c| !c.items.is_empty()) else {
        if state.conversation_id.is_some() && state.conversation.is_none() {
            lines.push(Line::from(Span::styled("Loading the conversation…", muted)));
            return lines;
        }
        if state.sending {
            if let Some(text) = state.input_sent.as_deref() {
                push_user(&mut lines, text, width, theme);
            }
            return lines;
        }
        lines.push(Line::from(Span::styled(
            "Talk to your mail.",
            Style::default().fg(theme.text_primary).bold(),
        )));
        push_wrapped(
            &mut lines,
            "",
            "Ask questions, get summaries, or have it sort, archive and draft. Every change waits for your approval first.",
            width,
            muted,
            muted,
        );
        lines.push(Line::default());
        lines.push(Line::from(Span::styled("For example:", muted)));
        for suggestion in SUGGESTIONS {
            push_wrapped(&mut lines, "  ", suggestion, width, muted, muted);
        }
        return lines;
    };

    let running = state.running();
    let items = &conversation.items;
    // Only one reply numbers its links, the latest that has any, so a
    // number always means one thing: the link `1`-`9` opens.
    let linked_reply = state.linked_reply().map(|(index, _)| index);
    let mut index = 0;
    while index < items.len() {
        if !lines.is_empty() {
            lines.push(Line::default());
        }
        match &items[index] {
            AssistantItemData::User { text, .. } => push_user(&mut lines, text, width, theme),
            AssistantItemData::Assistant {
                text,
                failed_call_ids,
                rows,
                ..
            } => {
                push_wrapped(
                    &mut lines,
                    "",
                    &if linked_reply == Some(index) {
                        plain_text_with_links(text).0
                    } else {
                        plain_text(text)
                    },
                    width,
                    Style::default(),
                    Style::default().fg(theme.text_primary),
                );
                for row in rows {
                    let line = match &row.detail {
                        Some(detail) => format!("{}  {detail}", row.title),
                        None => row.title.clone(),
                    };
                    lines.push(Line::from(vec![
                        Span::styled("  • ", muted),
                        Span::styled(
                            truncate(&one_line(&line), width.saturating_sub(4)),
                            Style::default().fg(theme.text_secondary),
                        ),
                    ]));
                }
                let failed = failed_steps(items, failed_call_ids);
                if !failed.is_empty() {
                    let error = Style::default().fg(theme.error);
                    let head = if failed.len() == 1 {
                        "This didn't happen, whatever the reply says:"
                    } else {
                        "These didn't happen, whatever the reply says:"
                    };
                    push_wrapped(&mut lines, "! ", head, width, error, error.bold());
                    for label in failed {
                        push_wrapped(&mut lines, "  ", &format!("✕ {label}"), width, error, error);
                    }
                }
            }
            AssistantItemData::Summary { text, .. } => {
                let label = " Earlier messages summarised ";
                let side = width.saturating_sub(label.width()) / 2;
                lines.push(Line::from(Span::styled(
                    format!("{}{label}{}", "─".repeat(side), "─".repeat(side)),
                    muted,
                )));
                if state.steps_expanded {
                    push_wrapped(&mut lines, "", text, width, muted, muted);
                }
            }
            AssistantItemData::ToolCall { .. } => {
                let end = items[index..]
                    .iter()
                    .position(|item| !matches!(item, AssistantItemData::ToolCall { .. }))
                    .map_or(items.len(), |offset| index + offset);
                let steps = &items[index..end];
                let live = running && end == items.len();
                let settled = !live
                    && steps.iter().all(|step| {
                        !matches!(
                            step,
                            AssistantItemData::ToolCall {
                                state: AssistantCallStateData::Running,
                                ..
                            }
                        )
                    });
                if settled && !state.steps_expanded {
                    let waiting = steps.iter().any(|step| {
                        matches!(
                            step,
                            AssistantItemData::ToolCall {
                                state: AssistantCallStateData::Waiting,
                                ..
                            }
                        )
                    });
                    let count = if steps.len() == 1 {
                        "1 step".to_string()
                    } else {
                        format!("{} steps", steps.len())
                    };
                    lines.push(Line::from(Span::styled(
                        format!(
                            "▸ {count}{}",
                            if waiting { ", waiting for you" } else { "" }
                        ),
                        muted,
                    )));
                } else {
                    for step in steps {
                        if let AssistantItemData::ToolCall {
                            tool,
                            reason,
                            state: call_state,
                            result,
                            readout,
                            title,
                            ..
                        } = step
                        {
                            let label = step_label(
                                tool,
                                reason,
                                result.as_deref(),
                                *call_state,
                                title.as_ref(),
                            );
                            let glyph = state_glyph(*call_state, theme);
                            let indent = "  ";
                            for (row_index, row) in wrap(&one_line(&label), width.saturating_sub(2))
                                .into_iter()
                                .enumerate()
                            {
                                let lead = if row_index == 0 {
                                    glyph.clone()
                                } else {
                                    Span::raw(indent)
                                };
                                lines.push(Line::from(vec![
                                    lead,
                                    Span::styled(row, Style::default().fg(theme.text_secondary)),
                                ]));
                            }
                            if let Some(found) = readout.as_ref().and_then(readout_line) {
                                let found = strip_control_chars(&one_line(&found)).into_owned();
                                for row in wrap(&found, width.saturating_sub(4)) {
                                    lines.push(Line::from(vec![
                                        Span::raw("    "),
                                        Span::styled(row, muted),
                                    ]));
                                }
                            }
                        }
                    }
                }
                index = end;
                continue;
            }
        }
        index += 1;
    }

    // The reply as it is written, until the finished one is stored.
    if let Some(AssistantProgressData::Writing { text }) = &state.progress {
        let stored = matches!(items.last(), Some(AssistantItemData::Assistant { text: last, .. }) if last == text);
        if running && !text.trim().is_empty() && !stored {
            lines.push(Line::default());
            push_wrapped(
                &mut lines,
                "",
                &plain_text(text),
                width,
                Style::default(),
                Style::default().fg(theme.text_primary),
            );
        }
    }

    if conversation.status == AssistantStatusData::Idle {
        let outcome = match conversation.last_outcome {
            // The daemon words known failures as a sentence with what to do
            // next and keeps the raw text in the detail.
            Some(AssistantOutcomeData::Failed) => Some((
                match (&conversation.last_error, &conversation.last_error_detail) {
                    (Some(sentence), Some(_)) => one_line(sentence),
                    (raw, _) => format!(
                        "That turn failed: {}. Send a message to try again.",
                        one_line(raw.as_deref().unwrap_or("unknown error"))
                    ),
                },
                theme.error,
            )),
            Some(AssistantOutcomeData::StepLimit) => Some((
                "I stopped after many steps. Send \"go on\" to keep going.".to_string(),
                theme.text_muted,
            )),
            Some(AssistantOutcomeData::Cancelled) => {
                Some(("Stopped.".to_string(), theme.text_muted))
            }
            Some(AssistantOutcomeData::Interrupted) => Some((
                "mxr restarted during this turn. Send a message to carry on.".to_string(),
                theme.text_muted,
            )),
            Some(AssistantOutcomeData::Finished) | None => None,
        };
        if let Some((text, color)) = outcome {
            lines.push(Line::default());
            let style = Style::default().fg(color);
            push_wrapped(&mut lines, "", &text, width, style, style);
        }
    }
    lines
}

/// The daemon's warnings for the waiting changes that can't be undone, in
/// the danger colour. Drawn after the card, right above the keys.
fn warning_lines(state: &AssistantState, width: usize, theme: &Theme) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    if state.overlay_open() {
        return lines;
    }
    let danger = Style::default().fg(theme.error).bold();
    // Several waiting changes with the same warning say it once.
    let mut said = std::collections::HashSet::new();
    for pending in state.pending() {
        if let AssistantPendingData::Approval {
            warning: Some(warning),
            ..
        } = pending
        {
            if !warning.is_empty() && said.insert(warning.as_str()) {
                push_wrapped(&mut lines, "", &one_line(warning), width, danger, danger);
            }
        }
    }
    lines
}

/// What waits for the user: a change to approve, or questions to answer.
fn card_lines(state: &AssistantState, width: usize, theme: &Theme) -> Vec<Line<'static>> {
    let mut lines: Vec<Line<'static>> = Vec::new();
    let muted = Style::default().fg(theme.text_muted);
    // Over the history list the card is only the delete confirm.
    let pending: &[AssistantPendingData] = if state.overlay_open() {
        &[]
    } else {
        state.pending()
    };
    let label = Style::default().fg(theme.accent).bold();
    let strong = Style::default().fg(theme.text_primary).bold();
    let body = Style::default().fg(theme.text_primary);
    for pending in pending {
        lines.push(Line::from(Span::styled("─".repeat(width), muted)));
        match pending {
            AssistantPendingData::Approval {
                call_id,
                tool,
                arguments,
                reason,
                title,
                preview,
                preview_error,
                ..
            } => {
                lines.push(Line::from(Span::styled("PROOF", label)));
                push_wrapped(
                    &mut lines,
                    "",
                    &one_line(
                        title
                            .as_deref()
                            .filter(|title| !title.is_empty())
                            .unwrap_or("Change waiting for you"),
                    ),
                    width,
                    strong,
                    strong,
                );
                if !reason.is_empty() {
                    push_wrapped(&mut lines, "", &one_line(reason), width, muted, muted);
                }
                let messages = preview.as_ref().map(preview_messages).unwrap_or_default();
                let draft = if messages.is_empty() {
                    preview.as_ref().and_then(preview_draft)
                } else {
                    None
                };
                if !messages.is_empty() {
                    lines.push(Line::from(Span::styled(
                        format!(
                            "touches {} message{}:",
                            messages.len(),
                            if messages.len() == 1 { "" } else { "s" }
                        ),
                        muted,
                    )));
                    let expanded = state.full_list_calls.contains(call_id.as_str());
                    let shown = if expanded {
                        messages.len()
                    } else {
                        PREVIEW_ROWS
                    };
                    for message in messages.iter().take(shown) {
                        let row = format!(
                            "{}  {}",
                            one_line(&message.subject),
                            one_line(&message.from)
                        );
                        lines.push(Line::from(vec![
                            Span::styled("• ", muted),
                            Span::styled(
                                truncate(&row, width.saturating_sub(2)),
                                Style::default().fg(theme.text_secondary),
                            ),
                        ]));
                    }
                    if messages.len() > PREVIEW_ROWS {
                        // The key only works from the pane's own focus.
                        let key = state.focus == AssistantFocus::Pane;
                        let text = match (expanded, key) {
                            (true, true) => format!("  {FULL_LIST_KEY} show fewer"),
                            (true, false) => "  show fewer".into(),
                            (false, true) => format!(
                                "  and {} more · {FULL_LIST_KEY} show all",
                                messages.len() - PREVIEW_ROWS
                            ),
                            (false, false) => {
                                format!("  and {} more", messages.len() - PREVIEW_ROWS)
                            }
                        };
                        lines.push(Line::from(Span::styled(text, muted)));
                    }
                } else if let Some(draft) = draft {
                    let to = if draft.to.is_empty() {
                        "(no recipients)".to_string()
                    } else {
                        draft.to
                    };
                    push_wrapped(&mut lines, "To ", &one_line(&to), width, muted, body);
                    let subject = if draft.subject.is_empty() {
                        "(no subject)".to_string()
                    } else {
                        draft.subject
                    };
                    push_wrapped(&mut lines, "", &one_line(&subject), width, strong, strong);
                    let mut rows = wrap(&strip_control_chars(&draft.body), width);
                    let more = rows.len() > PREVIEW_ROWS;
                    rows.truncate(PREVIEW_ROWS);
                    for row in rows {
                        lines.push(Line::from(Span::styled(row, muted)));
                    }
                    if more {
                        lines.push(Line::from(Span::styled("…", muted)));
                    }
                } else if let Some(error) = preview_error {
                    let warn = Style::default().fg(theme.warning);
                    push_wrapped(
                        &mut lines,
                        "",
                        &format!("The preview failed: {}", one_line(error)),
                        width,
                        warn,
                        warn,
                    );
                }
                if state.exact_call_for.as_deref() == Some(call_id.as_str()) {
                    lines.push(Line::from(Span::styled("EXACT CALL", label)));
                    push_json(&mut lines, &format!("{tool} "), arguments, width, muted);
                    // A preview the card couldn't put in words shows as
                    // the daemon returned it, as the web's does.
                    if let Some(preview) = preview.as_ref().filter(|_| messages.is_empty()) {
                        if preview_draft(preview).is_none() {
                            push_json(&mut lines, "preview ", preview, width, muted);
                        }
                    }
                }
                if let Some(AssistantPrompt::RejectReason {
                    call_id: rejecting,
                    text,
                }) = &state.prompt
                {
                    if rejecting == call_id {
                        push_wrapped(
                            &mut lines,
                            "Why, or what to do instead (optional): ",
                            &format!("{text}█"),
                            width,
                            muted,
                            body,
                        );
                    }
                }
            }
            AssistantPendingData::Question {
                call_id, questions, ..
            } => {
                lines.push(Line::from(Span::styled("A QUESTION FOR YOU", label)));
                let current = state.answers_for.as_deref() == Some(call_id.as_str());
                for (index, question) in questions.iter().enumerate() {
                    let answer = state.answers.get(index).filter(|_| current);
                    let active = index == state.active_question;
                    if let Some(header) = &question.header {
                        lines.push(Line::from(Span::styled(
                            one_line(header).to_uppercase(),
                            muted,
                        )));
                    }
                    let marker = if questions.len() > 1 && active {
                        "› "
                    } else {
                        ""
                    };
                    push_wrapped(
                        &mut lines,
                        marker,
                        &one_line(&question.question),
                        width,
                        Style::default().fg(theme.accent),
                        strong,
                    );
                    for (number, option) in question.options.iter().enumerate() {
                        let chosen =
                            answer.is_some_and(|answer| answer.selected.contains(&option.label));
                        let mark = match (question.multi_select, chosen) {
                            (true, true) => "[x]",
                            (true, false) => "[ ]",
                            (false, true) => "(•)",
                            (false, false) => "( )",
                        };
                        let key = if active && number < 9 {
                            format!("{} ", number + 1)
                        } else {
                            "  ".into()
                        };
                        push_wrapped(
                            &mut lines,
                            &format!("{key}{mark} "),
                            &one_line(&option.label),
                            width,
                            muted,
                            if chosen { strong } else { body },
                        );
                        if let Some(description) = &option.description {
                            push_wrapped(
                                &mut lines,
                                "      ",
                                &one_line(description),
                                width,
                                muted,
                                muted,
                            );
                        }
                    }
                    if question.allow_other {
                        let typed = answer.map_or("", |answer| answer.other.as_str());
                        let editing =
                            active && matches!(state.prompt, Some(AssistantPrompt::Other { .. }));
                        if editing {
                            if let Some(AssistantPrompt::Other { text }) = &state.prompt {
                                push_wrapped(
                                    &mut lines,
                                    "Your answer: ",
                                    &format!("{text}█"),
                                    width,
                                    muted,
                                    body,
                                );
                            }
                        } else if !typed.trim().is_empty() {
                            push_wrapped(&mut lines, "Your answer: ", typed, width, muted, strong);
                        } else if active {
                            let hint = if question.options.is_empty() {
                                "o  write your answer"
                            } else {
                                "o  or write your own answer"
                            };
                            lines.push(Line::from(Span::styled(hint, muted)));
                        }
                    }
                }
            }
        }
    }
    if let Some(AssistantPrompt::ConfirmDelete(delete)) = &state.prompt {
        lines.push(Line::from(Span::styled("─".repeat(width), muted)));
        let warn = Style::default().fg(theme.warning);
        push_wrapped(
            &mut lines,
            "",
            &format!(
                "Delete \"{}\" and its {} message{}? This can't be undone.",
                one_line(&delete.title),
                delete.message_count,
                if delete.message_count == 1 { "" } else { "s" }
            ),
            width,
            warn,
            warn,
        );
    }
    lines
}

/// The running line, errors, the message box and where the model runs.
fn footer_lines(
    state: &AssistantState,
    width: usize,
    now: Instant,
    theme: &Theme,
) -> Vec<Line<'static>> {
    let mut lines: Vec<Line<'static>> = Vec::new();
    let muted = Style::default().fg(theme.text_muted);
    if state.overlay_open() {
        // A refused delete or model choice says why there.
        if let Some(error) = &state.error {
            let style = Style::default().fg(theme.error);
            push_wrapped(&mut lines, "", &one_line(error), width, style, style);
        }
        return lines;
    }
    if state.running() {
        let doing = match &state.progress {
            // An older daemon sends no title: its reason is all there is.
            Some(AssistantProgressData::Calling {
                tool,
                reason,
                title: None,
            }) => {
                if reason.is_empty() {
                    tool_words(tool)
                } else {
                    one_line(reason)
                }
            }
            Some(progress) => one_line(&progress.live_line()),
            None => AssistantProgressData::Thinking { step: 1 }.live_line(),
        };
        let elapsed = state
            .started_at
            .map(|started| format!("  {}s", now.saturating_duration_since(started).as_secs()))
            .unwrap_or_default();
        let slow = slow_note(state, now)
            .map(|note| format!(" · {note}"))
            .unwrap_or_default();
        let room = width.saturating_sub(elapsed.width() + slow.width() + 2);
        lines.push(Line::from(vec![
            Span::styled("◌ ", Style::default().fg(theme.accent)),
            Span::styled(
                truncate(&doing, room),
                Style::default().fg(theme.text_secondary),
            ),
            Span::styled(elapsed, muted),
            Span::styled(truncate(&slow, width), Style::default().fg(theme.warning)),
        ]));
    }
    if let Some(error) = &state.error {
        let style = Style::default().fg(theme.error);
        push_wrapped(&mut lines, "", &one_line(error), width, style, style);
    }
    lines.push(Line::from(Span::styled("─".repeat(width), muted)));
    if !state.available() {
        let reason = state
            .availability
            .as_ref()
            .and_then(|availability| availability.reason.clone())
            .unwrap_or_else(|| "The assistant can't run.".into());
        let warn = Style::default().fg(theme.warning);
        push_wrapped(&mut lines, "", &reason, width, warn, warn);
        push_wrapped(
            &mut lines,
            "",
            "Set a model under [llm] in mxr.toml (Edit config in the palette), then reopen this pane.",
            width,
            muted,
            muted,
        );
        return lines;
    }
    let waiting = !state.pending().is_empty();
    let typing = state.focus == AssistantFocus::Input && state.prompt.is_none();
    if state.input.is_empty() && !typing {
        let placeholder = if waiting {
            "› Answer above, or press i to write instead"
        } else {
            "› Ask about your mail, or say what to do"
        };
        lines.push(Line::from(Span::styled(placeholder, muted)));
    } else {
        let text = if typing {
            format!("{}█", state.input)
        } else {
            state.input.clone()
        };
        let mut rows: Vec<Line<'static>> = Vec::new();
        push_wrapped(
            &mut rows,
            "› ",
            &text,
            width,
            Style::default().fg(theme.accent),
            Style::default().fg(theme.text_primary),
        );
        let skip = rows.len().saturating_sub(INPUT_ROWS);
        lines.extend(rows.into_iter().skip(skip));
        if waiting && !state.input.trim().is_empty() {
            lines.push(Line::from(Span::styled(
                "Sending this declines what is waiting above.",
                muted,
            )));
        }
    }
    if let Some(availability) = &state.availability {
        if let Some(model) = &availability.model {
            let place = if availability.leaves_machine {
                "sent off this machine"
            } else {
                "runs on this machine"
            };
            lines.push(Line::from(Span::styled(
                truncate(&format!("{model} · {place}"), width),
                muted,
            )));
        }
    }
    lines
}

/// "<model> is slow on this; <N>s" once the live phrase has lasted long
/// enough to name the model, as the web's `slowNote` does.
fn slow_note(state: &AssistantState, now: Instant) -> Option<String> {
    let model = state.availability.as_ref()?.model.as_deref()?;
    let seconds = now.saturating_duration_since(state.phrase_since?).as_secs();
    (seconds >= SLOW_STEP_SECONDS)
        .then(|| format!("{} is slow on this; {seconds}s", one_line(model)))
}

fn list_lines(
    list: &crate::app::AssistantListState,
    width: usize,
    theme: &Theme,
) -> Vec<Line<'static>> {
    let muted = Style::default().fg(theme.text_muted);
    let mut lines = vec![
        Line::from(Span::styled(
            "Conversations",
            Style::default().fg(theme.accent).bold(),
        )),
        Line::default(),
    ];
    if list.loading {
        lines.push(Line::from(Span::styled("Loading…", muted)));
        return lines;
    }
    if list.conversations.is_empty() {
        lines.push(Line::from(Span::styled("None yet.", muted)));
        return lines;
    }
    let now = chrono::Utc::now().timestamp();
    for (index, summary) in list.conversations.iter().enumerate() {
        let selected = index == list.selected;
        let style = if selected {
            Style::default()
                .fg(theme.selection_fg)
                .bg(theme.selection_bg)
        } else {
            Style::default().fg(theme.text_primary)
        };
        let title = truncate(&one_line(&summary.title), width.saturating_sub(2));
        lines.push(Line::from(Span::styled(
            format!("{}{title}", if selected { "› " } else { "  " }),
            style,
        )));
        let mut meta = age(now - summary.updated_at);
        if summary.pending_count > 0 {
            meta.push_str(" · waiting for you");
        }
        if summary.status == AssistantStatusData::Running {
            meta.push_str(" · working");
        }
        lines.push(Line::from(Span::styled(format!("  {meta}"), muted)));
    }
    lines
}

/// The model picker: each choice with where it runs, the one in use marked,
/// and for one that can't be picked, what to do about it.
fn models_lines(
    models: &crate::app::AssistantModelsState,
    width: usize,
    theme: &Theme,
) -> (Vec<Line<'static>>, std::ops::Range<usize>) {
    let muted = Style::default().fg(theme.text_muted);
    let mut lines = vec![
        Line::from(Span::styled(
            "Model",
            Style::default().fg(theme.accent).bold(),
        )),
        Line::default(),
    ];
    let Some(data) = &models.models else {
        lines.push(Line::from(Span::styled(
            if models.loading {
                "Loading…"
            } else {
                "No models listed."
            },
            muted,
        )));
        return (lines, 0..0);
    };
    let mut selected_rows = 0..0;
    for (index, choice) in data.choices.iter().enumerate() {
        let selected = index == models.selected;
        let first_row = lines.len();
        let current = data.current.as_deref() == Some(choice.id.as_str());
        let style = if selected {
            Style::default()
                .fg(theme.selection_fg)
                .bg(theme.selection_bg)
        } else if choice.available {
            Style::default().fg(theme.text_primary)
        } else {
            muted
        };
        let place = if choice.cloud {
            "cloud"
        } else {
            "on this machine"
        };
        let row = format!(
            "{}{} {}  {place}",
            if selected { "› " } else { "  " },
            if current { "✓" } else { " " },
            one_line(&choice.label),
        );
        lines.push(Line::from(Span::styled(truncate(&row, width), style)));
        if let Some(reason) = &choice.unavailable_reason {
            push_wrapped(&mut lines, "      ", &one_line(reason), width, muted, muted);
        }
        if selected {
            selected_rows = first_row..lines.len();
        }
    }
    lines.push(Line::default());
    push_wrapped(
        &mut lines,
        "",
        "A cloud model receives your messages and the mail the assistant reads.",
        width,
        muted,
        muted,
    );
    (lines, selected_rows)
}

fn age(seconds: i64) -> String {
    match seconds.max(0) {
        s if s < 60 => "just now".into(),
        s if s < 3600 => format!("{}m ago", s / 60),
        s if s < 86_400 => format!("{}h ago", s / 3600),
        s => format!("{}d ago", s / 86_400),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{AnswerDraft, AssistantState};
    use mxr_protocol::{
        AssistantAvailabilityData, AssistantConversationData, AssistantQuestionData,
        AssistantQuestionOptionData,
    };
    use mxr_test_support::render_to_string;
    use serde_json::json;

    fn conversation(
        status: AssistantStatusData,
        items: Vec<AssistantItemData>,
        pending: Vec<AssistantPendingData>,
    ) -> AssistantConversationData {
        AssistantConversationData {
            last_error_detail: None,
            id: "c1".into(),
            title: "Tidy newsletters".into(),
            status,
            last_outcome: None,
            last_error: None,
            created_at: 0,
            updated_at: 0,
            auto_approve: false,
            items,
            pending,
        }
    }

    fn user(seq: i64, text: &str) -> AssistantItemData {
        AssistantItemData::User {
            seq,
            text: text.into(),
            created_at: 0,
        }
    }

    fn step(seq: i64, reason: &str, state: AssistantCallStateData) -> AssistantItemData {
        AssistantItemData::ToolCall {
            title: None,
            seq,
            call_id: format!("call{seq}"),
            tool: "mxr_search".into(),
            reason: reason.into(),
            arguments: json!({}),
            state,
            result: None,
            readout: None,
            created_at: 0,
        }
    }

    fn open_state() -> AssistantState {
        let mut state = AssistantState::default();
        state.open = true;
        state.focus = AssistantFocus::Input;
        state.availability = Some(AssistantAvailabilityData {
            available: true,
            model: Some("qwen3:8b".into()),
            leaves_machine: false,
            reason: None,
        });
        state
    }

    fn render(state: &AssistantState) -> String {
        render_to_string(48, 24, |frame| {
            draw(
                frame,
                Rect::new(0, 0, 48, 24),
                state,
                state.started_at.unwrap_or_else(Instant::now),
                &Theme::default(),
            );
        })
    }

    #[test]
    fn assistant_pane_empty() {
        insta::assert_snapshot!("assistant_pane_empty", render(&open_state()));
    }

    #[test]
    fn assistant_pane_running_with_steps() {
        let mut state = open_state();
        state.conversation_id = Some("c1".into());
        state.conversation = Some(conversation(
            AssistantStatusData::Running,
            vec![
                user(1, "Find newsletters I never open"),
                step(
                    2,
                    "Search newsletters from the last month",
                    AssistantCallStateData::Done,
                ),
                step(3, "Count opens per sender", AssistantCallStateData::Running),
            ],
            Vec::new(),
        ));
        state.progress = Some(AssistantProgressData::Calling {
            title: None,
            tool: "mxr_search".into(),
            reason: "Count opens per sender".into(),
        });
        state.focus = AssistantFocus::Pane;
        insta::assert_snapshot!("assistant_pane_running", render(&state));
    }

    #[test]
    fn assistant_pane_approval_waiting() {
        let mut state = open_state();
        state.focus = AssistantFocus::Pane;
        state.conversation_id = Some("c1".into());
        let mut waiting = conversation(
            AssistantStatusData::AwaitingUser,
            vec![
                user(1, "Archive the Acme receipts"),
                step(2, "Find Acme receipts", AssistantCallStateData::Done),
                step(
                    3,
                    "Archive the three receipts",
                    AssistantCallStateData::Waiting,
                ),
            ],
            vec![AssistantPendingData::Approval {
                title: Some("Archive 3 emails".into()),
                warning: None,
                call_id: "call3".into(),
                tool: "mxr_mutate".into(),
                arguments: json!({"action": "archive", "message_ids": ["a", "b", "c"]}),
                reason: "They are receipts you already filed".into(),
                preview: Some(json!({"messages": [
                    {"subject": "Your receipt #1", "from": {"name": "Acme"}},
                    {"subject": "Your receipt #2", "from": {"name": "Acme"}},
                    {"subject": "Your receipt #3", "from": {"email": "billing@acme.test"}}
                ]})),
                preview_error: None,
            }],
        );
        waiting.auto_approve = true;
        state.conversation = Some(waiting);
        insta::assert_snapshot!("assistant_pane_approval", render(&state));
    }

    #[test]
    fn assistant_pane_approval_irreversible() {
        let mut state = open_state();
        state.focus = AssistantFocus::Pane;
        state.conversation_id = Some("c1".into());
        state.conversation = Some(conversation(
            AssistantStatusData::AwaitingUser,
            vec![user(1, "Send the reply to Ann")],
            vec![AssistantPendingData::Approval {
                title: Some("Send a reply to Ann".into()),
                warning: Some("Sends an email. Can't be undone.".into()),
                call_id: "call4".into(),
                tool: "mxr_send_draft".into(),
                arguments: json!({"draft_id": "d1", "confirm": true}),
                reason: "You asked me to reply".into(),
                preview: Some(json!({"draft": {
                    "to": [{"email": "ann@example.test"}],
                    "subject": "Re: Lunch",
                    "body": "Thursday works."
                }})),
                preview_error: None,
            }],
        ));
        let screen = render(&state);
        assert!(
            screen.contains("Sends an email. Can't be undone."),
            "{screen}"
        );
        insta::assert_snapshot!("assistant_pane_approval_irreversible", screen);
    }

    #[test]
    fn a_long_preview_never_pushes_the_warning_off_a_short_pane() {
        let mut state = open_state();
        state.focus = AssistantFocus::Pane;
        state.conversation_id = Some("c1".into());
        state.conversation = Some(conversation(
            AssistantStatusData::AwaitingUser,
            vec![user(1, "Send the reply to Ann")],
            vec![AssistantPendingData::Approval {
                title: Some("Send a reply to Ann".into()),
                warning: Some("Sends an email. Can't be undone.".into()),
                call_id: "call6".into(),
                tool: "mxr_send_draft".into(),
                arguments: json!({"draft_id": "d1", "confirm": true}),
                reason: "You asked me to reply".into(),
                preview: Some(json!({"draft": {
                    "to": [{"email": "ann@example.test"}],
                    "subject": "Re: Lunch",
                    "body": "One.\nTwo.\nThree.\nFour.\nFive.\nSix.\nSeven.\nEight."
                }})),
                preview_error: None,
            }],
        ));
        let screen = render(&state);
        assert!(screen.contains('…'), "the card was cut: {screen}");
        assert!(
            screen.contains("Sends an email. Can't be undone."),
            "{screen}"
        );
    }

    #[test]
    fn the_warning_outlasts_a_footer_that_fills_the_bottom() {
        let line = |text: &str| Line::from(text.to_string());
        let kept = bottom_block(
            vec![line("YOUR CALL"), line("Send a reply")],
            vec![line("Sends an email. Can't be undone.")],
            (0..12).map(|n| line(&format!("footer {n}"))).collect(),
            8,
        );
        let text: Vec<String> = kept.iter().map(ToString::to_string).collect();
        assert_eq!(text.len(), 8, "{text:?}");
        assert_eq!(text[0], "Sends an email. Can't be undone.");
        assert_eq!(text[7], "footer 11", "the keys stay last: {text:?}");
        let tight = bottom_block(
            vec![],
            vec![line("w1"), line("w2")],
            vec![line("input"), line("keys")],
            2,
        );
        let tight: Vec<String> = tight.iter().map(ToString::to_string).collect();
        assert_eq!(tight, ["w1", "keys"]);
        let roomy = bottom_block(vec![line("a")], vec![line("w")], vec![line("f")], 5);
        let roomy: Vec<String> = roomy.iter().map(ToString::to_string).collect();
        assert_eq!(roomy, ["a", "w", "f"]);
    }

    #[test]
    fn a_warning_is_drawn_in_the_danger_colour_above_the_keys() {
        let theme = Theme::default();
        let mut state = open_state();
        state.focus = AssistantFocus::Pane;
        state.conversation_id = Some("c1".into());
        state.conversation = Some(conversation(
            AssistantStatusData::AwaitingUser,
            vec![user(1, "bin them")],
            vec![AssistantPendingData::Approval {
                title: Some("Move 2 emails to Trash".into()),
                warning: Some("Can't be undone.".into()),
                call_id: "call5".into(),
                tool: "mxr_mutate".into(),
                arguments: json!({"action": "trash", "message_ids": ["a", "b"]}),
                reason: "Junk".into(),
                preview: None,
                preview_error: None,
            }],
        ));
        let lines = warning_lines(&state, 40, &theme);
        let warning = lines
            .iter()
            .find(|line| line.to_string().contains("Can't be undone."))
            .expect("the warning is on the card");
        assert_eq!(warning.spans[0].style.fg, Some(theme.error));
    }

    #[test]
    fn assistant_pane_approval_exact_call() {
        let mut state = open_state();
        state.focus = AssistantFocus::Pane;
        state.conversation_id = Some("c1".into());
        state.conversation = Some(conversation(
            AssistantStatusData::AwaitingUser,
            vec![user(1, "Make a rule for Acme receipts")],
            vec![AssistantPendingData::Approval {
                title: Some("Make a rule for Acme receipts".into()),
                warning: None,
                call_id: "call2".into(),
                tool: "mxr_rule".into(),
                arguments: json!({"name": "Acme receipts", "query": "from:acme.test"}),
                reason: "So they file themselves".into(),
                preview: Some(json!({"matches": 3})),
                preview_error: None,
            }],
        ));
        state.exact_call_for = Some("call2".into());
        insta::assert_snapshot!("assistant_pane_approval_exact_call", render(&state));
    }

    #[test]
    fn assistant_pane_history_delete_confirm() {
        let mut state = open_state();
        state.focus = AssistantFocus::Pane;
        state.list = Some(crate::app::AssistantListState {
            loading: false,
            conversations: vec![mxr_protocol::AssistantConversationSummaryData {
                id: "c2".into(),
                title: "Old receipts".into(),
                status: AssistantStatusData::Idle,
                last_outcome: None,
                created_at: 0,
                updated_at: chrono::Utc::now().timestamp() - 2 * 3600,
                message_count: 4,
                pending_count: 0,
            }],
            selected: 0,
        });
        state.prompt = Some(AssistantPrompt::ConfirmDelete(
            mxr_protocol::AssistantDeleteData {
                conversation_id: "c2".into(),
                title: "Old receipts".into(),
                message_count: 4,
                dry_run: true,
                deleted: false,
            },
        ));
        insta::assert_snapshot!("assistant_pane_history_delete_confirm", render(&state));
    }

    #[test]
    fn assistant_pane_model_picker() {
        let choice = |id: &str, label: &str, cloud: bool, reason: Option<&str>| {
            mxr_protocol::AssistantModelChoiceData {
                id: id.into(),
                label: label.into(),
                cloud,
                available: reason.is_none(),
                unavailable_reason: reason.map(str::to_string),
            }
        };
        let mut state = open_state();
        state.focus = AssistantFocus::Pane;
        state.models = Some(crate::app::AssistantModelsState {
            loading: false,
            selected: 1,
            models: Some(mxr_protocol::AssistantModelsData {
                current: Some("gemma4:latest".into()),
                choices: vec![
                    choice("gemma4:latest", "Gemma 4", false, None),
                    choice(
                        "gpt-6-luna",
                        "GPT-6 Luna",
                        true,
                        Some("Add an OpenAI key: mxr llm key set openai"),
                    ),
                    choice("claude-sonnet-5-5", "Claude Sonnet 5.5", true, None),
                ],
            }),
        });
        insta::assert_snapshot!("assistant_pane_model_picker", render(&state));
    }

    #[test]
    fn assistant_pane_question_waiting() {
        let mut state = open_state();
        state.focus = AssistantFocus::Pane;
        state.conversation_id = Some("c1".into());
        state.conversation = Some(conversation(
            AssistantStatusData::AwaitingUser,
            vec![user(1, "Tidy my newsletters")],
            vec![AssistantPendingData::Question {
                call_id: "q1".into(),
                reason: "Which ones count".into(),
                questions: vec![AssistantQuestionData {
                    question: "Which newsletters should go?".into(),
                    header: Some("Scope".into()),
                    options: vec![
                        AssistantQuestionOptionData {
                            label: "Unopened for 3 months".into(),
                            description: Some("12 senders".into()),
                        },
                        AssistantQuestionOptionData {
                            label: "All of them".into(),
                            description: None,
                        },
                    ],
                    multi_select: false,
                    allow_other: true,
                }],
            }],
        ));
        state.answers_for = Some("q1".into());
        state.answers = vec![AnswerDraft {
            selected: vec!["Unopened for 3 months".into()],
            other: String::new(),
        }];
        insta::assert_snapshot!("assistant_pane_question", render(&state));
    }

    #[test]
    fn unavailable_shows_the_reason_instead_of_the_box() {
        let mut state = open_state();
        state.availability = Some(AssistantAvailabilityData {
            available: false,
            model: None,
            leaves_machine: false,
            reason: Some("No model is set up for the assistant.".into()),
        });
        let screen = render(&state);
        assert!(screen.contains("No model is set up"), "{screen}");
        assert!(!screen.contains("Ask about your mail"), "{screen}");
    }

    #[test]
    fn settled_steps_fold_and_expand() {
        let mut state = open_state();
        state.conversation_id = Some("c1".into());
        state.conversation = Some(conversation(
            AssistantStatusData::Idle,
            vec![
                user(1, "hi"),
                step(2, "Look around", AssistantCallStateData::Done),
                step(3, "Look again", AssistantCallStateData::Done),
            ],
            Vec::new(),
        ));
        let folded = render(&state);
        assert!(folded.contains("▸ 2 steps"), "{folded}");
        state.steps_expanded = true;
        let open = render(&state);
        assert!(open.contains("✓ Look again"), "{open}");
    }

    #[test]
    fn a_reply_after_failed_steps_names_them() {
        let mut state = open_state();
        state.conversation_id = Some("c1".into());
        let reply = |seq, failed: &str| AssistantItemData::Assistant {
            rows: Vec::new(),
            seq,
            text: "Starred it.".into(),
            model: None,
            leaves_machine: None,
            failed_call_ids: vec![failed.to_string()],
            created_at: 0,
        };
        state.conversation = Some(conversation(
            AssistantStatusData::Idle,
            vec![
                user(1, "star the Q1 email"),
                step(2, "Find it", AssistantCallStateData::Failed),
                reply(3, "call2"),
                user(4, "again"),
                step(5, "Find it", AssistantCallStateData::Done),
                step(6, "Star the Q1 email", AssistantCallStateData::Failed),
                reply(7, "call6"),
            ],
            Vec::new(),
        ));
        let screen = render(&state);
        assert_eq!(
            screen
                .matches("This didn't happen, whatever the reply")
                .count(),
            2,
            "{screen}"
        );
        assert!(screen.contains("✕ Star the Q1 email"), "{screen}");
        assert_eq!(
            screen.matches("✕ ").count(),
            2,
            "only the steps the replies name: {screen}"
        );
    }

    fn titled(
        seq: i64,
        state: AssistantCallStateData,
        running: &str,
        done: &str,
    ) -> AssistantItemData {
        let mut item = step(seq, "The model's odd reason", state);
        if let AssistantItemData::ToolCall { title, .. } = &mut item {
            *title = Some(AssistantStepTitleData {
                running: running.into(),
                done: done.into(),
            });
        }
        item
    }

    #[test]
    fn steps_read_as_the_daemons_wire_lines_not_the_models_reason() {
        let mut state = open_state();
        state.conversation_id = Some("c1".into());
        state.conversation = Some(conversation(
            AssistantStatusData::Running,
            vec![
                user(1, "what shipped"),
                titled(
                    2,
                    AssistantCallStateData::Done,
                    "Searching your mail for \"shipped\"",
                    "Searched your mail: 14 found",
                ),
                titled(
                    3,
                    AssistantCallStateData::Running,
                    "Reading a conversation",
                    "Read a conversation",
                ),
                titled(
                    4,
                    AssistantCallStateData::Failed,
                    "Starring 1 email",
                    "Couldn't star 1 email: it needs your confirmation first",
                ),
                titled(
                    5,
                    AssistantCallStateData::Waiting,
                    "Archiving 14 emails",
                    "Waiting for you: archive 14 emails",
                ),
            ],
            Vec::new(),
        ));
        state.progress = Some(AssistantProgressData::Calling {
            tool: "mxr_read_thread".into(),
            reason: "The model's odd reason".into(),
            title: Some("Reading a conversation".into()),
        });
        let screen = render(&state);
        for line in [
            "✓ Searched your mail: 14 found",
            "◌ Reading a conversation",
            "✕ Couldn't star 1 email: it needs",
            "◇ Waiting for you: archive 14",
        ] {
            assert!(screen.contains(line), "{line}\n{screen}");
        }
        assert!(!screen.contains("odd reason"), "{screen}");
        assert!(
            !screen.contains("Thinking") && !screen.contains("Working"),
            "{screen}"
        );
    }

    #[test]
    fn the_live_line_says_what_the_turn_is_doing() {
        let mut state = open_state();
        state.conversation_id = Some("c1".into());
        state.conversation = Some(conversation(
            AssistantStatusData::Running,
            vec![user(1, "hi")],
            Vec::new(),
        ));
        for (progress, line) in [
            (None, "Reading your request"),
            (
                Some(AssistantProgressData::Thinking { step: 3 }),
                "Deciding what to do next",
            ),
            (
                Some(AssistantProgressData::Writing {
                    text: String::new(),
                }),
                "Writing the answer",
            ),
            (
                Some(AssistantProgressData::Summarizing),
                "Making room: summarising earlier",
            ),
        ] {
            state.progress = progress;
            let screen = render(&state);
            assert!(screen.contains(line), "{line}\n{screen}");
        }
    }

    #[test]
    fn an_approval_uses_the_daemons_title() {
        let mut state = open_state();
        state.focus = AssistantFocus::Pane;
        state.conversation_id = Some("c1".into());
        state.conversation = Some(conversation(
            AssistantStatusData::AwaitingUser,
            vec![user(1, "archive them")],
            vec![AssistantPendingData::Approval {
                title: Some("Archive 14 emails".into()),
                warning: None,
                call_id: "call3".into(),
                tool: "mxr_mutate".into(),
                arguments: json!({"action": "archive", "message_ids": ["a"]}),
                reason: "Old shipping notices".into(),
                preview: None,
                preview_error: None,
            }],
        ));
        let screen = render(&state);
        assert!(screen.contains("Archive 14 emails"), "{screen}");
        assert!(!screen.contains("Archive 1 message"), "{screen}");
    }

    #[test]
    fn a_reply_lists_the_rows_the_daemon_attached() {
        let mut state = open_state();
        state.conversation_id = Some("c1".into());
        state.conversation = Some(conversation(
            AssistantStatusData::Idle,
            vec![
                user(1, "who wrote"),
                AssistantItemData::Assistant {
                    seq: 2,
                    text: "These two wrote.".into(),
                    model: None,
                    leaves_machine: None,
                    failed_call_ids: Vec::new(),
                    rows: vec![
                        mxr_protocol::AssistantReadoutRowData {
                            title: "Weekly sync".into(),
                            detail: Some("Quinn".into()),
                            link: None,
                            ..Default::default()
                        },
                        mxr_protocol::AssistantReadoutRowData {
                            title: "Invoice".into(),
                            detail: None,
                            link: None,
                            ..Default::default()
                        },
                    ],
                    created_at: 0,
                },
            ],
            Vec::new(),
        ));
        let screen = render(&state);
        assert!(screen.contains("• Weekly sync  Quinn"), "{screen}");
        assert!(screen.contains("• Invoice"), "{screen}");
    }

    #[test]
    fn daemon_text_with_escape_sequences_draws_as_plain_text() {
        let hostile = "\u{1b}[2J\u{1b}[Hforged\u{202e}gnp\u{9b}";
        let mut state = open_state();
        state.focus = AssistantFocus::Pane;
        state.conversation_id = Some("c1".into());
        state.conversation = Some(conversation(
            AssistantStatusData::AwaitingUser,
            vec![
                user(1, "who wrote"),
                titled(2, AssistantCallStateData::Done, hostile, hostile),
                AssistantItemData::Assistant {
                    seq: 3,
                    text: "Two wrote.".into(),
                    model: None,
                    leaves_machine: None,
                    failed_call_ids: Vec::new(),
                    rows: vec![mxr_protocol::AssistantReadoutRowData {
                        title: hostile.into(),
                        detail: Some(hostile.into()),
                        link: None,
                        ..Default::default()
                    }],
                    created_at: 0,
                },
            ],
            vec![AssistantPendingData::Approval {
                title: Some(hostile.into()),
                warning: Some(hostile.into()),
                call_id: "c".into(),
                tool: "mxr_mutate".into(),
                arguments: json!({}),
                reason: hostile.into(),
                preview: None,
                preview_error: None,
            }],
        ));
        state.progress = Some(AssistantProgressData::Calling {
            tool: "mxr_search".into(),
            reason: String::new(),
            title: Some(hostile.into()),
        });
        let screen = render(&state);
        assert!(screen.contains("forged"), "{screen}");
        assert!(
            !screen.chars().any(|c| c.is_control() && c != '\n') && !screen.contains('\u{202e}'),
            "{screen:?}"
        );
    }

    #[test]
    fn a_reworded_failure_shows_as_the_sentence_alone() {
        let mut state = open_state();
        state.conversation_id = Some("c1".into());
        let mut failed = conversation(AssistantStatusData::Idle, vec![user(1, "hi")], Vec::new());
        failed.last_outcome = Some(AssistantOutcomeData::Failed);
        failed.last_error = Some("The model stopped answering. Send your message again.".into());
        failed.last_error_detail = Some("LLM returned an empty completion".into());
        state.conversation = Some(failed);
        let screen = render(&state);
        assert!(screen.contains("The model stopped answering."), "{screen}");
        assert!(!screen.contains("empty completion"), "{screen}");
        assert!(!screen.contains("That turn failed"), "{screen}");
    }

    #[test]
    fn wrap_keeps_breaks_and_splits_long_words() {
        assert_eq!(wrap("one two three", 7), vec!["one two", "three"]);
        assert_eq!(wrap("a\nb", 10), vec!["a", "b"]);
        assert_eq!(wrap("abcdefgh", 3), vec!["abc", "def", "gh"]);
    }

    fn approval_of(call_id: &str, messages: usize, warning: Option<&str>) -> AssistantPendingData {
        let messages: Vec<_> = (1..=messages)
            .map(|n| json!({"subject": format!("Receipt {n}"), "from": {"name": "Acme"}}))
            .collect();
        AssistantPendingData::Approval {
            title: Some("Archive the receipts".into()),
            warning: warning.map(Into::into),
            call_id: call_id.into(),
            tool: "mxr_mutate".into(),
            arguments: json!({"action": "archive"}),
            reason: "They are receipts you already filed".into(),
            preview: Some(json!({ "messages": messages })),
            preview_error: None,
        }
    }

    fn waiting_on(pending: Vec<AssistantPendingData>) -> AssistantState {
        let mut state = open_state();
        state.focus = AssistantFocus::Pane;
        state.conversation_id = Some("c1".into());
        state.conversation = Some(conversation(
            AssistantStatusData::AwaitingUser,
            vec![user(1, "Archive the receipts")],
            pending,
        ));
        state
    }

    fn render_tall(state: &AssistantState, height: u16) -> String {
        render_to_string(48, height, |frame| {
            draw(
                frame,
                Rect::new(0, 0, 48, height),
                state,
                Instant::now(),
                &Theme::default(),
            );
        })
    }

    #[test]
    fn a_long_approval_folds_to_six_with_the_key_to_show_all() {
        let state = waiting_on(vec![approval_of("call9", 9, None)]);
        let screen = render(&state);
        assert!(screen.contains("Receipt 6"), "{screen}");
        assert!(!screen.contains("Receipt 7"), "{screen}");
        assert!(screen.contains("and 3 more · a show all"), "{screen}");
        assert!(
            screen.contains("a show all"),
            "the hint names the key: {screen}"
        );
    }

    #[test]
    fn the_key_names_show_all_only_when_the_pane_has_the_keys() {
        let mut state = waiting_on(vec![approval_of("call9", 9, None)]);
        state.focus = AssistantFocus::Input;
        let screen = render(&state);
        assert!(screen.contains("and 3 more"), "{screen}");
        assert!(!screen.contains("show all"), "{screen}");
    }

    #[test]
    fn an_expanded_approval_lists_every_message() {
        let mut state = waiting_on(vec![approval_of("call9", 9, None)]);
        state.full_list_calls.insert("call9".into());
        let screen = render_tall(&state, 40);
        assert!(screen.contains("Receipt 9"), "{screen}");
        assert!(screen.contains("a show fewer"), "{screen}");
        assert!(!screen.contains("and 3 more"), "{screen}");
        insta::assert_snapshot!("assistant_pane_approval_expanded", screen);
    }

    #[test]
    fn the_warning_stays_visible_while_the_list_is_expanded_on_a_short_pane() {
        let mut state = waiting_on(vec![approval_of(
            "call9",
            9,
            Some("Sends an email. Can't be undone."),
        )]);
        state.full_list_calls.insert("call9".into());
        let screen = render_tall(&state, 16);
        assert!(
            screen.contains("Sends an email. Can't be undone."),
            "{screen}"
        );
        // The hint wraps on this width, so only its last word is on one row.
        assert!(screen.contains("fewer"), "{screen}");
    }

    #[test]
    fn a_new_approval_starts_folded() {
        let mut state = waiting_on(vec![approval_of("call9", 9, None)]);
        state.full_list_calls.insert("call9".into());
        state.conversation.as_mut().unwrap().pending = vec![approval_of("call10", 9, None)];
        let screen = render(&state);
        assert!(!screen.contains("Receipt 7"), "{screen}");
        assert!(screen.contains("and 3 more · a show all"), "{screen}");
    }

    #[test]
    fn a_running_line_names_a_slow_model_from_twenty_seconds() {
        let mut state = open_state();
        state.focus = AssistantFocus::Pane;
        state.conversation_id = Some("c1".into());
        state.conversation = Some(conversation(
            AssistantStatusData::Running,
            vec![user(1, "Tidy my newsletters")],
            vec![],
        ));
        let now = Instant::now();
        state.started_at = Some(now);
        state.progress = Some(AssistantProgressData::Thinking { step: 1 });
        state.phrase_since = Some(now - std::time::Duration::from_secs(19));
        let before = render_at(&state, now);
        assert!(!before.contains("is slow on this"), "{before}");

        state.phrase_since = Some(now - std::time::Duration::from_secs(20));
        let after = render_at(&state, now);
        assert!(after.contains("qwen3:8b is slow on this; 20s"), "{after}");
    }

    fn render_at(state: &AssistantState, now: Instant) -> String {
        render_to_string(60, 24, |frame| {
            draw(
                frame,
                Rect::new(0, 0, 60, 24),
                state,
                now,
                &Theme::default(),
            );
        })
    }
}
