use crate::app::{AccountFormMode, AccountsPageState};
use ratatui::prelude::*;
use ratatui::widgets::*;
use throbber_widgets_tui::{Throbber, BRAILLE_SIX};
use super::centered_rect;

pub fn draw(
    frame: &mut Frame,
    area: Rect,
    state: &AccountsPageState,
    sync_statuses: &[mxr_protocol::AccountSyncStatus],
    theme: &crate::theme::Theme,
) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(62), Constraint::Percentage(38)])
        .split(area);

    let detail_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(3)])
        .split(chunks[0]);

    let items = state
        .accounts
        .iter()
        .map(|account| {
            let sync = account.sync_kind.as_deref().unwrap_or("none");
            let send = account.send_kind.as_deref().unwrap_or("none");
            let source = match account.source {
                mxr_protocol::AccountSourceData::Runtime => "runtime",
                mxr_protocol::AccountSourceData::Config => "config",
                mxr_protocol::AccountSourceData::Both => "both",
            };
            let badges = [
                if account.is_default {
                    Some("default")
                } else {
                    None
                },
                Some(sync),
                Some(send),
                Some(source),
            ]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" | ");
            // Phase 2.3: surface an unhealthy indicator inline so users
            // can spot a broken account without opening Diagnostics.
            let unhealthy = sync_statuses
                .iter()
                .find(|s| s.account_id == account.account_id)
                .filter(|s| !s.healthy);
            let mut spans = vec![
                Span::styled(
                    account.name.clone(),
                    Style::default().fg(theme.text_primary).bold(),
                ),
                Span::styled(
                    format!("  [{badges}]"),
                    Style::default().fg(theme.text_secondary),
                ),
            ];
            if unhealthy.is_some() {
                spans.push(Span::styled(
                    "  [unhealthy]",
                    Style::default().fg(theme.error).bold(),
                ));
            }
            ListItem::new(Line::from(spans))
        })
        .collect::<Vec<_>>();
    let list = List::new(items)
        .block(
            Block::default()
                .title(" Accounts / Browse ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.accent)),
        )
        .highlight_style(theme.highlight_style())
        .highlight_symbol("> ");
    let mut list_state = ListState::default().with_selected(Some(state.selected_index));
    frame.render_stateful_widget(list, chunks[1], &mut list_state);

    if state.form.visible {
        draw_form(frame, chunks[1], state, theme);
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
            Line::from(vec![
                Span::styled(
                    account.name.clone(),
                    Style::default().fg(theme.accent).bold(),
                ),
                Span::styled(
                    if account.is_default {
                        "  [default]"
                    } else {
                        "  [secondary]"
                    },
                    Style::default().fg(theme.warning),
                ),
            ]),
            Line::from(""),
            Line::from(format!(
                "Summary: {} via {}",
                account.email, account.provider_kind
            )),
            Line::from(format!(
                "Key: {}",
                account.key.as_deref().unwrap_or("(runtime-only)")
            )),
            Line::from(format!(
                "Auth: {}",
                if account.sync.as_ref().is_some_and(|sync| matches!(
                    sync,
                    mxr_protocol::AccountSyncConfigData::Gmail { .. }
                )) {
                    "gmail configured"
                } else {
                    "managed by saved config"
                }
            )),
            Line::from(format!(
                "Sync: {}",
                account.sync_kind.as_deref().unwrap_or("none")
            )),
            Line::from(format!(
                "Send: {}",
                account.send_kind.as_deref().unwrap_or("none")
            )),
            Line::from(format!(
                "Default: {}",
                if account.is_default { "yes" } else { "no" }
            )),
            Line::from(format!("Source: {source}")),
            Line::from(format!("Editable: {editability}")),
            Line::from(""),
            Line::from("Actions"),
            Line::from("Enter edit selected account"),
            Line::from("t test account"),
            Line::from("d set default"),
            Line::from("c edit config"),
        ]
    } else {
        let mut lines = vec![
            Line::from("Accounts connect mxr to sync and send mail."),
            Line::from(""),
            Line::from("Press n to add Gmail, IMAP + SMTP, or SMTP-only."),
            Line::from("Press c to open config in your editor."),
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

    let paragraph = Paragraph::new(detail_lines)
        .block(
            Block::default()
                .title(" Account Details ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.warning)),
        )
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, detail_chunks[0]);

    let mut footer_lines = Vec::new();
    if state.operation_in_flight {
        footer_lines.push(account_operation_status_line(state, theme));
    } else if let Some(status) = &state.status {
        footer_lines.push(Line::from(status.clone()));
    }
    let selected_unhealthy = state
        .accounts
        .get(state.selected_index)
        .and_then(|account| {
            sync_statuses
                .iter()
                .find(|s| s.account_id == account.account_id)
                .filter(|s| !s.healthy)
        })
        .is_some();
    let mut footer_hint = if state.accounts.is_empty() {
        "n:new account  c:edit config  Esc:mailbox".to_string()
    } else if selected_unhealthy {
        "j/k:select  Enter:edit  n:new  t:test  d:default  c:config  r:repair  R:refresh"
            .to_string()
    } else {
        "j/k:select  Enter:edit  n:new  t:test  d:default  c:config  r:refresh".to_string()
    };
    if account_result_has_details(state.last_result.as_ref()) {
        footer_hint.push_str("  O:details");
    }
    footer_lines.push(Line::from(footer_hint));
    let footer = Paragraph::new(footer_lines).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.accent)),
    );
    frame.render_widget(footer, detail_chunks[1]);

    if let Some(session) = &state.active_auth_session {
        draw_device_code_overlay(frame, area, session, theme);
    }

    if state.resume_new_account_draft_prompt_open {
        draw_resume_new_account_draft_modal(frame, area, theme);
    }
}

