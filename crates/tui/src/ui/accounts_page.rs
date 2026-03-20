use crate::app::{AccountFormMode, AccountsPageState};
use ratatui::prelude::*;
use ratatui::widgets::*;

pub fn draw(frame: &mut Frame, area: Rect, state: &AccountsPageState, theme: &crate::theme::Theme) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(36), Constraint::Percentage(64)])
        .split(area);

    let items = state
        .accounts
        .iter()
        .map(|account| {
            let default = if account.is_default { " *" } else { "" };
            let sync = account.sync_kind.as_deref().unwrap_or("none");
            let send = account.send_kind.as_deref().unwrap_or("none");
            let source = match account.source {
                mxr_protocol::AccountSourceData::Runtime => "runtime",
                mxr_protocol::AccountSourceData::Config => "config",
                mxr_protocol::AccountSourceData::Both => "both",
            };
            ListItem::new(format!(
                "{}{} [{} / {}] {{{source}}}",
                account.name, default, sync, send
            ))
        })
        .collect::<Vec<_>>();
    let list = List::new(items).block(
        Block::default()
            .title(" Accounts ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.accent)),
    );
    let mut list_state = ListState::default().with_selected(Some(state.selected_index));
    frame.render_stateful_widget(list, chunks[0], &mut list_state);

    if state.form.visible {
        draw_form(frame, chunks[1], &state.form, theme);
        return;
    }

    let mut detail_lines = if let Some(account) = state.accounts.get(state.selected_index) {
        let editability = match account.editable {
            mxr_protocol::AccountEditModeData::Full => "full",
            mxr_protocol::AccountEditModeData::RuntimeOnly => "runtime-only",
        };
        let source = match account.source {
            mxr_protocol::AccountSourceData::Runtime => "runtime",
            mxr_protocol::AccountSourceData::Config => "config",
            mxr_protocol::AccountSourceData::Both => "runtime + config",
        };
        vec![
            Line::from(format!(
                "Key: {}",
                account.key.as_deref().unwrap_or("(runtime-only)")
            )),
            Line::from(format!("Name: {}", account.name)),
            Line::from(format!("Email: {}", account.email)),
            Line::from(format!("Provider: {}", account.provider_kind)),
            Line::from(format!("Sync: {}", account.sync_kind.as_deref().unwrap_or("none"))),
            Line::from(format!("Send: {}", account.send_kind.as_deref().unwrap_or("none"))),
            Line::from(format!("Source: {source}")),
            Line::from(format!("Editable: {editability}")),
            Line::from(format!("Default: {}", if account.is_default { "yes" } else { "no" })),
        ]
    } else {
        let mut lines = vec![
            Line::from("No accounts configured"),
            Line::from(""),
            Line::from("Press n to add a Gmail, IMAP/SMTP, or SMTP account"),
        ];
        if let Some(status) = &state.status {
            lines.push(Line::from(""));
            lines.push(Line::from(status.clone()));
        }
        lines
    };

    let result_lines = format_account_result_lines(state.last_result.as_ref());
    if !result_lines.is_empty() {
        detail_lines.push(Line::from(""));
        detail_lines.extend(result_lines);
    }

    if let Some(status) = &state.status {
        detail_lines.push(Line::from(""));
        detail_lines.push(Line::from(status.clone()));
    } else if !state.accounts.is_empty() {
        detail_lines.push(Line::from(""));
        detail_lines.push(Line::from("n:new  Enter:edit  t:test  d:set default"));
    }

    let paragraph = Paragraph::new(detail_lines)
        .block(
            Block::default()
                .title(" Details ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.warning)),
        )
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, chunks[1]);

    if state.onboarding_modal_open {
        draw_onboarding_modal(frame, area, theme);
    }
}

fn draw_form(frame: &mut Frame, area: Rect, form: &crate::app::AccountFormState, theme: &crate::theme::Theme) {
    let titles = ["Gmail", "IMAP + SMTP", "SMTP only"]
        .into_iter()
        .map(Line::from)
        .collect::<Vec<_>>();
    let selected = match form.mode {
        AccountFormMode::Gmail => 0,
        AccountFormMode::ImapSmtp => 1,
        AccountFormMode::SmtpOnly => 2,
    };

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0), Constraint::Length(2)])
        .split(area);

    let tabs = Tabs::new(titles)
        .select(selected)
        .block(
            Block::default()
                .title(" Account Mode ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.accent)),
        )
        .highlight_style(Style::default().fg(theme.warning).bold());
    frame.render_widget(tabs, layout[0]);

    let fields = build_fields(form);
    let mut body_lines = fields
        .iter()
        .enumerate()
        .map(|(index, (label, value, editable))| {
            let mut style = if index == form.active_field {
                Style::default().fg(theme.warning).bold()
            } else {
                Style::default().fg(theme.text_primary)
            };
            if !editable {
                style = style.fg(theme.text_muted);
            }
            let value = if form.editing_field && index == form.active_field && *editable {
                render_with_cursor(value, form.field_cursor)
            } else {
                value.clone()
            };
            Line::from(vec![
                Span::styled(format!("{label:<18}"), style),
                Span::raw(value),
            ])
        })
        .collect::<Vec<_>>();

    body_lines.push(Line::from(""));
    body_lines.push(Line::from(format!(
        "Auth state: {}",
        if form.gmail_authorized {
            "ready"
        } else {
            "not verified"
        }
    )));
    let result_lines = format_account_result_lines(form.last_result.as_ref());
    if !result_lines.is_empty() {
        body_lines.extend(result_lines);
    }

    let paragraph = Paragraph::new(body_lines)
        .block(
            Block::default()
                .title(" Account Form ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.warning)),
        )
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, layout[1]);

    let footer = Paragraph::new(if form.editing_field {
        "Esc/Enter:finish  Left/Right:move  Backspace/Delete:edit  Tab:next field"
    } else {
        "j/k:move  Enter/i:edit  h/l:switch mode  s:save  t:test  r:reauth  Esc:close"
    })
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.accent)),
    );
    frame.render_widget(footer, layout[2]);

    if form.pending_mode_switch.is_some() {
        draw_mode_switch_confirm_modal(frame, area, theme);
    }
}

