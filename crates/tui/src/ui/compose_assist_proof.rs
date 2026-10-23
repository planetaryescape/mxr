use mxr_protocol::{ComposeProposalData, TextChangeKindData};
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};

pub fn draw(
    frame: &mut Frame,
    area: Rect,
    proposal: &ComposeProposalData,
    note_input: Option<&str>,
    theme: &crate::theme::Theme,
) {
    let popup = super::centered_rect(84, 72, area);
    frame.render_widget(Clear, popup);
    let block = Block::default()
        .title(" Writing proof · y accept · e accept/edit · n revise · Esc keep mine ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.accent))
        .style(Style::default().bg(theme.modal_bg));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let no_color = std::env::var_os("NO_COLOR").is_some();
    let mut text = if no_color {
        vec![Line::from(no_color_diff(proposal))]
    } else {
        let lines = proposal
            .changes
            .iter()
            .map(|change| {
                let style = match change.kind {
                    TextChangeKindData::Kept => Style::default().fg(theme.text_primary),
                    TextChangeKindData::Added => Style::default().fg(theme.accent),
                    TextChangeKindData::Removed => Style::default()
                        .fg(theme.error)
                        .add_modifier(Modifier::CROSSED_OUT),
                };
                Span::styled(change.text.clone(), style)
            })
            .collect::<Vec<_>>();
        vec![Line::from(lines)]
    };
    if !proposal.gaps.is_empty() {
        text.push(Line::from(Span::styled(
            format!(
                "Warning: {} detail(s) need your input; this draft cannot be sent.",
                proposal.gaps.len()
            ),
            Style::default().fg(theme.warning),
        )));
    }
    if let Some(input) = note_input {
        text.push(Line::from(format!(
            "Revise: {input}  (Enter submit · Esc cancel)"
        )));
    }
    text.push(Line::from(Span::styled(
        proposal.provenance.summary_line(),
        Style::default().fg(theme.text_muted),
    )));
    frame.render_widget(Paragraph::new(text).wrap(Wrap { trim: false }), inner);
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
    use mxr_protocol::{AiLocalityData, AssistActionData, DraftProvenanceData, TextChangeData};

    fn proposal() -> ComposeProposalData {
        ComposeProposalData {
            proposal_id: "p".into(),
            action: AssistActionData::Polish,
            base: "b".into(),
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
        }
    }

    #[test]
    fn plain_text_proof_marks_removed_and_added_words() {
        assert_eq!(no_color_diff(&proposal()), "[-Hi-]{+Hello+} there");
    }
}