fn draw_form(
    frame: &mut Frame,
    area: Rect,
    state: &AccountsPageState,
    theme: &crate::theme::Theme,
) {
    let form = &state.form;
    let titles = [
        "Gmail",
        "IMAP + SMTP",
        "SMTP only",
        "Outlook",
        "Outlook (Work)",
    ]
    .into_iter()
    .map(Line::from)
    .collect::<Vec<_>>();
    let selected = match form.mode {
        AccountFormMode::Gmail => 0,
        AccountFormMode::ImapSmtp => 1,
        AccountFormMode::SmtpOnly => 2,
        AccountFormMode::OutlookPersonal => 3,
        AccountFormMode::OutlookWork => 4,
    };

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(3),
        ])
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
    let auth_ready = match form.mode {
        AccountFormMode::Gmail => form.gmail_authorized,
        AccountFormMode::OutlookPersonal | AccountFormMode::OutlookWork => form.outlook_authorized,
        _ => false,
    };
    if matches!(
        form.mode,
        AccountFormMode::Gmail | AccountFormMode::OutlookPersonal | AccountFormMode::OutlookWork
    ) {
        body_lines.push(Line::from(format!(
            "Auth state: {}",
            if auth_ready {
                "ready"
            } else {
                "not authorized"
            }
        )));
    }
    body_lines.push(Line::from(""));
    body_lines.extend(account_form_hint_lines(form, &fields, theme));
    let result_lines = format_account_result_lines(form.last_result.as_ref());
    if !result_lines.is_empty() {
        body_lines.push(Line::from(""));
        body_lines.extend(result_lines);
        let result_hint_lines = account_result_hint_lines(form, form.last_result.as_ref(), theme);
        if !result_hint_lines.is_empty() {
            body_lines.push(Line::from(""));
            body_lines.extend(result_hint_lines);
        }
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

    let mut footer_lines = Vec::new();
    if state.operation_in_flight {
        footer_lines.push(account_operation_status_line(state, theme));
    }
    let mut footer_hint = if form.editing_field {
        "Enter/Esc:finish  Left/Right:cursor  Backspace/Delete:edit  Tab/Shift-Tab:field"
            .to_string()
    } else {
        "j/k or Tab:field  Enter/i:edit  Shift-Tab:prev  h/l:mode  s:save  t:test  r:reauth  Esc:close"
            .to_string()
    };
    if !form.editing_field && account_result_has_details(form.last_result.as_ref()) {
        footer_hint.push_str("  o:details");
    }
    footer_lines.push(Line::from(footer_hint));
    let footer = Paragraph::new(footer_lines).block(
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
                AccountFormMode::OutlookPersonal => "Outlook".to_string(),
                AccountFormMode::OutlookWork => "Outlook (Work)".to_string(),
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
                (
                    "IMAP auth",
                    if form.imap_auth_required {
                        "Required".to_string()
                    } else {
                        "Not required".to_string()
                    },
                    false,
                ),
                ("IMAP pass ref", form.imap_password_ref.clone(), true),
                ("IMAP password", mask(&form.imap_password), true),
                ("SMTP host", form.smtp_host.clone(), true),
                ("SMTP port", form.smtp_port.clone(), true),
                ("SMTP user", form.smtp_username.clone(), true),
                (
                    "SMTP auth",
                    if form.smtp_auth_required {
                        "Required".to_string()
                    } else {
                        "Not required".to_string()
                    },
                    false,
                ),
                ("SMTP pass ref", form.smtp_password_ref.clone(), true),
                ("SMTP password", mask(&form.smtp_password), true),
            ]);
        }
        AccountFormMode::SmtpOnly => {
            fields.extend([
                ("SMTP host", form.smtp_host.clone(), true),
                ("SMTP port", form.smtp_port.clone(), true),
                ("SMTP user", form.smtp_username.clone(), true),
                (
                    "SMTP auth",
                    if form.smtp_auth_required {
                        "Required".to_string()
                    } else {
                        "Not required".to_string()
                    },
                    false,
                ),
                ("SMTP pass ref", form.smtp_password_ref.clone(), true),
                ("SMTP password", mask(&form.smtp_password), true),
            ]);
        }
        AccountFormMode::OutlookPersonal | AccountFormMode::OutlookWork => {
            fields.push(("Azure client ID", form.outlook_client_id.clone(), true));
            fields.push(("Outlook token ref", form.outlook_token_ref.clone(), false));
        }
    }

    fields
}

