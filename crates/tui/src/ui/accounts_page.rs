use crate::app::{AccountFormMode, AccountsPageState};
use ratatui::prelude::*;
use ratatui::widgets::*;

pub fn draw(frame: &mut Frame, area: Rect, state: &AccountsPageState) {
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
            .border_style(Style::default().fg(Color::Cyan)),
    );
    let mut list_state = ListState::default().with_selected(Some(state.selected_index));
    frame.render_stateful_widget(list, chunks[0], &mut list_state);

    if state.form.visible {
        draw_form(frame, chunks[1], &state.form);
        return;
    }

    let detail_lines = if let Some(account) = state.accounts.get(state.selected_index) {
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
            Line::from(""),
            Line::from(state.status.clone().unwrap_or_else(|| {
                "n:new  Enter:edit  t:test  d:set default".to_string()
            })),
        ]
    } else {
        vec![
            Line::from("No accounts configured"),
            Line::from(""),
            Line::from("Press n to add an IMAP/SMTP account"),
        ]
    };

    let paragraph = Paragraph::new(detail_lines)
        .block(
            Block::default()
                .title(" Details ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Yellow)),
        )
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, chunks[1]);
}

fn draw_form(frame: &mut Frame, area: Rect, form: &crate::app::AccountFormState) {
    let titles = ["IMAP + SMTP", "SMTP only"]
        .into_iter()
        .map(Line::from)
        .collect::<Vec<_>>();
    let selected = match form.mode {
        AccountFormMode::ImapSmtp => 0,
        AccountFormMode::SmtpOnly => 1,
    };

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0)])
        .split(area);

    let tabs = Tabs::new(titles)
        .select(selected)
        .block(
            Block::default()
                .title(" Account Mode ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan)),
        )
        .highlight_style(Style::default().fg(Color::Yellow).bold());
    frame.render_widget(tabs, layout[0]);

    let fields = build_fields(form);
    let lines = fields
        .iter()
        .enumerate()
        .map(|(index, (label, value))| {
            let style = if index == form.active_field {
                Style::default().fg(Color::Yellow).bold()
            } else {
                Style::default().fg(Color::White)
            };
            Line::from(vec![
                Span::styled(format!("{label:<16}"), style),
                Span::raw(value.clone()),
            ])
        })
        .collect::<Vec<_>>();

    let paragraph = Paragraph::new(lines)
        .block(
            Block::default()
                .title(" Account Form ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Yellow)),
        )
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, layout[1]);
}

fn build_fields(form: &crate::app::AccountFormState) -> Vec<(&'static str, String)> {
    let mut fields = vec![
        ("Mode", match form.mode {
            AccountFormMode::ImapSmtp => "IMAP + SMTP".to_string(),
            AccountFormMode::SmtpOnly => "SMTP only".to_string(),
        }),
        ("Account key", form.key.clone()),
        ("Display name", form.name.clone()),
        ("Email", form.email.clone()),
    ];
    if matches!(form.mode, AccountFormMode::ImapSmtp) {
        fields.extend([
            ("IMAP host", form.imap_host.clone()),
            ("IMAP port", form.imap_port.clone()),
            ("IMAP user", form.imap_username.clone()),
            ("IMAP pass ref", form.imap_password_ref.clone()),
            ("IMAP password", mask(&form.imap_password)),
        ]);
    }
    fields.extend([
        ("SMTP host", form.smtp_host.clone()),
        ("SMTP port", form.smtp_port.clone()),
        ("SMTP user", form.smtp_username.clone()),
        ("SMTP pass ref", form.smtp_password_ref.clone()),
        ("SMTP password", mask(&form.smtp_password)),
    ]);
    fields
}

fn mask(value: &str) -> String {
    if value.is_empty() {
        String::new()
    } else {
        "*".repeat(value.chars().count().min(12))
    }
}