fn build_fields(form: &crate::app::AccountFormState) -> Vec<(&'static str, String, bool)> {
    let mut fields = vec![
        (
            "Mode",
            match form.mode {
                AccountFormMode::Gmail => "Gmail".to_string(),
                AccountFormMode::ImapSmtp => "IMAP + SMTP".to_string(),
                AccountFormMode::SmtpOnly => "SMTP only".to_string(),
            },
            false,
        ),
        ("Account key", form.key.clone(), true),
        ("Display name", form.name.clone(), true),
        ("Email", form.email.clone(), true),
    ];

    match form.mode {
        AccountFormMode::Gmail => {
            fields.push((
                "Credential source",
                match form.gmail_credential_source {
                    mxr_protocol::GmailCredentialSourceData::Bundled => "Bundled".to_string(),
                    mxr_protocol::GmailCredentialSourceData::Custom => "Custom".to_string(),
                },
                false,
            ));
            if form.gmail_credential_source == mxr_protocol::GmailCredentialSourceData::Custom {
                fields.push(("Client ID", form.gmail_client_id.clone(), true));
                fields.push(("Client Secret", mask(&form.gmail_client_secret), true));
            }
            fields.push(("Token ref", form.gmail_token_ref.clone(), false));
        }
        AccountFormMode::ImapSmtp => {
            fields.extend([
                ("IMAP host", form.imap_host.clone(), true),
                ("IMAP port", form.imap_port.clone(), true),
                ("IMAP user", form.imap_username.clone(), true),
                ("IMAP pass ref", form.imap_password_ref.clone(), true),
                ("IMAP password", mask(&form.imap_password), true),
                ("SMTP host", form.smtp_host.clone(), true),
                ("SMTP port", form.smtp_port.clone(), true),
                ("SMTP user", form.smtp_username.clone(), true),
                ("SMTP pass ref", form.smtp_password_ref.clone(), true),
                ("SMTP password", mask(&form.smtp_password), true),
            ]);
        }
        AccountFormMode::SmtpOnly => {
            fields.extend([
                ("SMTP host", form.smtp_host.clone(), true),
                ("SMTP port", form.smtp_port.clone(), true),
                ("SMTP user", form.smtp_username.clone(), true),
                ("SMTP pass ref", form.smtp_password_ref.clone(), true),
                ("SMTP password", mask(&form.smtp_password), true),
            ]);
        }
    }

    fields
}

fn format_account_result_lines(
    result: Option<&mxr_protocol::AccountOperationResult>,
) -> Vec<Line<'static>> {
    let Some(result) = result else {
        return Vec::new();
    };
    let mut lines = vec![Line::from(result.summary.clone())];
    if let Some(step) = &result.save {
        lines.push(Line::from(format_step("Save", step)));
    }
    if let Some(step) = &result.auth {
        lines.push(Line::from(format_step("Auth", step)));
    }
    if let Some(step) = &result.sync {
        lines.push(Line::from(format_step("Sync", step)));
    }
    if let Some(step) = &result.send {
        lines.push(Line::from(format_step("Send", step)));
    }
    lines
}

fn format_step(label: &str, step: &mxr_protocol::AccountOperationStep) -> String {
    format!(
        "{label}: {} - {}",
        if step.ok { "ok" } else { "failed" },
        step.detail
    )
}

fn render_with_cursor(value: &str, cursor: usize) -> String {
    let mut rendered = String::new();
    let mut inserted = false;
    for (index, ch) in value.chars().enumerate() {
        if index == cursor {
            rendered.push('|');
            inserted = true;
        }
        rendered.push(ch);
    }
    if !inserted {
        rendered.push('|');
    }
    rendered
}

fn mask(value: &str) -> String {
    if value.is_empty() {
        String::new()
    } else {
        "*".repeat(value.chars().count().min(12))
    }
}

fn draw_onboarding_modal(frame: &mut Frame, area: Rect, theme: &crate::theme::Theme) {
    let popup = centered_rect(54, 28, area);
    frame.render_widget(Clear, popup);

    let lines = vec![
        Line::from("You need to set up an account first"),
        Line::from("before you can use mxr."),
        Line::from(""),
        Line::from("Press Enter to continue to the account form."),
    ];

    let paragraph = Paragraph::new(lines)
        .block(
            Block::default()
                .title(" Welcome ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.accent))
                .style(Style::default().bg(theme.modal_bg)),
        )
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, popup);
}

fn draw_mode_switch_confirm_modal(
    frame: &mut Frame,
    area: Rect,
    theme: &crate::theme::Theme,
) {
    let popup = centered_rect(58, 28, area);
    frame.render_widget(Clear, popup);

    let lines = vec![
        Line::from("Switch account mode?"),
        Line::from(""),
        Line::from("This can hide or replace fields you already filled in."),
        Line::from(""),
        Line::from("Enter/y: switch  Esc/n: stay"),
    ];

    let paragraph = Paragraph::new(lines)
        .block(
            Block::default()
                .title(" Confirm Mode Switch ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.warning))
                .style(Style::default().bg(theme.modal_bg)),
        )
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, popup);
}

fn centered_rect(percent_x: u16, percent_y: u16, rect: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(rect);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}