fn account_form_hint_lines(
    form: &crate::app::AccountFormState,
    fields: &[(&'static str, String, bool)],
    theme: &crate::theme::Theme,
) -> Vec<Line<'static>> {
    let Some((label, _, _)) = fields.get(form.active_field) else {
        return Vec::new();
    };

    let hints = vec![
        if form.editing_field {
            "Tip: type to edit. Enter or Esc finishes. Tab and Shift-Tab jump between fields."
                .to_string()
        } else {
            "Tip: Tab and Shift-Tab move between fields. Enter or i edits the selected field."
                .to_string()
        },
        format!("{label}: {}", account_field_help_text(label)),
    ];

    hints
        .into_iter()
        .map(|hint| {
            Line::from(Span::styled(
                hint,
                Style::default().fg(theme.text_secondary),
            ))
        })
        .collect()
}

fn account_field_help_text(label: &str) -> &'static str {
    match label {
        "Mode" => "Choose Gmail OAuth, IMAP + SMTP, SMTP-only, Outlook (personal @outlook.com), or Outlook (Work) for Microsoft 365/Exchange.",
        "Azure client ID" => "Optional: your own Azure app client ID. Leave blank to use the bundled mxr client ID (if available). Register at portal.azure.com as a multi-tenant public client app.",
        "Account key" => "Short internal ID used in config and secret refs, like work or personal.",
        "Display name" => "Shown as the sender name on outgoing mail.",
        "Email" => "Primary email address for this account.",
        "Credential source" => "Bundled uses mxr's built-in Gmail app. Custom uses your own Google OAuth client.",
        "Client ID" => "Google OAuth client ID for custom Gmail auth.",
        "Client Secret" => "Google OAuth client secret for custom Gmail auth.",
        "Token ref" => "Where mxr stores the Gmail OAuth token. Usually auto-filled from the account key.",
        "Outlook token ref" => "Where mxr stores the Outlook OAuth token. Usually auto-filled from the account key.",
        "IMAP host" => "Incoming mail server hostname, usually something like imap.example.com.",
        "IMAP port" => "Usually 993 for TLS IMAP.",
        "IMAP user" => "Usually your full email address or mailbox login.",
        "IMAP auth" => "Toggle whether mxr should authenticate to the IMAP server. Turn this off only for servers that explicitly allow anonymous or no-auth access.",
        "IMAP pass ref" => "Secret/keychain ref for the IMAP app password. If you leave it blank, mxr will generate one from the account key when needed.",
        "IMAP password" => "Inline IMAP app password. Many providers require this instead of your normal account password.",
        "SMTP host" => "Outgoing mail server hostname, usually something like smtp.example.com.",
        "SMTP port" => "Usually 587 for STARTTLS SMTP.",
        "SMTP user" => "Usually your full email address or SMTP login.",
        "SMTP auth" => "Toggle whether mxr should authenticate to the SMTP server. Turn this off only for relay servers that explicitly allow sending without auth.",
        "SMTP pass ref" => "Secret/keychain ref for the SMTP app password. If you leave it blank, mxr will generate one from the account key when needed.",
        "SMTP password" => "Inline SMTP app password. Many providers require this instead of your normal account password.",
        _ => "Update this field for the selected account.",
    }
}

fn format_account_result_lines(
    result: Option<&mxr_protocol::AccountOperationResult>,
) -> Vec<Line<'static>> {
    let Some(result) = result else {
        return Vec::new();
    };
    let mut lines = vec![Line::from(result.summary.clone())];
    let save_label = if result.summary.starts_with("Account form has problems.") {
        "Form"
    } else {
        "Save"
    };
    if let Some(step) = &result.save {
        lines.push(Line::from(format_step(save_label, step)));
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

fn account_operation_status_line(
    state: &AccountsPageState,
    theme: &crate::theme::Theme,
) -> Line<'static> {
    let status = state
        .status
        .clone()
        .unwrap_or_else(|| "Working...".to_string());
    Line::from(vec![
        Throbber::default()
            .throbber_set(BRAILLE_SIX)
            .throbber_style(Style::default().fg(theme.accent))
            .to_symbol_span(&state.throbber),
        Span::raw(" "),
        Span::styled(status, Style::default().fg(theme.text_secondary)),
    ])
}

fn account_result_hint_lines(
    form: &crate::app::AccountFormState,
    result: Option<&mxr_protocol::AccountOperationResult>,
    theme: &crate::theme::Theme,
) -> Vec<Line<'static>> {
    let Some(result) = result else {
        return Vec::new();
    };
    if result.summary.starts_with("Account form has problems.") {
        return Vec::new();
    }

    let mut hints = Vec::new();
    if let Some(step) = &result.auth {
        if !step.ok {
            push_unique_hint(&mut hints, gmail_result_hint(&step.detail));
        }
    }
    if let Some(step) = &result.sync {
        if !step.ok {
            push_unique_hint(
                &mut hints,
                server_result_hint("IMAP", &step.detail, form.imap_auth_required),
            );
        }
    }
    if let Some(step) = &result.send {
        if !step.ok {
            push_unique_hint(
                &mut hints,
                server_result_hint("SMTP", &step.detail, form.smtp_auth_required),
            );
        }
    }

    hints
        .into_iter()
        .map(|hint| {
            Line::from(Span::styled(
                format!("Hint: {hint}"),
                Style::default().fg(theme.text_secondary),
            ))
        })
        .collect()
}

fn push_unique_hint(hints: &mut Vec<String>, hint: Option<String>) {
    let Some(hint) = hint else {
        return;
    };
    if !hints.iter().any(|existing| existing == &hint) {
        hints.push(hint);
    }
}

fn account_result_has_details(result: Option<&mxr_protocol::AccountOperationResult>) -> bool {
    let Some(result) = result else {
        return false;
    };

    result.save.is_some() || result.auth.is_some() || result.sync.is_some() || result.send.is_some()
}

fn gmail_result_hint(detail: &str) -> Option<String> {
    let detail = detail.to_ascii_lowercase();
    if detail.contains("client id")
        || detail.contains("client secret")
        || detail.contains("oauth")
        || detail.contains("token")
        || detail.contains("credential")
    {
        return Some(
            "Check fields: Credential source, Client ID, Client Secret, and Token ref.".to_string(),
        );
    }
    None
}

fn server_result_hint(service: &str, detail: &str, auth_required: bool) -> Option<String> {
    let detail = detail.to_ascii_lowercase();
    if detail.contains("namespace response")
        || detail.contains("could not parse")
        || detail.contains("unsupported format")
    {
        return Some(format!(
            "{service} returned an unexpected protocol response. This looks like a server compatibility issue, not a bad password. Open details for the exact step."
        ));
    }
    if detail.contains("keyring") {
        return Some(format!(
            "Check fields: {service} pass ref and {service} user."
        ));
    }
    if detail.contains("tls")
        || detail.contains("ssl")
        || detail.contains("starttls")
        || detail.contains("certificate")
    {
        return Some(format!(
            "Check fields: {service} host and {service} port. Server TLS settings may also be wrong."
        ));
    }
    if detail.contains("connect")
        || detail.contains("connection")
        || detail.contains("timed out")
        || detail.contains("timeout")
        || detail.contains("refused")
        || detail.contains("resolve")
        || detail.contains("name or service not known")
        || detail.contains("unreachable")
    {
        return Some(format!("Check fields: {service} host and {service} port."));
    }
    if detail.contains("auth")
        || detail.contains("login")
        || detail.contains("credential")
        || detail.contains("username")
        || detail.contains("password")
    {
        if auth_required {
            return Some(format!(
                "Check fields: {service} user, {service} pass ref, and {service} password."
            ));
        }
        return Some(format!(
            "Check field: {service} auth. This server likely requires authentication."
        ));
    }
    Some(if auth_required {
        format!(
            "Check fields: {service} host, {service} port, {service} user, and {service} password."
        )
    } else {
        format!("Check fields: {service} host, {service} port, and {service} auth.")
    })
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

pub fn draw_account_setup_onboarding(frame: &mut Frame, area: Rect, theme: &crate::theme::Theme) {
    let popup = centered_rect(64, 60, area);
    frame.render_widget(Clear, popup);

    let label_style = Style::default().fg(theme.accent).bold();
    let dim_style = Style::default().fg(theme.text_secondary);

    let lines = vec![
        Line::from(Span::styled(
            "Welcome to mxr",
            Style::default().fg(theme.text_primary).bold(),
        )),
        Line::from(""),
        Line::from("Pick a setup path:"),
        Line::from(""),
        Line::from(vec![
            Span::styled(" d ", label_style),
            Span::raw(" "),
            Span::styled("Try the demo (in-memory fake provider)", dim_style),
        ]),
        Line::from(vec![
            Span::styled("   ", Style::default()),
            Span::styled("   ", Style::default()),
            Span::styled(
                "→ runs `mxr setup --demo` for you",
                Style::default().fg(theme.text_muted),
            ),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled(" g ", label_style),
            Span::raw(" "),
            Span::styled("Connect a Gmail account", dim_style),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled(" i ", label_style),
            Span::raw(" "),
            Span::styled("Connect an IMAP / SMTP account", dim_style),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled(" Enter ", label_style),
            Span::raw(" "),
            Span::styled("Open the account form (defaults)", dim_style),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled(" Esc ", label_style),
            Span::raw(" "),
            Span::styled("Dismiss (you'll see this on next launch)", dim_style),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "All paths land in `mxr accounts`. The demo path is local-only.",
            Style::default().fg(theme.text_muted),
        )),
    ];

    let paragraph = Paragraph::new(lines)
        .block(
            Block::default()
                .title(" Setup ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.accent))
                .style(Style::default().bg(theme.modal_bg)),
        )
        .alignment(Alignment::Left)
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, popup);
}

fn draw_mode_switch_confirm_modal(frame: &mut Frame, area: Rect, theme: &crate::theme::Theme) {
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

fn draw_resume_new_account_draft_modal(frame: &mut Frame, area: Rect, theme: &crate::theme::Theme) {
    let popup = centered_rect(62, 32, area);
    frame.render_widget(Clear, popup);

    let lines = vec![
        Line::from("Unsaved new account form found."),
        Line::from(""),
        Line::from("Continue the draft you were editing,"),
        Line::from("or start a fresh account form."),
        Line::from(""),
        Line::from("Enter/c: continue  n: start new  Esc: cancel"),
    ];

    let paragraph = Paragraph::new(lines)
        .block(
            Block::default()
                .title(" Resume Draft ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.warning))
                .style(Style::default().bg(theme.modal_bg)),
        )
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, popup);
}

fn draw_device_code_overlay(
    frame: &mut Frame,
    area: Rect,
    session: &mxr_protocol::AuthSessionData,
    theme: &crate::theme::Theme,
) {
    let popup = centered_rect(66, 50, area);
    frame.render_widget(Clear, popup);

    let user_code = session.user_code.as_deref().unwrap_or("—");
    let url = session
        .verification_uri
        .as_deref()
        .unwrap_or("https://microsoft.com/devicelogin");

    let expiry_line = if let Some(expires_at) = session.expires_at_unix {
        let remaining = expires_at - chrono::Utc::now().timestamp();
        if remaining > 0 {
            format!("Code expires in {remaining}s")
        } else {
            "Code expired".to_string()
        }
    } else {
        String::new()
    };

    let mut lines = vec![
        Line::from(""),
        Line::from(Span::styled(
            "Open this URL in your browser:",
            Style::default().fg(theme.accent),
        )),
        Line::from(""),
        Line::from(Span::styled(url, Style::default().fg(theme.warning).bold())),
        Line::from(""),
        Line::from("Then enter this code:"),
        Line::from(""),
        Line::from(Span::styled(
            user_code,
            Style::default()
                .fg(theme.accent)
                .bold()
                .add_modifier(Modifier::REVERSED),
        )),
        Line::from(""),
        Line::from("Waiting for browser confirmation..."),
    ];
    if !expiry_line.is_empty() {
        lines.push(Line::from(Span::styled(
            expiry_line,
            Style::default().fg(theme.warning),
        )));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Esc: cancel",
        Style::default().fg(theme.accent),
    )));

    let paragraph = Paragraph::new(lines)
        .block(
            Block::default()
                .title(" Authorize with Microsoft ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.accent))
                .style(Style::default().bg(theme.modal_bg)),
        )
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, popup);
}


#[cfg(test)]
mod tests {
    use super::{account_field_help_text, draw, draw_account_setup_onboarding, server_result_hint};
    use crate::app::AccountsPageState;
    use mxr_test_support::render_to_string;
    use ratatui::layout::Rect;

    #[test]
    fn account_setup_onboarding_lists_three_paths() {
        let snapshot = render_to_string(80, 28, |frame| {
            draw_account_setup_onboarding(
                frame,
                Rect::new(0, 0, 80, 28),
                &crate::theme::Theme::default(),
            );
        });
        assert!(
            snapshot.contains("Try the demo"),
            "demo path must be listed; got:\n{snapshot}",
        );
        assert!(
            snapshot.contains("Gmail"),
            "Gmail path must be listed; got:\n{snapshot}",
        );
        assert!(
            snapshot.contains("IMAP"),
            "IMAP path must be listed; got:\n{snapshot}",
        );
    }

    fn fake_account(name: &str) -> mxr_protocol::AccountSummaryData {
        mxr_protocol::AccountSummaryData {
            account_id: mxr_core::AccountId::new(),
            key: Some("user".into()),
            name: name.into(),
            email: format!("{name}@example.com"),
            provider_kind: "imap".into(),
            sync_kind: Some("imap".into()),
            send_kind: Some("smtp".into()),
            enabled: true,
            is_default: false,
            source: mxr_protocol::AccountSourceData::Config,
            editable: mxr_protocol::AccountEditModeData::Full,
            sync: None,
            send: None,
            capabilities: Default::default(),
        }
    }

    fn fake_status(
        account: &mxr_protocol::AccountSummaryData,
        healthy: bool,
    ) -> mxr_protocol::AccountSyncStatus {
        mxr_protocol::AccountSyncStatus {
            account_id: account.account_id.clone(),
            account_name: account.name.clone(),
            last_attempt_at: None,
            last_success_at: None,
            last_error: if healthy {
                None
            } else {
                Some("auth failed".into())
            },
            failure_class: if healthy { None } else { Some("auth".into()) },
            consecutive_failures: if healthy { 0 } else { 3 },
            backoff_until: None,
            sync_in_progress: false,
            current_cursor_summary: None,
            last_synced_count: 0,
            healthy,
        }
    }

    /// Phase 2.3 / Behavior 1: an account whose sync status reports
    /// `healthy=false` renders an "[unhealthy: r repairs]" inline
    /// indicator that the user can spot at a glance. Catches "no
    /// indicator at all" regressions.
    #[test]
    fn unhealthy_account_renders_repair_indicator() {
        let account = fake_account("Work");
        let status = fake_status(&account, false);
        let state = AccountsPageState {
            accounts: vec![account],
            ..AccountsPageState::default()
        };

        let rendered = render_to_string(120, 24, |frame| {
            draw(
                frame,
                Rect::new(0, 0, 120, 24),
                &state,
                std::slice::from_ref(&status),
                &crate::theme::Theme::default(),
            );
        });
        assert!(
            rendered.contains("[unhealthy]"),
            "rendered output must include the unhealthy indicator; got:\n{rendered}"
        );
        assert!(
            rendered.contains("r:repair"),
            "footer hint must replace `r:refresh` with `r:repair` when an unhealthy account is selected; got:\n{rendered}"
        );
    }

    /// Phase 2.3: a healthy account does NOT render the indicator —
    /// otherwise users would see false-positive alarms on every
    /// account.
    #[test]
    fn healthy_account_does_not_render_repair_indicator() {
        let account = fake_account("Work");
        let status = fake_status(&account, true);
        let state = AccountsPageState {
            accounts: vec![account],
            ..AccountsPageState::default()
        };

        let rendered = render_to_string(120, 24, |frame| {
            draw(
                frame,
                Rect::new(0, 0, 120, 24),
                &state,
                std::slice::from_ref(&status),
                &crate::theme::Theme::default(),
            );
        });
        assert!(
            !rendered.contains("[unhealthy"),
            "healthy account must not render the unhealthy indicator; got:\n{rendered}"
        );
    }

    #[test]
    fn account_key_help_explains_internal_id_and_refs() {
        let help = account_field_help_text("Account key");
        assert!(help.contains("internal ID"));
        assert!(help.contains("secret refs"));
    }

    #[test]
    fn password_field_help_mentions_app_passwords() {
        assert!(account_field_help_text("IMAP password").contains("app password"));
        assert!(account_field_help_text("SMTP pass ref").contains("app password"));
        assert!(!account_field_help_text("Email").contains("app password"));
    }

    #[test]
    fn auth_toggle_help_explains_when_to_disable_it() {
        assert!(account_field_help_text("IMAP auth").contains("anonymous"));
        assert!(account_field_help_text("SMTP auth").contains("without auth"));
    }

    #[test]
    fn parse_failures_get_compatibility_hint_instead_of_password_hint() {
        let hint = server_result_hint(
            "IMAP",
            "Protocol error: IMAP server returned a NAMESPACE response in an unsupported format during folder discovery.",
            true,
        )
        .unwrap();
        assert!(hint.contains("compatibility issue"));
        assert!(!hint.contains("IMAP password"));
    }
}
