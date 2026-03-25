mod accounts;
mod diagnostics;
mod mailbox;
mod mutations;
mod rules;

use crate::mxr_core::provider::MailSyncProvider;
#[cfg(test)]
use crate::mxr_core::types::UnsubscribeMethod;
use crate::mxr_core::types::{ExportFormat, Snoozed};
use crate::mxr_export::{ExportAttachment, ExportMessage, ExportThread};
use crate::mxr_protocol::*;
use crate::mxr_reader::ReaderConfig;
use crate::mxr_rules::{
    Conditions, DryRunResult, FieldCondition, Rule, RuleAction, RuleEngine, StringMatch,
};
use crate::mxr_search::parse_query;
use crate::state::AppState;
use std::io::{BufRead, BufReader};
use std::sync::Arc;

type HandlerResult = Result<ResponseData, String>;

pub async fn handle_request(state: &Arc<AppState>, msg: &IpcMessage) -> IpcMessage {
    let response_data = match &msg.payload {
        IpcPayload::Request(req) => dispatch(state, req).await,
        _ => Response::Error {
            message: "Expected a Request".to_string(),
        },
    };

    IpcMessage {
        id: msg.id,
        payload: IpcPayload::Response(response_data),
    }
}

async fn dispatch(state: &Arc<AppState>, req: &Request) -> Response {
    let result = match req {
        Request::ListEnvelopes {
            label_id,
            account_id,
            limit,
            offset,
        } => {
            mailbox::list_envelopes(
                state,
                label_id.as_ref(),
                account_id.as_ref(),
                *limit,
                *offset,
            )
            .await
        }
        Request::ListEnvelopesByIds { message_ids } => {
            mailbox::list_envelopes_by_ids(state, message_ids).await
        }
        Request::GetEnvelope { message_id } => mailbox::get_envelope(state, message_id).await,
        Request::GetBody { message_id } => mailbox::get_body(state, message_id).await,
        Request::DownloadAttachment {
            message_id,
            attachment_id,
        } => mailbox::download_attachment(state, message_id, attachment_id).await,
        Request::OpenAttachment {
            message_id,
            attachment_id,
        } => mailbox::open_attachment(state, message_id, attachment_id).await,
        Request::ListBodies { message_ids } => mailbox::list_bodies(state, message_ids).await,
        Request::GetThread { thread_id } => mailbox::get_thread(state, thread_id).await,
        Request::ListLabels { account_id } => {
            mailbox::list_labels(state, account_id.as_ref()).await
        }
        Request::CreateLabel {
            name,
            color,
            account_id,
        } => mailbox::create_label(state, name, color.as_deref(), account_id.as_ref()).await,
        Request::DeleteLabel { name, account_id } => {
            mailbox::delete_label(state, name, account_id.as_ref()).await
        }
        Request::RenameLabel {
            old,
            new,
            account_id,
        } => mailbox::rename_label(state, old, new, account_id.as_ref()).await,
        Request::ListRules => rules::list_rules(state).await,
        Request::ListAccounts => accounts::list_accounts(state).await,
        Request::ListAccountsConfig => accounts::list_accounts_config(),
        Request::AuthorizeAccountConfig {
            account,
            reauthorize,
        } => accounts::authorize_account(account.clone(), *reauthorize).await,
        Request::UpsertAccountConfig { account } => {
            accounts::upsert_account(state, account.clone()).await
        }
        Request::SetDefaultAccount { key } => accounts::set_default_account_key(state, key).await,
        Request::TestAccountConfig { account } => {
            accounts::test_account(state, account.clone()).await
        }
        Request::GetRule { rule } => rules::get_rule(state, rule).await,
        Request::GetRuleForm { rule } => rules::get_rule_form(state, rule).await,
        Request::UpsertRule { rule } => rules::upsert_rule_value(state, rule.clone()).await,
        Request::DeleteRule { rule } => rules::delete_rule(state, rule).await,
        Request::UpsertRuleForm {
            existing_rule,
            name,
            condition,
            action,
            priority,
            enabled,
        } => {
            rules::upsert_rule_form(
                state,
                existing_rule.as_ref(),
                name,
                condition,
                action,
                *priority,
                *enabled,
            )
            .await
        }
        Request::ListRuleHistory { rule, limit } => {
            rules::list_rule_history(state, rule.as_ref(), *limit).await
        }
        Request::DryRunRules { rule, all, after } => {
            rules::dry_run(state, rule.as_ref(), *all, after.as_ref()).await
        }
        Request::ListEvents {
            limit,
            level,
            category,
        } => diagnostics::list_events(state, *limit, level.as_deref(), category.as_deref()).await,
        Request::GetLogs { limit, level } => diagnostics::get_logs(*limit, level.as_deref()),
        Request::GetDoctorReport => diagnostics::doctor_report(state).await,
        Request::GenerateBugReport {
            verbose,
            full_logs,
            since,
        } => diagnostics::bug_report(*verbose, *full_logs, since.clone()).await,
        Request::Search {
            query,
            limit,
            offset,
            mode,
            sort,
            explain,
        } => {
            diagnostics::search(
                state,
                query,
                *limit,
                *offset,
                mode.unwrap_or(state.config_snapshot().search.default_mode),
                sort.clone()
                    .unwrap_or(crate::mxr_core::types::SortOrder::DateDesc),
                *explain,
            )
            .await
        }
        Request::Count { query, mode } => {
            diagnostics::count(
                state,
                query,
                mode.unwrap_or(state.config_snapshot().search.default_mode),
            )
            .await
        }
        Request::GetHeaders { message_id } => diagnostics::get_headers(state, message_id).await,
        Request::ListSavedSearches => diagnostics::list_saved_searches(state).await,
        Request::ListSubscriptions { account_id, limit } => {
            diagnostics::list_subscriptions(state, account_id.as_ref(), *limit).await
        }
        Request::GetSemanticStatus => diagnostics::semantic_status(state).await,
        Request::EnableSemantic { enabled } => diagnostics::enable_semantic(state, *enabled).await,
        Request::InstallSemanticProfile { profile } => {
            diagnostics::install_semantic_profile(state, *profile).await
        }
        Request::UseSemanticProfile { profile } => {
            diagnostics::use_semantic_profile(state, *profile).await
        }
        Request::ReindexSemantic => diagnostics::reindex_semantic(state).await,
        Request::CreateSavedSearch {
            name,
            query,
            search_mode,
        } => diagnostics::create_saved_search(state, name, query, *search_mode).await,
        Request::DeleteSavedSearch { name } => diagnostics::delete_saved_search(state, name).await,
        Request::RunSavedSearch { name, limit } => {
            diagnostics::run_saved_search(state, name, *limit).await
        }
        Request::GetStatus => diagnostics::get_status(state).await,
        Request::SyncNow { account_id } => diagnostics::sync_now(state, account_id.as_ref()).await,
        Request::ExportThread { thread_id, format } => {
            diagnostics::export_thread(state, thread_id, format).await
        }
        Request::ExportSearch { query, format } => {
            diagnostics::export_search(state, query, format).await
        }
        Request::Ping => Ok(ResponseData::Pong),
        Request::Shutdown => std::process::exit(0),
        Request::Mutation(cmd) => mutations::mutation(state, cmd).await,
        Request::Snooze {
            message_id,
            wake_at,
        } => mutations::snooze(state, message_id, wake_at).await,
        Request::Unsnooze { message_id } => mutations::unsnooze(state, message_id).await,
        Request::ListSnoozed => mutations::list_snoozed(state).await,
        Request::ListDrafts => mutations::list_drafts(state).await,
        Request::PrepareReply {
            message_id,
            reply_all,
        } => mutations::prepare_reply(state, message_id, *reply_all).await,
        Request::PrepareForward { message_id } => {
            mutations::prepare_forward(state, message_id).await
        }
        Request::SendDraft { draft } => mutations::send_draft(state, draft).await,
        Request::SaveDraftToServer { draft } => mutations::save_draft_to_server(state, draft).await,
        Request::Unsubscribe { message_id } => mutations::unsubscribe(state, message_id).await,
        Request::SetFlags { message_id, flags } => {
            mutations::set_flags(state, message_id, *flags).await
        }
        Request::GetSyncStatus { account_id } => {
            diagnostics::get_sync_status(state, account_id).await
        }
    };

    match result {
        Ok(data) => Response::Ok { data },
        Err(message) => Response::Error { message },
    }
}
fn build_reply_references(envelope: &crate::mxr_core::types::Envelope) -> Vec<String> {
    let mut references = envelope.references.clone();
    if let Some(message_id) = &envelope.message_id_header {
        if !references.iter().any(|reference| reference == message_id) {
            references.push(message_id.clone());
        }
    }
    references
}

/// Build an ExportThread from a thread_id by fetching envelopes and bodies from the store.
async fn build_export_thread(
    state: &Arc<AppState>,
    thread_id: &crate::mxr_core::ThreadId,
) -> Result<ExportThread, String> {
    let thread = state
        .store
        .get_thread(thread_id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Thread not found: {}", thread_id))?;

    let envelopes = state
        .store
        .get_thread_envelopes(thread_id)
        .await
        .map_err(|e| e.to_string())?;

    let mut messages = Vec::with_capacity(envelopes.len());
    for env in &envelopes {
        let body = state
            .store
            .get_body(&env.id)
            .await
            .map_err(|e| e.to_string())?;

        messages.push(ExportMessage {
            id: env.id.to_string(),
            from_name: env.from.name.clone(),
            from_email: env.from.email.clone(),
            to: env.to.iter().map(|a| a.email.clone()).collect(),
            date: env.date,
            subject: env.subject.clone(),
            body_text: body.as_ref().and_then(|b| b.text_plain.clone()),
            body_html: body.as_ref().and_then(|b| b.text_html.clone()),
            headers_raw: body.as_ref().and_then(|b| b.metadata.raw_headers.clone()),
            attachments: body
                .as_ref()
                .map(|b| {
                    b.attachments
                        .iter()
                        .map(|a| ExportAttachment {
                            filename: a.filename.clone(),
                            size_bytes: a.size_bytes,
                            local_path: a.local_path.as_ref().map(|p| p.display().to_string()),
                        })
                        .collect()
                })
                .unwrap_or_default(),
        });
    }

    Ok(ExportThread {
        thread_id: thread_id.to_string(),
        subject: thread.subject,
        messages,
    })
}

async fn find_label_by_name(
    state: &Arc<AppState>,
    account_id: &crate::mxr_core::AccountId,
    name: &str,
) -> Result<crate::mxr_core::Label, String> {
    let labels = state
        .store
        .list_labels_by_account(account_id)
        .await
        .map_err(|e| e.to_string())?;
    labels
        .into_iter()
        .find(|label| label.name == name)
        .ok_or_else(|| format!("Label not found: {name}"))
}

fn render_message_context(body: &crate::mxr_core::types::MessageBody) -> String {
    crate::mxr_reader::clean(
        body.text_plain.as_deref(),
        body.text_html.as_deref(),
        &ReaderConfig::default(),
    )
    .content
}

async fn populate_envelope_label_provider_ids(
    state: &Arc<AppState>,
    envelope: &mut crate::mxr_core::types::Envelope,
    labels: &[crate::mxr_core::types::Label],
) -> Result<(), String> {
    let label_ids = state
        .store
        .get_message_label_ids(&envelope.id)
        .await
        .map_err(|e| e.to_string())?;
    envelope.label_provider_ids = labels
        .iter()
        .filter(|label| label_ids.iter().any(|id| id == &label.id))
        .map(|label| label.provider_id.clone())
        .collect();
    Ok(())
}

async fn persist_local_label_changes(
    state: &Arc<AppState>,
    message_id: &crate::mxr_core::MessageId,
    add: &[String],
    remove: &[String],
) -> Result<(), sqlx::Error> {
    let envelope = state
        .store
        .get_envelope(message_id)
        .await?
        .ok_or(sqlx::Error::RowNotFound)?;
    let labels = state
        .store
        .list_labels_by_account(&envelope.account_id)
        .await?;
    let mut label_ids = state.store.get_message_label_ids(message_id).await?;

    for label_ref in remove {
        if let Some(label) = labels
            .iter()
            .find(|candidate| candidate.provider_id == *label_ref || candidate.name == *label_ref)
        {
            label_ids.retain(|id| id != &label.id);
        }
    }

    for label_ref in add {
        if let Some(label) = labels
            .iter()
            .find(|candidate| candidate.provider_id == *label_ref || candidate.name == *label_ref)
        {
            if !label_ids.iter().any(|id| id == &label.id) {
                label_ids.push(label.id.clone());
            }
        }
    }

    state
        .store
        .set_message_labels(message_id, &label_ids)
        .await?;
    state
        .store
        .recalculate_label_counts(&envelope.account_id)
        .await?;
    Ok(())
}

pub(crate) async fn apply_snooze(
    state: &Arc<AppState>,
    message_id: &crate::mxr_core::MessageId,
    wake_at: &chrono::DateTime<chrono::Utc>,
) -> Result<(), String> {
    let envelope = state
        .store
        .get_envelope(message_id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Message not found: {message_id}"))?;
    let provider_id = state
        .store
        .get_provider_id(message_id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Missing provider id for message: {message_id}"))?;
    let original_labels = state
        .store
        .get_message_label_ids(message_id)
        .await
        .map_err(|e| e.to_string())?;
    state
        .get_provider(Some(&envelope.account_id))?
        .modify_labels(&provider_id, &[], &["INBOX".to_string()])
        .await
        .map_err(|e| e.to_string())?;
    persist_local_label_changes(state, message_id, &[], &["INBOX".to_string()])
        .await
        .map_err(|e| e.to_string())?;
    state
        .store
        .insert_snooze(&Snoozed {
            message_id: message_id.clone(),
            account_id: envelope.account_id,
            snoozed_at: chrono::Utc::now(),
            wake_at: *wake_at,
            original_labels,
        })
        .await
        .map_err(|e| e.to_string())
}

pub(crate) async fn restore_snoozed_message(
    state: &Arc<AppState>,
    snoozed: &Snoozed,
) -> Result<(), String> {
    let provider_id = state
        .store
        .get_provider_id(&snoozed.message_id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Missing provider id for message: {}", snoozed.message_id))?;
    let labels = state
        .store
        .list_labels_by_account(&snoozed.account_id)
        .await
        .map_err(|e| e.to_string())?;
    let restore_provider_ids: Vec<String> = labels
        .iter()
        .filter(|label| snoozed.original_labels.iter().any(|id| id == &label.id))
        .map(|label| label.provider_id.clone())
        .collect();

    state
        .get_provider(Some(&snoozed.account_id))?
        .modify_labels(&provider_id, &restore_provider_ids, &[])
        .await
        .map_err(|e| e.to_string())?;
    state
        .store
        .set_message_labels(&snoozed.message_id, &snoozed.original_labels)
        .await
        .map_err(|e| e.to_string())?;
    state
        .store
        .recalculate_label_counts(&snoozed.account_id)
        .await
        .map_err(|e| e.to_string())?;
    state
        .store
        .remove_snooze(&snoozed.message_id)
        .await
        .map_err(|e| e.to_string())
}

fn parse_rule_value(value: serde_json::Value) -> Result<Rule, String> {
    serde_json::from_value(value).map_err(|e| e.to_string())
}

async fn build_rule_from_form(
    state: &Arc<AppState>,
    existing_rule: Option<&String>,
    name: &str,
    condition: &str,
    action: &str,
    priority: i32,
    enabled: bool,
) -> Result<Rule, String> {
    let existing = if let Some(rule) = existing_rule {
        state
            .store
            .get_rule_by_id_or_name(rule)
            .await
            .map_err(|e| e.to_string())?
            .map(|row| {
                serde_json::from_value::<Rule>(crate::mxr_store::row_to_rule_json(&row))
                    .map_err(|e| e.to_string())
            })
            .transpose()?
    } else {
        None
    };

    let now = chrono::Utc::now();
    Ok(Rule {
        id: existing
            .as_ref()
            .map(|rule| rule.id.clone())
            .unwrap_or_default(),
        name: name.to_string(),
        enabled,
        priority,
        conditions: parse_rule_condition_string(condition)?,
        actions: vec![parse_rule_action_string(action)?],
        created_at: existing.as_ref().map(|rule| rule.created_at).unwrap_or(now),
        updated_at: now,
    })
}

fn parse_rule_condition_string(input: &str) -> Result<Conditions, String> {
    let ast = parse_query(input).map_err(|e| e.to_string())?;
    query_ast_to_conditions(ast)
}

fn query_ast_to_conditions(node: crate::mxr_search::ast::QueryNode) -> Result<Conditions, String> {
    use crate::mxr_search::ast::{DateBound, DateValue, FilterKind, QueryField, QueryNode, SizeOp};

    Ok(match node {
        QueryNode::And(left, right) => Conditions::And {
            conditions: vec![
                query_ast_to_conditions(*left)?,
                query_ast_to_conditions(*right)?,
            ],
        },
        QueryNode::Or(left, right) => Conditions::Or {
            conditions: vec![
                query_ast_to_conditions(*left)?,
                query_ast_to_conditions(*right)?,
            ],
        },
        QueryNode::Not(node) => Conditions::Not {
            condition: Box::new(query_ast_to_conditions(*node)?),
        },
        QueryNode::Field { field, value } => Conditions::Field(match field {
            QueryField::From => FieldCondition::From {
                pattern: StringMatch::Contains(value),
            },
            QueryField::To => FieldCondition::To {
                pattern: StringMatch::Contains(value),
            },
            QueryField::Subject => FieldCondition::Subject {
                pattern: StringMatch::Contains(value),
            },
            QueryField::Body => FieldCondition::BodyContains {
                pattern: StringMatch::Contains(value),
            },
            QueryField::Cc | QueryField::Bcc | QueryField::Filename => {
                return Err("field is not supported in rules form".to_string())
            }
        }),
        QueryNode::Label(label) => Conditions::Field(FieldCondition::HasLabel { label }),
        QueryNode::Filter(FilterKind::Unread) => Conditions::Field(FieldCondition::IsUnread),
        QueryNode::Filter(FilterKind::Starred) => Conditions::Field(FieldCondition::IsStarred),
        QueryNode::Filter(FilterKind::HasAttachment) => {
            Conditions::Field(FieldCondition::HasAttachment)
        }
        QueryNode::Filter(FilterKind::Read) => Conditions::Not {
            condition: Box::new(Conditions::Field(FieldCondition::IsUnread)),
        },
        QueryNode::Filter(FilterKind::Draft) => Conditions::Field(FieldCondition::HasLabel {
            label: "DRAFT".to_string(),
        }),
        QueryNode::Filter(FilterKind::Sent) => Conditions::Field(FieldCondition::HasLabel {
            label: "SENT".to_string(),
        }),
        QueryNode::Filter(FilterKind::Trash) => Conditions::Field(FieldCondition::HasLabel {
            label: "TRASH".to_string(),
        }),
        QueryNode::Filter(FilterKind::Spam) => Conditions::Field(FieldCondition::HasLabel {
            label: "SPAM".to_string(),
        }),
        QueryNode::Filter(FilterKind::Inbox) => Conditions::Field(FieldCondition::HasLabel {
            label: "INBOX".to_string(),
        }),
        QueryNode::Filter(FilterKind::Archived) => Conditions::Field(FieldCondition::HasLabel {
            label: "ARCHIVE".to_string(),
        }),
        QueryNode::Filter(FilterKind::Answered) => {
            return Err("is:answered is not supported in rules form".to_string())
        }
        QueryNode::Text(value) | QueryNode::Phrase(value) => {
            Conditions::Field(FieldCondition::BodyContains {
                pattern: StringMatch::Contains(value),
            })
        }
        QueryNode::DateRange { bound, date } => {
            let date = match date {
                DateValue::Specific(date) => {
                    chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(
                        date.and_hms_opt(0, 0, 0)
                            .ok_or_else(|| "invalid date".to_string())?,
                        chrono::Utc,
                    )
                }
                _ => return Err("relative dates are not supported in rules form".to_string()),
            };
            match bound {
                DateBound::After => Conditions::Field(FieldCondition::DateAfter { date }),
                DateBound::Before => Conditions::Field(FieldCondition::DateBefore { date }),
                DateBound::Exact => Conditions::And {
                    conditions: vec![
                        Conditions::Field(FieldCondition::DateAfter { date }),
                        Conditions::Field(FieldCondition::DateBefore {
                            date: date + chrono::Duration::days(1),
                        }),
                    ],
                },
            }
        }
        QueryNode::Size { op, bytes } => match op {
            SizeOp::GreaterThan => Conditions::Field(FieldCondition::SizeGreaterThan { bytes }),
            SizeOp::GreaterThanOrEqual => Conditions::Field(FieldCondition::SizeGreaterThan {
                bytes: bytes.saturating_sub(1),
            }),
            SizeOp::LessThan => Conditions::Field(FieldCondition::SizeLessThan { bytes }),
            SizeOp::LessThanOrEqual => Conditions::Field(FieldCondition::SizeLessThan {
                bytes: bytes.saturating_add(1),
            }),
            SizeOp::Equal => Conditions::And {
                conditions: vec![
                    Conditions::Field(FieldCondition::SizeGreaterThan {
                        bytes: bytes.saturating_sub(1),
                    }),
                    Conditions::Field(FieldCondition::SizeLessThan {
                        bytes: bytes.saturating_add(1),
                    }),
                ],
            },
        },
    })
}

fn parse_rule_action_string(value: &str) -> Result<RuleAction, String> {
    let lower = value.to_ascii_lowercase();
    if lower == "archive" {
        return Ok(RuleAction::Archive);
    }
    if lower == "trash" {
        return Ok(RuleAction::Trash);
    }
    if lower == "star" {
        return Ok(RuleAction::Star);
    }
    if lower == "mark-read" {
        return Ok(RuleAction::MarkRead);
    }
    if lower == "mark-unread" {
        return Ok(RuleAction::MarkUnread);
    }
    if let Some(label) = value.strip_prefix("add-label:") {
        return Ok(RuleAction::AddLabel {
            label: label.to_string(),
        });
    }
    if let Some(label) = value.strip_prefix("remove-label:") {
        return Ok(RuleAction::RemoveLabel {
            label: label.to_string(),
        });
    }
    if let Some(command) = value.strip_prefix("shell:") {
        return Ok(RuleAction::ShellHook {
            command: command.to_string(),
        });
    }
    Err(format!("Unsupported action: {value}"))
}

fn rule_to_form_data(rule: &Rule) -> Result<crate::mxr_protocol::RuleFormData, String> {
    let action = rule
        .actions
        .first()
        .ok_or_else(|| "rule has no actions".to_string())
        .and_then(rule_action_to_string)?;
    Ok(crate::mxr_protocol::RuleFormData {
        id: Some(rule.id.to_string()),
        name: rule.name.clone(),
        condition: conditions_to_query(&rule.conditions)?,
        action,
        priority: rule.priority,
        enabled: rule.enabled,
    })
}

fn rule_action_to_string(action: &RuleAction) -> Result<String, String> {
    match action {
        RuleAction::Archive => Ok("archive".to_string()),
        RuleAction::Trash => Ok("trash".to_string()),
        RuleAction::Star => Ok("star".to_string()),
        RuleAction::MarkRead => Ok("mark-read".to_string()),
        RuleAction::MarkUnread => Ok("mark-unread".to_string()),
        RuleAction::AddLabel { label } => Ok(format!("add-label:{label}")),
        RuleAction::RemoveLabel { label } => Ok(format!("remove-label:{label}")),
        RuleAction::ShellHook { command } => Ok(format!("shell:{command}")),
        RuleAction::Snooze { .. } => {
            Err("snooze rules are not editable in the TUI yet".to_string())
        }
    }
}

fn conditions_to_query(conditions: &Conditions) -> Result<String, String> {
    match conditions {
        Conditions::And { conditions } => {
            let parts = conditions
                .iter()
                .map(conditions_to_query)
                .collect::<Result<Vec<_>, _>>()?;
            Ok(parts
                .into_iter()
                .map(|part| format!("({part})"))
                .collect::<Vec<_>>()
                .join(" AND "))
        }
        Conditions::Or { conditions } => {
            let parts = conditions
                .iter()
                .map(conditions_to_query)
                .collect::<Result<Vec<_>, _>>()?;
            Ok(parts
                .into_iter()
                .map(|part| format!("({part})"))
                .collect::<Vec<_>>()
                .join(" OR "))
        }
        Conditions::Not { condition } => Ok(format!("NOT ({})", conditions_to_query(condition)?)),
        Conditions::Field(field) => field_condition_to_query(field),
    }
}

fn field_condition_to_query(field: &FieldCondition) -> Result<String, String> {
    match field {
        FieldCondition::From { pattern } => string_match_to_query("from", pattern),
        FieldCondition::To { pattern } => string_match_to_query("to", pattern),
        FieldCondition::Subject { pattern } => string_match_to_query("subject", pattern),
        FieldCondition::HasLabel { label } => Ok(format!("label:{label}")),
        FieldCondition::HasAttachment => Ok("has:attachment".to_string()),
        FieldCondition::DateAfter { date } => Ok(format!("after:{}", date.format("%Y-%m-%d"))),
        FieldCondition::DateBefore { date } => Ok(format!("before:{}", date.format("%Y-%m-%d"))),
        FieldCondition::IsUnread => Ok("is:unread".to_string()),
        FieldCondition::IsStarred => Ok("is:starred".to_string()),
        FieldCondition::BodyContains { pattern } => string_match_to_query("", pattern),
        FieldCondition::SizeGreaterThan { .. }
        | FieldCondition::SizeLessThan { .. }
        | FieldCondition::HasUnsubscribe => {
            Err("condition not editable in the TUI yet".to_string())
        }
    }
}

fn string_match_to_query(field: &str, pattern: &StringMatch) -> Result<String, String> {
    let value = match pattern {
        StringMatch::Contains(value) | StringMatch::Exact(value) => value.clone(),
        StringMatch::Regex(_) | StringMatch::Glob(_) => {
            return Err("regex/glob rules are not editable in the TUI yet".to_string())
        }
    };
    if field.is_empty() {
        Ok(value)
    } else {
        Ok(format!("{field}:{value}"))
    }
}

fn protocol_event_entry(
    entry: crate::mxr_store::EventLogEntry,
) -> crate::mxr_protocol::EventLogEntry {
    crate::mxr_protocol::EventLogEntry {
        timestamp: entry.timestamp,
        level: entry.level,
        category: entry.category,
        account_id: entry.account_id,
        message_id: entry.message_id,
        rule_id: entry.rule_id,
        summary: entry.summary,
        details: entry.details,
    }
}

fn recent_log_lines(limit: usize, level: Option<&str>) -> Result<Vec<String>, std::io::Error> {
    let log_path = crate::mxr_config::data_dir().join("logs").join("mxr.log");
    if !log_path.exists() {
        return Ok(vec!["(no recent logs)".to_string()]);
    }

    let file = std::fs::File::open(log_path)?;
    let mut lines = BufReader::new(file)
        .lines()
        .collect::<Result<Vec<_>, _>>()?;
    if let Some(level) = level {
        let level = level.to_ascii_lowercase();
        lines.retain(|line| line.to_ascii_lowercase().contains(&level));
    }
    if lines.is_empty() {
        return Ok(vec!["(no recent logs)".to_string()]);
    }
    let start = lines.len().saturating_sub(limit.max(1));
    Ok(lines.split_off(start))
}

fn should_fallback_to_tantivy(query: &str, error: &crate::mxr_search::ParseError) -> bool {
    if looks_structured_query(query) {
        return false;
    }

    matches!(
        error,
        crate::mxr_search::ParseError::UnexpectedToken(_)
            | crate::mxr_search::ParseError::UnexpectedEnd
            | crate::mxr_search::ParseError::UnmatchedParen
    )
}

fn looks_structured_query(query: &str) -> bool {
    let trimmed = query.trim();
    trimmed.contains(':')
        || trimmed.contains('(')
        || trimmed.contains(')')
        || trimmed.starts_with('-')
        || trimmed.contains(" AND ")
        || trimmed.contains(" OR ")
        || trimmed.contains(" NOT ")
}

async fn collect_status_snapshot(
    state: &Arc<AppState>,
) -> Result<(Vec<String>, u32, Vec<AccountSyncStatus>), String> {
    let accounts = state
        .store
        .list_accounts()
        .await
        .map_err(|e| e.to_string())?;
    let mut names = Vec::new();
    let mut total_messages = 0;
    let mut sync_statuses = Vec::new();

    for account in accounts {
        names.push(account.name.clone());
        total_messages += state
            .store
            .count_messages_by_account(&account.id)
            .await
            .map_err(|e| e.to_string())?;
        sync_statuses.push(build_account_sync_status(state, &account.id).await?);
    }

    if names.is_empty() {
        names.push("unknown".to_string());
    }

    Ok((names, total_messages, sync_statuses))
}

async fn build_account_sync_status(
    state: &Arc<AppState>,
    account_id: &crate::mxr_core::AccountId,
) -> Result<AccountSyncStatus, String> {
    let account = state
        .store
        .get_account(account_id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Account not found: {account_id}"))?;
    let runtime = state
        .store
        .get_sync_runtime_status(account_id)
        .await
        .map_err(|e| e.to_string())?;
    let cursor = state
        .store
        .get_sync_cursor(account_id)
        .await
        .map_err(|e| e.to_string())?;

    let last_attempt_at = runtime
        .as_ref()
        .and_then(|row| row.last_attempt_at)
        .map(|dt| dt.to_rfc3339());
    let last_success_at = runtime
        .as_ref()
        .and_then(|row| row.last_success_at)
        .map(|dt| dt.to_rfc3339());
    let last_error = runtime.as_ref().and_then(|row| row.last_error.clone());
    let backoff_until = runtime
        .as_ref()
        .and_then(|row| row.backoff_until)
        .map(|dt| dt.to_rfc3339());
    let sync_in_progress = runtime
        .as_ref()
        .map(|row| row.sync_in_progress)
        .unwrap_or(false);
    let consecutive_failures = runtime
        .as_ref()
        .map(|row| row.consecutive_failures)
        .unwrap_or(0);
    let healthy = !sync_in_progress
        && last_error.is_none()
        && backoff_until.is_none()
        && last_success_at.is_some();

    Ok(AccountSyncStatus {
        account_id: account.id,
        account_name: account.name,
        last_attempt_at,
        last_success_at,
        last_error,
        failure_class: runtime.as_ref().and_then(|row| row.failure_class.clone()),
        consecutive_failures,
        backoff_until,
        sync_in_progress,
        current_cursor_summary: Some(
            runtime
                .as_ref()
                .and_then(|row| row.current_cursor_summary.clone())
                .unwrap_or_else(|| describe_cursor_for_status(cursor.as_ref())),
        ),
        last_synced_count: runtime
            .as_ref()
            .map(|row| row.last_synced_count)
            .unwrap_or(0),
        healthy,
    })
}

fn describe_cursor_for_status(cursor: Option<&crate::mxr_core::types::SyncCursor>) -> String {
    match cursor {
        Some(crate::mxr_core::types::SyncCursor::Initial) | None => "initial".to_string(),
        Some(crate::mxr_core::types::SyncCursor::Gmail { history_id }) => {
            format!("gmail history_id={history_id}")
        }
        Some(crate::mxr_core::types::SyncCursor::GmailBackfill {
            history_id,
            page_token,
        }) => {
            let short: String = page_token.chars().take(24).collect();
            if page_token.chars().count() > 24 {
                format!("gmail_backfill history_id={history_id} page_token={short}...")
            } else {
                format!("gmail_backfill history_id={history_id} page_token={short}")
            }
        }
        Some(crate::mxr_core::types::SyncCursor::Imap {
            uid_validity,
            uid_next,
            mailboxes,
            ..
        }) => format!(
            "imap uid_validity={uid_validity} uid_next={uid_next} mailboxes={}",
            mailboxes.len()
        ),
    }
}

async fn collect_doctor_report(
    state: &Arc<AppState>,
) -> Result<crate::mxr_protocol::DoctorReport, String> {
    let data_dir = crate::mxr_config::data_dir();
    let db_path = data_dir.join("mxr.db");
    let index_path = data_dir.join("search_index");
    let log_path = data_dir.join("logs").join("mxr.log");
    let socket_path = crate::state::AppState::socket_path();

    let data_dir_exists = data_dir.exists();
    let database_exists = db_path.exists();
    let index_exists = index_path.exists();
    let socket_exists = socket_path.exists();
    let (_, total_messages, sync_statuses) = collect_status_snapshot(state).await?;
    let data_stats = doctor_data_stats(
        state
            .store
            .collect_record_counts()
            .await
            .map_err(|e| e.to_string())?,
    );
    let repair_required = crate::server::search_requires_repair(state, total_messages).await;
    let restart_required = false;
    let lexical_index_freshness =
        lexical_index_freshness(index_exists, repair_required, restart_required);
    let last_successful_sync_at = latest_successful_sync_at(&sync_statuses);
    let lexical_last_rebuilt_at = state
        .store
        .latest_event_timestamp("search", Some("Lexical index rebuilt"))
        .await
        .map_err(|e| e.to_string())?
        .map(|value| value.to_rfc3339());
    let semantic_config = state.config_snapshot().search.semantic.clone();
    let semantic_snapshot = state.semantic.lock().await.status_snapshot().await.ok();
    let (
        semantic_enabled,
        semantic_active_profile,
        semantic_index_freshness,
        semantic_last_indexed_at,
    ) = semantic_freshness_from_snapshot(
        semantic_snapshot.as_ref(),
        semantic_config.enabled,
        semantic_config.active_profile.as_str(),
    );
    let health_class =
        crate::server::classify_health(&sync_statuses, repair_required, restart_required);
    let recent_sync_events = state
        .store
        .list_events(10, None, Some("sync"))
        .await
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(protocol_event_entry)
        .collect();
    let recent_error_logs = recent_log_lines(10, Some("error")).unwrap_or_default();
    let recommended_next_steps = if matches!(
        health_class,
        crate::mxr_protocol::DaemonHealthClass::Healthy
    ) {
        vec!["mxr status".to_string()]
    } else {
        vec![
            "mxr status".to_string(),
            "mxr sync --status".to_string(),
            "mxr logs --level error".to_string(),
            "mxr daemon --foreground".to_string(),
        ]
    };
    let healthy = data_dir_exists
        && database_exists
        && index_exists
        && socket_exists
        && matches!(
            health_class,
            crate::mxr_protocol::DaemonHealthClass::Healthy
        );

    Ok(crate::mxr_protocol::DoctorReport {
        healthy,
        health_class,
        lexical_index_freshness,
        last_successful_sync_at,
        lexical_last_rebuilt_at,
        semantic_enabled,
        semantic_active_profile,
        semantic_index_freshness,
        semantic_last_indexed_at,
        data_stats,
        data_dir_exists,
        database_exists,
        index_exists,
        socket_exists,
        socket_reachable: true,
        stale_socket: false,
        daemon_running: true,
        daemon_pid: Some(std::process::id()),
        daemon_protocol_version: crate::mxr_protocol::IPC_PROTOCOL_VERSION,
        daemon_version: Some(crate::server::current_daemon_version().to_string()),
        daemon_build_id: Some(crate::server::current_build_id()),
        index_lock_held: false,
        index_lock_error: None,
        restart_required,
        repair_required,
        database_path: db_path.display().to_string(),
        database_size_bytes: file_size(&db_path),
        index_path: index_path.display().to_string(),
        index_size_bytes: dir_size(&index_path),
        log_path: log_path.display().to_string(),
        log_size_bytes: file_size(&log_path),
        sync_statuses,
        recent_sync_events,
        recent_error_logs,
        recommended_next_steps,
    })
}

fn file_size(path: &std::path::Path) -> u64 {
    std::fs::metadata(path).map(|meta| meta.len()).unwrap_or(0)
}

fn latest_successful_sync_at(
    sync_statuses: &[crate::mxr_protocol::AccountSyncStatus],
) -> Option<String> {
    sync_statuses
        .iter()
        .filter_map(|status| status.last_success_at.as_deref())
        .filter_map(|value| chrono::DateTime::parse_from_rfc3339(value).ok())
        .max()
        .map(|value| value.to_rfc3339())
}

fn lexical_index_freshness(
    index_exists: bool,
    repair_required: bool,
    restart_required: bool,
) -> crate::mxr_protocol::IndexFreshness {
    if repair_required || !index_exists {
        crate::mxr_protocol::IndexFreshness::RepairRequired
    } else if restart_required {
        crate::mxr_protocol::IndexFreshness::Stale
    } else {
        crate::mxr_protocol::IndexFreshness::Current
    }
}

fn semantic_freshness_from_snapshot(
    snapshot: Option<&crate::mxr_core::types::SemanticStatusSnapshot>,
    enabled_fallback: bool,
    active_profile_fallback: &str,
) -> (
    bool,
    Option<String>,
    crate::mxr_protocol::IndexFreshness,
    Option<String>,
) {
    let Some(snapshot) = snapshot else {
        return if enabled_fallback {
            (
                true,
                Some(active_profile_fallback.to_string()),
                crate::mxr_protocol::IndexFreshness::Unknown,
                None,
            )
        } else {
            (
                false,
                None,
                crate::mxr_protocol::IndexFreshness::Disabled,
                None,
            )
        };
    };

    if !snapshot.enabled {
        return (
            false,
            None,
            crate::mxr_protocol::IndexFreshness::Disabled,
            None,
        );
    }

    let active_profile = snapshot.active_profile.as_str().to_string();
    let active_record = snapshot
        .profiles
        .iter()
        .find(|profile| profile.profile == snapshot.active_profile);
    let freshness = match active_record.map(|profile| profile.status) {
        Some(crate::mxr_core::types::SemanticProfileStatus::Ready) => {
            crate::mxr_protocol::IndexFreshness::Current
        }
        Some(crate::mxr_core::types::SemanticProfileStatus::Indexing)
        | Some(crate::mxr_core::types::SemanticProfileStatus::Pending) => {
            crate::mxr_protocol::IndexFreshness::Indexing
        }
        Some(crate::mxr_core::types::SemanticProfileStatus::Error) => {
            crate::mxr_protocol::IndexFreshness::Error
        }
        None => crate::mxr_protocol::IndexFreshness::Stale,
    };

    (
        true,
        Some(active_profile),
        freshness,
        active_record
            .and_then(|profile| profile.last_indexed_at)
            .map(|value| value.to_rfc3339()),
    )
}

fn doctor_data_stats(
    counts: crate::mxr_store::StoreRecordCounts,
) -> crate::mxr_protocol::DoctorDataStats {
    crate::mxr_protocol::DoctorDataStats {
        accounts: counts.accounts,
        labels: counts.labels,
        messages: counts.messages,
        unread_messages: counts.unread_messages,
        starred_messages: counts.starred_messages,
        messages_with_attachments: counts.messages_with_attachments,
        message_labels: counts.message_labels,
        bodies: counts.bodies,
        attachments: counts.attachments,
        drafts: counts.drafts,
        snoozed: counts.snoozed,
        saved_searches: counts.saved_searches,
        rules: counts.rules,
        rule_logs: counts.rule_logs,
        sync_log: counts.sync_log,
        sync_runtime_statuses: counts.sync_runtime_statuses,
        event_log: counts.event_log,
        semantic_profiles: counts.semantic_profiles,
        semantic_chunks: counts.semantic_chunks,
        semantic_embeddings: counts.semantic_embeddings,
    }
}

fn dir_size(path: &std::path::Path) -> u64 {
    if !path.exists() {
        return 0;
    }

    let Ok(entries) = std::fs::read_dir(path) else {
        return 0;
    };

    entries
        .filter_map(Result::ok)
        .map(|entry| {
            let path = entry.path();
            if path.is_dir() {
                dir_size(&path)
            } else {
                entry.metadata().map(|meta| meta.len()).unwrap_or(0)
            }
        })
        .sum()
}

async fn persist_rule(state: &Arc<AppState>, rule: &Rule) -> Result<(), String> {
    let conditions_json = serde_json::to_string(&rule.conditions).map_err(|e| e.to_string())?;
    let actions_json = serde_json::to_string(&rule.actions).map_err(|e| e.to_string())?;
    state
        .store
        .upsert_rule(crate::mxr_store::RuleRecordInput {
            id: &rule.id.0,
            name: &rule.name,
            enabled: rule.enabled,
            priority: rule.priority,
            conditions_json: &conditions_json,
            actions_json: &actions_json,
            created_at: rule.created_at,
            updated_at: rule.updated_at,
        })
        .await
        .map_err(|e| e.to_string())
}

fn row_to_rule(row: &sqlx::sqlite::SqliteRow) -> Result<Rule, String> {
    serde_json::from_value(crate::mxr_store::row_to_rule_json(row)).map_err(|e| e.to_string())
}

async fn list_runtime_accounts(state: &Arc<AppState>) -> Result<Vec<AccountSummaryData>, String> {
    use std::collections::BTreeMap;

    let config = state.config_snapshot();
    let default_config_key = config.general.default_account.clone();
    let runtime_ids = state.runtime_account_ids();
    let default_account_id = state.default_account_id_opt();
    let runtime_accounts = state
        .store
        .list_accounts()
        .await
        .map_err(|e| e.to_string())?;

    let mut accounts: BTreeMap<String, AccountSummaryData> = BTreeMap::new();

    for account in runtime_accounts
        .into_iter()
        .filter(|account| runtime_ids.iter().any(|id| id == &account.id))
    {
        let key = account
            .sync_backend
            .as_ref()
            .map(|backend| backend.config_key.clone())
            .or_else(|| {
                account
                    .send_backend
                    .as_ref()
                    .map(|backend| backend.config_key.clone())
            });
        let sync_kind = account
            .sync_backend
            .as_ref()
            .map(|backend| provider_kind_label(&backend.provider_kind).to_string());
        let send_kind = account
            .send_backend
            .as_ref()
            .map(|backend| provider_kind_label(&backend.provider_kind).to_string());
        let provider_kind = sync_kind
            .clone()
            .or_else(|| send_kind.clone())
            .unwrap_or_else(|| "unknown".to_string());
        let map_key = key.clone().unwrap_or_else(|| account.id.to_string());

        accounts.insert(
            map_key,
            AccountSummaryData {
                account_id: account.id.clone(),
                key,
                name: account.name,
                email: account.email,
                provider_kind,
                sync_kind,
                send_kind,
                enabled: account.enabled,
                is_default: default_account_id.as_ref() == Some(&account.id),
                source: AccountSourceData::Runtime,
                editable: AccountEditModeData::RuntimeOnly,
                sync: None,
                send: None,
            },
        );
    }

    for (key, account) in config.accounts {
        let account_id = config_account_id(&key, &account);
        let summary = accounts
            .entry(key.clone())
            .or_insert_with(|| AccountSummaryData {
                account_id: account_id.clone(),
                key: Some(key.clone()),
                name: account.name.clone(),
                email: account.email.clone(),
                provider_kind: account_primary_provider_kind(&account),
                sync_kind: account.sync.as_ref().map(config_sync_kind_label),
                send_kind: account.send.as_ref().map(config_send_kind_label),
                enabled: true,
                is_default: false,
                source: AccountSourceData::Config,
                editable: AccountEditModeData::Full,
                sync: None,
                send: None,
            });

        summary.account_id = account_id;
        summary.key = Some(key.clone());
        summary.name = account.name.clone();
        summary.email = account.email.clone();
        summary.provider_kind = account_primary_provider_kind(&account);
        summary.sync_kind = account.sync.as_ref().map(config_sync_kind_label);
        summary.send_kind = account.send.as_ref().map(config_send_kind_label);
        summary.sync = account.sync.clone().map(sync_config_to_data);
        summary.send = account.send.clone().map(send_config_to_data);
        summary.is_default = default_config_key.as_deref() == Some(key.as_str());
        summary.source = match summary.source {
            AccountSourceData::Runtime => AccountSourceData::Both,
            _ => AccountSourceData::Config,
        };
        summary.editable = AccountEditModeData::Full;
    }

    let mut accounts = accounts.into_values().collect::<Vec<_>>();
    accounts.sort_by(|left, right| {
        right
            .is_default
            .cmp(&left.is_default)
            .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
            .then_with(|| left.email.to_lowercase().cmp(&right.email.to_lowercase()))
    });
    Ok(accounts)
}

fn list_account_configs() -> Result<Vec<AccountConfigData>, String> {
    let config = crate::mxr_config::load_config().map_err(|e| e.to_string())?;
    let default_account = config.general.default_account.clone();
    let mut accounts = config
        .accounts
        .into_iter()
        .map(|(key, account)| AccountConfigData {
            is_default: default_account.as_deref() == Some(key.as_str()),
            key,
            name: account.name,
            email: account.email,
            sync: account.sync.map(sync_config_to_data),
            send: account.send.map(send_config_to_data),
        })
        .collect::<Vec<_>>();
    accounts.sort_by(|left, right| left.key.cmp(&right.key));
    Ok(accounts)
}

async fn upsert_account_config(
    state: &Arc<AppState>,
    account: AccountConfigData,
) -> AccountOperationResult {
    let save_result = (|| -> Result<String, String> {
        let mut config = crate::mxr_config::load_config().map_err(|e| e.to_string())?;
        persist_account_passwords(&account).map_err(|e| e.to_string())?;

        config.accounts.insert(
            account.key.clone(),
            crate::mxr_config::AccountConfig {
                name: account.name.clone(),
                email: account.email.clone(),
                sync: account.sync.clone().map(sync_data_to_config).transpose()?,
                send: account.send.clone().map(send_data_to_config).transpose()?,
            },
        );
        if account.is_default || config.general.default_account.is_none() {
            config.general.default_account = Some(account.key.clone());
        }
        crate::mxr_config::save_config(&config).map_err(|e| e.to_string())?;
        Ok(format!("Saved account '{}' to config.", account.key))
    })();

    match save_result {
        Ok(save_detail) => match state.reload_accounts_from_disk().await {
            Ok(()) => account_operation_result(
                true,
                format!("Saved account '{}' and reloaded runtime.", account.key),
                Some(account_step(
                    true,
                    format!("{save_detail} Runtime reloaded."),
                )),
                None,
                None,
                None,
            ),
            Err(error) => account_operation_result(
                false,
                format!(
                    "Saved account '{}' but failed to reload runtime.",
                    account.key
                ),
                Some(account_step(
                    false,
                    format!("{save_detail} Reload failed: {error}"),
                )),
                None,
                None,
                None,
            ),
        },
        Err(error) => account_operation_result(
            false,
            format!("Failed to save account '{}'.", account.key),
            Some(account_step(false, error)),
            None,
            None,
            None,
        ),
    }
}

async fn set_default_account(state: &Arc<AppState>, key: &str) -> Result<String, String> {
    let mut config = crate::mxr_config::load_config().map_err(|e| e.to_string())?;
    if !config.accounts.contains_key(key) {
        return Err(format!("Account '{}' cannot be set as default", key));
    }
    config.general.default_account = Some(key.to_string());
    crate::mxr_config::save_config(&config).map_err(|e| e.to_string())?;
    state.reload_accounts_from_disk().await?;
    Ok(format!("Default account set to '{}'.", key))
}

async fn authorize_account_config(
    account: AccountConfigData,
    reauthorize: bool,
) -> AccountOperationResult {
    // Outlook device-code flow — check sync first, fall back to send for send-only accounts
    let outlook_tenant = match &account.sync {
        Some(AccountSyncConfigData::OutlookPersonal { .. }) => {
            Some(crate::mxr_provider_outlook::OutlookTenant::Personal)
        }
        Some(AccountSyncConfigData::OutlookWork { .. }) => {
            Some(crate::mxr_provider_outlook::OutlookTenant::Work)
        }
        _ => match &account.send {
            Some(AccountSendConfigData::OutlookPersonal { .. }) => {
                Some(crate::mxr_provider_outlook::OutlookTenant::Personal)
            }
            Some(AccountSendConfigData::OutlookWork { .. }) => {
                Some(crate::mxr_provider_outlook::OutlookTenant::Work)
            }
            _ => None,
        },
    };
    if let Some(tenant) = outlook_tenant {
        let (client_id, token_ref) = match &account.sync {
            Some(
                AccountSyncConfigData::OutlookPersonal { client_id, token_ref }
                | AccountSyncConfigData::OutlookWork { client_id, token_ref },
            ) => (client_id.clone(), token_ref.clone()),
            _ => match &account.send {
                Some(AccountSendConfigData::OutlookPersonal { token_ref }
                | AccountSendConfigData::OutlookWork { token_ref }) => (None, token_ref.clone()),
                _ => unreachable!(),
            },
        };
        let cid = client_id
            .or_else(|| crate::mxr_provider_outlook::OutlookAuth::bundled_client_id().map(String::from))
            .unwrap_or_default();
        if cid.is_empty() {
            return account_operation_result(
                false,
                "Outlook authorization requires a client ID.".into(),
                None,
                Some(account_step(
                    false,
                    "No bundled client ID and none provided. Add client_id to account config.".into(),
                )),
                None,
                None,
            );
        }
        let auth = crate::mxr_provider_outlook::OutlookAuth::new(cid, token_ref, tenant);
        if !reauthorize {
            if let Ok(Some(_)) = auth.load_tokens() {
                return account_operation_result(
                    true,
                    "Outlook authorization ready.".into(),
                    None,
                    Some(account_step(true, "Existing OAuth token loaded.".into())),
                    None,
                    None,
                );
            }
        }
        let device_resp = match auth.start_device_flow().await {
            Ok(r) => r,
            Err(e) => {
                return account_operation_result(
                    false,
                    "Outlook authorization failed.".into(),
                    None,
                    Some(account_step(false, e.to_string())),
                    None,
                    None,
                );
            }
        };
        let _ = open::that(&device_resp.verification_uri);
        tracing::info!(
            user_code = %device_resp.user_code,
            url = %device_resp.verification_uri,
            "Outlook device code flow started — user must enter code in browser"
        );
        return match auth.poll_for_token(&device_resp.device_code, device_resp.interval).await {
            Ok(tokens) => {
                if let Err(e) = auth.save_tokens(&tokens) {
                    account_operation_result(
                        false,
                        "Outlook authorization failed.".into(),
                        None,
                        Some(account_step(false, format!("Token save failed: {e}"))),
                        None,
                        None,
                    )
                } else {
                    account_operation_result(
                        true,
                        "Outlook authorization complete.".into(),
                        None,
                        Some(account_step(true, "Token stored successfully.".into())),
                        None,
                        None,
                    )
                }
            }
            Err(e) => account_operation_result(
                false,
                "Outlook authorization failed.".into(),
                None,
                Some(account_step(false, e.to_string())),
                None,
                None,
            ),
        };
    }

    let Some(AccountSyncConfigData::Gmail {
        credential_source,
        client_id,
        client_secret,
        token_ref,
    }) = account.sync
    else {
        return account_operation_result(
            false,
            "Authorization is only available for Gmail and Outlook accounts.".into(),
            None,
            Some(account_step(
                false,
                "Selected account does not use Gmail or Outlook sync.".into(),
            )),
            None,
            None,
        );
    };

    let (client_id, client_secret) =
        match resolve_gmail_credentials(credential_source, client_id, client_secret) {
            Ok(creds) => creds,
            Err(error) => {
                return account_operation_result(
                    false,
                    "Gmail authorization unavailable.".into(),
                    None,
                    Some(account_step(false, error)),
                    None,
                    None,
                )
            }
        };

    let mut auth =
        crate::mxr_provider_gmail::auth::GmailAuth::new(client_id, client_secret, token_ref);
    let auth_result = if reauthorize {
        auth.interactive_auth().await
    } else {
        match auth.load_existing().await {
            Ok(()) => Ok(()),
            Err(_) => auth.interactive_auth().await,
        }
    };

    match auth_result {
        Ok(()) => account_operation_result(
            true,
            if reauthorize {
                "Gmail authorization refreshed.".into()
            } else {
                "Gmail authorization ready.".into()
            },
            None,
            Some(account_step(
                true,
                if reauthorize {
                    "Browser authorization completed and token stored.".into()
                } else {
                    "OAuth token is available for this Gmail account.".into()
                },
            )),
            None,
            None,
        ),
        Err(error) => account_operation_result(
            false,
            "Gmail authorization failed.".into(),
            None,
            Some(account_step(false, error.to_string())),
            None,
            None,
        ),
    }
}

async fn test_account_config(account: AccountConfigData) -> AccountOperationResult {
    if let Err(error) = persist_account_passwords(&account) {
        return account_operation_result(
            false,
            "Failed to persist account secrets before testing.".into(),
            None,
            Some(account_step(false, error.to_string())),
            None,
            None,
        );
    }

    let mut auth = None;
    let mut sync = None;
    let mut send = None;
    let mut ok = true;

    if let Some(sync_config) = account.sync.clone() {
        match sync_config {
            AccountSyncConfigData::Gmail {
                credential_source,
                client_id,
                client_secret,
                token_ref,
            } => {
                let creds = resolve_gmail_credentials(credential_source, client_id, client_secret);
                match creds {
                    Ok((client_id, client_secret)) => {
                        let mut gmail_auth = crate::mxr_provider_gmail::auth::GmailAuth::new(
                            client_id,
                            client_secret,
                            token_ref,
                        );
                        let auth_result = match gmail_auth.load_existing().await {
                            Ok(()) => Ok("Existing OAuth token loaded.".to_string()),
                            Err(_) => gmail_auth.interactive_auth().await.map(|_| {
                                "Browser authorization completed and token stored.".to_string()
                            }),
                        };
                        match auth_result {
                            Ok(detail) => {
                                auth = Some(account_step(true, detail));
                                let client =
                                    crate::mxr_provider_gmail::client::GmailClient::new(gmail_auth);
                                match client.list_labels().await {
                                    Ok(response) => {
                                        let count =
                                            response.labels.map(|labels| labels.len()).unwrap_or(0);
                                        sync = Some(account_step(
                                            true,
                                            format!("Gmail sync ok: {count} labels"),
                                        ));
                                    }
                                    Err(error) => {
                                        ok = false;
                                        sync = Some(account_step(false, error.to_string()));
                                    }
                                }
                            }
                            Err(error) => {
                                ok = false;
                                auth = Some(account_step(false, error.to_string()));
                                sync = Some(account_step(
                                    false,
                                    "Skipped Gmail sync because authorization failed.".into(),
                                ));
                            }
                        }
                    }
                    Err(error) => {
                        ok = false;
                        auth = Some(account_step(false, error));
                        sync = Some(account_step(
                            false,
                            "Skipped Gmail sync because OAuth credentials are unavailable.".into(),
                        ));
                    }
                }
            }
            AccountSyncConfigData::Imap {
                host,
                port,
                username,
                password_ref,
                auth_required,
                use_tls,
                ..
            } => {
                let provider = crate::mxr_provider_imap::ImapProvider::new(
                    crate::mxr_core::AccountId::from_provider_id("imap", &account.email),
                    crate::mxr_provider_imap::config::ImapConfig {
                        host,
                        port,
                        username,
                        password_ref,
                        auth_required,
                        use_tls,
                    },
                );
                match provider.sync_labels().await {
                    Ok(folders) => {
                        sync = Some(account_step(
                            true,
                            format!("IMAP sync ok: {} folders", folders.len()),
                        ));
                    }
                    Err(error) => {
                        ok = false;
                        sync = Some(account_step(false, error.to_string()));
                    }
                }
            }
            AccountSyncConfigData::OutlookPersonal { client_id, token_ref }
            | AccountSyncConfigData::OutlookWork { client_id, token_ref } => {
                let tenant = match &account.sync {
                    Some(AccountSyncConfigData::OutlookWork { .. }) => {
                        crate::mxr_provider_outlook::OutlookTenant::Work
                    }
                    _ => crate::mxr_provider_outlook::OutlookTenant::Personal,
                };
                let cid = client_id
                    .or_else(|| crate::mxr_provider_outlook::BUNDLED_CLIENT_ID.map(String::from));
                match cid {
                    None => {
                        ok = false;
                        sync = Some(account_step(
                            false,
                            "No client_id and no bundled OUTLOOK_CLIENT_ID".into(),
                        ));
                    }
                    Some(cid) => {
                        let auth_inst = std::sync::Arc::new(
                            crate::mxr_provider_outlook::OutlookAuth::new(cid, token_ref, tenant),
                        );
                        let email = account.email.clone();
                        let token_fn: std::sync::Arc<
                            dyn Fn() -> futures::future::BoxFuture<
                                'static,
                                anyhow::Result<String>,
                            > + Send
                                + Sync,
                        > = std::sync::Arc::new(move || {
                            let a = auth_inst.clone();
                            Box::pin(async move {
                                a.get_valid_access_token()
                                    .await
                                    .map_err(|e| anyhow::anyhow!(e))
                            })
                        });
                        let factory = crate::mxr_provider_imap::XOAuth2ImapSessionFactory::new(
                            "outlook.office365.com".to_string(),
                            993,
                            email.clone(),
                            token_fn,
                        );
                        let provider = crate::mxr_provider_imap::ImapProvider::with_session_factory(
                            crate::mxr_core::AccountId::from_provider_id("outlook", &email),
                            crate::mxr_provider_imap::config::ImapConfig {
                                host: "outlook.office365.com".to_string(),
                                port: 993,
                                username: email,
                                password_ref: String::new(),
                                auth_required: true,
                                use_tls: true,
                            },
                            Box::new(factory),
                        );
                        match provider.sync_labels().await {
                            Ok(folders) => {
                                sync = Some(account_step(
                                    true,
                                    format!("Outlook IMAP ok: {} folders", folders.len()),
                                ));
                            }
                            Err(error) => {
                                ok = false;
                                sync = Some(account_step(false, error.to_string()));
                            }
                        }
                    }
                }
            }
        }
    }

    match account.send {
        Some(AccountSendConfigData::Gmail) => {
            send = Some(account_step(true, "Gmail send configured.".into()));
        }
        Some(
            send_cfg @ (AccountSendConfigData::OutlookPersonal { .. }
                | AccountSendConfigData::OutlookWork { .. }),
        ) => {
            let (token_ref, tenant) = match send_cfg {
                AccountSendConfigData::OutlookPersonal { token_ref } => {
                    (token_ref, crate::mxr_provider_outlook::OutlookTenant::Personal)
                }
                AccountSendConfigData::OutlookWork { token_ref } => {
                    (token_ref, crate::mxr_provider_outlook::OutlookTenant::Work)
                }
                _ => unreachable!(),
            };
            let cid = crate::mxr_provider_outlook::BUNDLED_CLIENT_ID.map(String::from);
            match cid {
                None => {
                    ok = false;
                    send = Some(account_step(
                        false,
                        "No bundled OUTLOOK_CLIENT_ID for Outlook send".into(),
                    ));
                }
                Some(cid) => {
                    let auth_inst = std::sync::Arc::new(
                        crate::mxr_provider_outlook::OutlookAuth::new(cid, token_ref, tenant),
                    );
                    let email = account.email.clone();
                    let token_fn: std::sync::Arc<
                        dyn Fn() -> futures::future::BoxFuture<'static, anyhow::Result<String>>
                            + Send
                            + Sync,
                    > = std::sync::Arc::new(move || {
                        let a = auth_inst.clone();
                        Box::pin(async move {
                            a.get_valid_access_token()
                                .await
                                .map_err(|e| anyhow::anyhow!(e))
                        })
                    });
                    let smtp_host = match tenant {
                        crate::mxr_provider_outlook::OutlookTenant::Personal => {
                            "smtp-mail.outlook.com"
                        }
                        crate::mxr_provider_outlook::OutlookTenant::Work => "smtp.office365.com",
                    };
                    let provider = crate::mxr_provider_outlook::OutlookSmtpSendProvider::new(
                        smtp_host.to_string(),
                        587,
                        email,
                        token_fn,
                    );
                    match provider.test_connection().await {
                        Ok(()) => {
                            send = Some(account_step(true, "Outlook SMTP ok".into()));
                        }
                        Err(error) => {
                            ok = false;
                            send = Some(account_step(false, error));
                        }
                    }
                }
            }
        }
        Some(AccountSendConfigData::Smtp {
            host,
            port,
            username,
            password_ref,
            auth_required,
            use_tls,
            ..
        }) => {
            let provider = crate::mxr_provider_smtp::SmtpSendProvider::new(
                crate::mxr_provider_smtp::config::SmtpConfig {
                    host,
                    port,
                    username,
                    password_ref,
                    auth_required,
                    use_tls,
                },
            );
            match provider.test_connection().await {
                Ok(()) => {
                    send = Some(account_step(true, "SMTP send ok".into()));
                }
                Err(error) => {
                    ok = false;
                    send = Some(account_step(false, error.to_string()));
                }
            }
        }
        None if account.sync.is_none() => {
            ok = false;
            send = Some(account_step(
                false,
                "No sync or send configuration provided.".into(),
            ));
        }
        None => {}
    }

    account_operation_result(
        ok,
        if ok {
            format!("Account '{}' test passed.", account.key)
        } else {
            format!("Account '{}' test failed.", account.key)
        },
        None,
        auth,
        sync,
        send,
    )
}

fn account_step(ok: bool, detail: String) -> AccountOperationStep {
    AccountOperationStep { ok, detail }
}

fn account_operation_result(
    ok: bool,
    summary: String,
    save: Option<AccountOperationStep>,
    auth: Option<AccountOperationStep>,
    sync: Option<AccountOperationStep>,
    send: Option<AccountOperationStep>,
) -> AccountOperationResult {
    AccountOperationResult {
        ok,
        summary,
        save,
        auth,
        sync,
        send,
    }
}

fn resolve_gmail_credentials(
    credential_source: GmailCredentialSourceData,
    client_id: String,
    client_secret: Option<String>,
) -> Result<(String, String), String> {
    match credential_source {
        GmailCredentialSourceData::Bundled => {
            match (
                crate::mxr_provider_gmail::auth::BUNDLED_CLIENT_ID,
                crate::mxr_provider_gmail::auth::BUNDLED_CLIENT_SECRET,
            ) {
                (Some(id), Some(secret)) => Ok((id.to_string(), secret.to_string())),
                _ => {
                    if client_id.trim().is_empty()
                        || client_secret.as_deref().unwrap_or("").trim().is_empty()
                    {
                        Err("Bundled Gmail OAuth credentials are unavailable. Switch Credential source to Custom and enter your client ID/client secret.".into())
                    } else {
                        Ok((client_id, client_secret.unwrap_or_default()))
                    }
                }
            }
        }
        GmailCredentialSourceData::Custom => {
            if client_id.trim().is_empty()
                || client_secret.as_deref().unwrap_or("").trim().is_empty()
            {
                Err("Custom Gmail OAuth requires both client ID and client secret.".into())
            } else {
                Ok((client_id, client_secret.unwrap_or_default()))
            }
        }
    }
}

fn sync_config_to_data(sync: crate::mxr_config::SyncProviderConfig) -> AccountSyncConfigData {
    match sync {
        crate::mxr_config::SyncProviderConfig::Gmail {
            credential_source,
            client_id,
            client_secret,
            token_ref,
        } => AccountSyncConfigData::Gmail {
            credential_source: match credential_source {
                crate::mxr_config::GmailCredentialSource::Bundled => {
                    GmailCredentialSourceData::Bundled
                }
                crate::mxr_config::GmailCredentialSource::Custom => {
                    GmailCredentialSourceData::Custom
                }
            },
            client_id,
            client_secret,
            token_ref,
        },
        crate::mxr_config::SyncProviderConfig::Imap {
            host,
            port,
            username,
            password_ref,
            auth_required,
            use_tls,
        } => AccountSyncConfigData::Imap {
            host,
            port,
            username,
            password_ref,
            password: None,
            auth_required,
            use_tls,
        },
        crate::mxr_config::SyncProviderConfig::OutlookPersonal {
            client_id,
            token_ref,
        } => AccountSyncConfigData::OutlookPersonal { client_id, token_ref },
        crate::mxr_config::SyncProviderConfig::OutlookWork {
            client_id,
            token_ref,
        } => AccountSyncConfigData::OutlookWork { client_id, token_ref },
    }
}

fn config_account_id(
    key: &str,
    account: &crate::mxr_config::AccountConfig,
) -> crate::mxr_core::AccountId {
    let kind = account
        .sync
        .as_ref()
        .map(config_sync_kind_label)
        .or_else(|| account.send.as_ref().map(config_send_kind_label))
        .unwrap_or_else(|| key.to_string());
    crate::mxr_core::AccountId::from_provider_id(&kind, &account.email)
}

fn config_sync_kind_label(sync: &crate::mxr_config::SyncProviderConfig) -> String {
    match sync {
        crate::mxr_config::SyncProviderConfig::Gmail { .. } => "gmail".into(),
        crate::mxr_config::SyncProviderConfig::Imap { .. } => "imap".into(),
        crate::mxr_config::SyncProviderConfig::OutlookPersonal { .. }
        | crate::mxr_config::SyncProviderConfig::OutlookWork { .. } => "outlook".into(),
    }
}

fn config_send_kind_label(send: &crate::mxr_config::SendProviderConfig) -> String {
    match send {
        crate::mxr_config::SendProviderConfig::Gmail => "gmail".into(),
        crate::mxr_config::SendProviderConfig::Smtp { .. } => "smtp".into(),
        crate::mxr_config::SendProviderConfig::OutlookPersonal { .. }
        | crate::mxr_config::SendProviderConfig::OutlookWork { .. } => "outlook".into(),
    }
}

fn account_primary_provider_kind(account: &crate::mxr_config::AccountConfig) -> String {
    account
        .sync
        .as_ref()
        .map(config_sync_kind_label)
        .or_else(|| account.send.as_ref().map(config_send_kind_label))
        .unwrap_or_else(|| "unknown".into())
}

fn provider_kind_label(kind: &crate::mxr_core::ProviderKind) -> &'static str {
    match kind {
        crate::mxr_core::ProviderKind::Gmail => "gmail",
        crate::mxr_core::ProviderKind::Imap => "imap",
        crate::mxr_core::ProviderKind::Smtp => "smtp",
        crate::mxr_core::ProviderKind::OutlookPersonal => "outlook-personal",
        crate::mxr_core::ProviderKind::OutlookWork => "outlook-work",
        crate::mxr_core::ProviderKind::Fake => "fake",
    }
}

fn send_config_to_data(send: crate::mxr_config::SendProviderConfig) -> AccountSendConfigData {
    match send {
        crate::mxr_config::SendProviderConfig::Gmail => AccountSendConfigData::Gmail,
        crate::mxr_config::SendProviderConfig::OutlookPersonal { token_ref } => {
            AccountSendConfigData::OutlookPersonal { token_ref }
        }
        crate::mxr_config::SendProviderConfig::OutlookWork { token_ref } => {
            AccountSendConfigData::OutlookWork { token_ref }
        }
        crate::mxr_config::SendProviderConfig::Smtp {
            host,
            port,
            username,
            password_ref,
            auth_required,
            use_tls,
        } => AccountSendConfigData::Smtp {
            host,
            port,
            username,
            password_ref,
            password: None,
            auth_required,
            use_tls,
        },
    }
}

fn sync_data_to_config(
    data: AccountSyncConfigData,
) -> Result<crate::mxr_config::SyncProviderConfig, String> {
    match data {
        AccountSyncConfigData::Gmail {
            credential_source,
            client_id,
            client_secret,
            token_ref,
        } => Ok(crate::mxr_config::SyncProviderConfig::Gmail {
            credential_source: match credential_source {
                GmailCredentialSourceData::Bundled => {
                    crate::mxr_config::GmailCredentialSource::Bundled
                }
                GmailCredentialSourceData::Custom => {
                    crate::mxr_config::GmailCredentialSource::Custom
                }
            },
            client_id,
            client_secret,
            token_ref,
        }),
        AccountSyncConfigData::Imap {
            host,
            port,
            username,
            password_ref,
            auth_required,
            use_tls,
            ..
        } => Ok(crate::mxr_config::SyncProviderConfig::Imap {
            host,
            port,
            username,
            password_ref,
            auth_required,
            use_tls,
        }),
        AccountSyncConfigData::OutlookPersonal { client_id, token_ref } => {
            Ok(crate::mxr_config::SyncProviderConfig::OutlookPersonal { client_id, token_ref })
        }
        AccountSyncConfigData::OutlookWork { client_id, token_ref } => {
            Ok(crate::mxr_config::SyncProviderConfig::OutlookWork { client_id, token_ref })
        }
    }
}

fn send_data_to_config(
    data: AccountSendConfigData,
) -> Result<crate::mxr_config::SendProviderConfig, String> {
    match data {
        AccountSendConfigData::Gmail => Ok(crate::mxr_config::SendProviderConfig::Gmail),
        AccountSendConfigData::OutlookPersonal { token_ref } => {
            Ok(crate::mxr_config::SendProviderConfig::OutlookPersonal { token_ref })
        }
        AccountSendConfigData::OutlookWork { token_ref } => {
            Ok(crate::mxr_config::SendProviderConfig::OutlookWork { token_ref })
        }
        AccountSendConfigData::Smtp {
            host,
            port,
            username,
            password_ref,
            auth_required,
            use_tls,
            ..
        } => Ok(crate::mxr_config::SendProviderConfig::Smtp {
            host,
            port,
            username,
            password_ref,
            auth_required,
            use_tls,
        }),
    }
}

fn persist_account_passwords(account: &AccountConfigData) -> anyhow::Result<()> {
    if let Some(AccountSyncConfigData::Imap {
        auth_required,
        username,
        password_ref,
        password: Some(password),
        ..
    }) = &account.sync
    {
        persist_account_password("IMAP", *auth_required, username, password_ref, password)?;
    }

    if let Some(AccountSendConfigData::Smtp {
        auth_required,
        username,
        password_ref,
        password: Some(password),
        ..
    }) = &account.send
    {
        persist_account_password("SMTP", *auth_required, username, password_ref, password)?;
    }

    Ok(())
}

fn persist_account_password(
    service: &str,
    auth_required: bool,
    username: &str,
    password_ref: &str,
    password: &str,
) -> anyhow::Result<()> {
    if !auth_required || password.is_empty() {
        return Ok(());
    }
    if username.trim().is_empty() {
        anyhow::bail!("{service} user is required to store the password.");
    }
    if password_ref.trim().is_empty() {
        anyhow::bail!("{service} pass ref is required to store the password.");
    }
    keyring::Entry::new(password_ref, username)?.set_password(password)?;
    Ok(())
}

async fn dry_run_rules(
    state: &Arc<AppState>,
    rule_key: Option<String>,
    all: bool,
    after: Option<String>,
) -> Result<Vec<DryRunResult>, String> {
    let rows = if all {
        state.store.list_rules().await.map_err(|e| e.to_string())?
    } else if let Some(rule_key) = rule_key {
        match state
            .store
            .get_rule_by_id_or_name(&rule_key)
            .await
            .map_err(|e| e.to_string())?
        {
            Some(row) => vec![row],
            None => return Err(format!("Rule not found: {rule_key}")),
        }
    } else {
        return Err("Provide a rule or use --all".to_string());
    };

    let rules: Vec<Rule> = rows.iter().map(row_to_rule).collect::<Result<_, _>>()?;
    let engine = RuleEngine::new(rules.clone());
    let after = after
        .map(|value| {
            chrono::NaiveDate::parse_from_str(&value, "%Y-%m-%d")
                .map(|date| date.and_hms_opt(0, 0, 0).unwrap())
                .map(|dt| {
                    chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(dt, chrono::Utc)
                })
                .map_err(|e| e.to_string())
        })
        .transpose()?;

    let mut owned_messages = Vec::new();
    for account in state
        .store
        .list_accounts()
        .await
        .map_err(|e| e.to_string())?
    {
        let labels = state
            .store
            .list_labels_by_account(&account.id)
            .await
            .map_err(|e| e.to_string())?;
        let envelopes = state
            .store
            .list_envelopes_by_account(&account.id, 10_000, 0)
            .await
            .map_err(|e| e.to_string())?;
        for envelope in envelopes {
            if after.is_some_and(|cutoff| envelope.date < cutoff) {
                continue;
            }
            let body = state
                .store
                .get_body(&envelope.id)
                .await
                .map_err(|e| e.to_string())?;
            let label_ids = state
                .store
                .get_message_label_ids(&envelope.id)
                .await
                .map_err(|e| e.to_string())?;
            let visible_labels = labels
                .iter()
                .filter(|label| label_ids.iter().any(|id| id == &label.id))
                .map(|label| label.provider_id.clone())
                .collect();
            owned_messages.push(DryRunMessage::from_parts(envelope, body, visible_labels));
        }
    }

    let dry_run_input: Vec<_> = owned_messages
        .iter()
        .map(|message| {
            (
                message as &dyn crate::mxr_rules::MessageView,
                message.id.as_str(),
                message.from.as_str(),
                message.subject.as_str(),
            )
        })
        .collect();

    if all {
        Ok(rules
            .iter()
            .filter(|rule| rule.enabled)
            .filter_map(|rule| engine.dry_run(&rule.id, &dry_run_input))
            .collect())
    } else {
        Ok(rules
            .first()
            .and_then(|rule| engine.dry_run(&rule.id, &dry_run_input))
            .into_iter()
            .collect())
    }
}

struct DryRunMessage {
    id: String,
    from: String,
    to: Vec<String>,
    subject: String,
    labels: Vec<String>,
    has_attachment: bool,
    size_bytes: u64,
    date: chrono::DateTime<chrono::Utc>,
    is_unread: bool,
    is_starred: bool,
    has_unsubscribe: bool,
    body_text: Option<String>,
}

impl DryRunMessage {
    fn from_parts(
        envelope: crate::mxr_core::Envelope,
        body: Option<crate::mxr_core::MessageBody>,
        labels: Vec<String>,
    ) -> Self {
        Self {
            id: envelope.id.to_string(),
            from: envelope.from.email,
            to: envelope.to.into_iter().map(|addr| addr.email).collect(),
            subject: envelope.subject,
            labels,
            has_attachment: envelope.has_attachments,
            size_bytes: envelope.size_bytes,
            date: envelope.date,
            is_unread: !envelope.flags.contains(crate::mxr_core::MessageFlags::READ),
            is_starred: envelope
                .flags
                .contains(crate::mxr_core::MessageFlags::STARRED),
            has_unsubscribe: !matches!(
                envelope.unsubscribe,
                crate::mxr_core::types::UnsubscribeMethod::None
            ),
            body_text: body.and_then(|body| body.text_plain.or(body.text_html)),
        }
    }
}

impl crate::mxr_rules::MessageView for DryRunMessage {
    fn sender_email(&self) -> &str {
        &self.from
    }

    fn to_emails(&self) -> &[String] {
        &self.to
    }

    fn subject(&self) -> &str {
        &self.subject
    }

    fn labels(&self) -> &[String] {
        &self.labels
    }

    fn has_attachment(&self) -> bool {
        self.has_attachment
    }

    fn size_bytes(&self) -> u64 {
        self.size_bytes
    }

    fn date(&self) -> chrono::DateTime<chrono::Utc> {
        self.date
    }

    fn is_unread(&self) -> bool {
        self.is_unread
    }

    fn is_starred(&self) -> bool {
        self.is_starred
    }

    fn has_unsubscribe(&self) -> bool {
        self.has_unsubscribe
    }

    fn body_text(&self) -> Option<&str> {
        self.body_text.as_deref()
    }
}

async fn handle_export_thread(
    state: &Arc<AppState>,
    thread_id: &crate::mxr_core::ThreadId,
    format: &ExportFormat,
) -> Response {
    match build_export_thread(state, thread_id).await {
        Ok(export_thread) => {
            let reader_config = ReaderConfig::default();
            let content = crate::mxr_export::export(&export_thread, format, &reader_config);
            Response::Ok {
                data: ResponseData::ExportResult { content },
            }
        }
        Err(e) => Response::Error { message: e },
    }
}

async fn handle_export_search(
    state: &Arc<AppState>,
    query: &str,
    format: &ExportFormat,
) -> Response {
    let search = state.search.lock().await;
    let search_results =
        match search.search(query, 100, 0, crate::mxr_core::types::SortOrder::DateDesc) {
            Ok(results) => results,
            Err(e) => {
                return Response::Error {
                    message: e.to_string(),
                }
            }
        };
    drop(search);

    // Collect unique thread IDs from search results
    let thread_ids: Vec<crate::mxr_core::ThreadId> = {
        let mut seen = std::collections::HashSet::new();
        search_results
            .results
            .iter()
            .filter_map(|r| {
                let tid =
                    crate::mxr_core::ThreadId::from_uuid(uuid::Uuid::parse_str(&r.thread_id).ok()?);
                if seen.insert(tid.clone()) {
                    Some(tid)
                } else {
                    None
                }
            })
            .collect()
    };

    let reader_config = ReaderConfig::default();
    let mut all_content = String::new();

    for tid in &thread_ids {
        match build_export_thread(state, tid).await {
            Ok(export_thread) => {
                all_content.push_str(&crate::mxr_export::export(
                    &export_thread,
                    format,
                    &reader_config,
                ));
                all_content.push('\n');
            }
            Err(e) => {
                tracing::warn!(thread_id = %tid, error = %e, "Skipping thread in bulk export");
            }
        }
    }

    Response::Ok {
        data: ResponseData::ExportResult {
            content: all_content,
        },
    }
}

async fn materialize_attachment_file(
    state: &Arc<AppState>,
    message_id: &crate::mxr_core::MessageId,
    attachment_id: &crate::mxr_core::AttachmentId,
) -> Result<crate::mxr_protocol::AttachmentFile, crate::mxr_core::MxrError> {
    let envelope = state
        .store
        .get_envelope(message_id)
        .await
        .map_err(|err| crate::mxr_core::MxrError::Store(err.to_string()))?
        .ok_or_else(|| crate::mxr_core::MxrError::NotFound(format!("message {message_id}")))?;

    let mut body = state.sync_engine.get_body(message_id).await?;
    let attachment = body
        .attachments
        .iter()
        .find(|attachment| &attachment.id == attachment_id)
        .cloned()
        .ok_or_else(|| {
            crate::mxr_core::MxrError::NotFound(format!("attachment {attachment_id}"))
        })?;

    if let Some(path) = attachment.local_path.as_ref().filter(|path| path.exists()) {
        return Ok(crate::mxr_protocol::AttachmentFile {
            attachment_id: attachment.id,
            filename: attachment.filename,
            path: path.display().to_string(),
        });
    }

    let provider = state
        .get_provider(Some(&envelope.account_id))
        .map_err(crate::mxr_core::MxrError::Provider)?;
    let bytes = provider
        .fetch_attachment(&envelope.provider_id, &attachment.provider_id)
        .await?;

    let target_dir = state.attachment_dir().join(message_id.as_str());
    tokio::fs::create_dir_all(&target_dir)
        .await
        .map_err(crate::mxr_core::MxrError::Io)?;

    let filename = sanitized_attachment_filename(&attachment.filename, &attachment.id);
    let path = target_dir.join(filename);
    tokio::fs::write(&path, bytes)
        .await
        .map_err(crate::mxr_core::MxrError::Io)?;

    for existing in &mut body.attachments {
        if existing.id == *attachment_id {
            existing.local_path = Some(path.clone());
        }
    }
    state
        .store
        .insert_body(&body)
        .await
        .map_err(|err| crate::mxr_core::MxrError::Store(err.to_string()))?;

    Ok(crate::mxr_protocol::AttachmentFile {
        attachment_id: attachment.id,
        filename: attachment.filename,
        path: path.display().to_string(),
    })
}

fn sanitized_attachment_filename(
    filename: &str,
    attachment_id: &crate::mxr_core::AttachmentId,
) -> String {
    let candidate = std::path::Path::new(filename)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(filename);
    let sanitized: String = candidate
        .chars()
        .map(|ch| match ch {
            '/' | '\\' | ':' | '\0' => '_',
            _ if ch.is_control() => '_',
            _ => ch,
        })
        .collect();

    if sanitized.trim().is_empty() {
        format!("attachment-{}", attachment_id.as_str())
    } else {
        sanitized
    }
}

fn open_local_file(path: &str) -> anyhow::Result<()> {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open").arg(path).spawn()?;
        Ok(())
    }
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open").arg(path).spawn()?;
        Ok(())
    }
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("cmd")
            .args(["/C", "start", "", path])
            .spawn()?;
        Ok(())
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        anyhow::bail!("opening attachments is not supported on this platform")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn dispatch_ping_returns_pong() {
        let state = Arc::new(AppState::in_memory().await.unwrap());

        let msg = IpcMessage {
            id: 1,
            payload: IpcPayload::Request(Request::Ping),
        };
        let resp = handle_request(&state, &msg).await;

        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Pong,
            }) => {}
            other => panic!("Expected Pong, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn dispatch_list_envelopes_after_sync() {
        let state = Arc::new(AppState::in_memory().await.unwrap());

        // Initial sync
        state
            .sync_engine
            .sync_account(state.default_provider().as_ref())
            .await
            .unwrap();

        let msg = IpcMessage {
            id: 2,
            payload: IpcPayload::Request(Request::ListEnvelopes {
                label_id: None,
                account_id: None,
                limit: 100,
                offset: 0,
            }),
        };
        let resp = handle_request(&state, &msg).await;

        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Envelopes { envelopes },
            }) => {
                assert_eq!(envelopes.len(), 55);
            }
            other => panic!("Expected Envelopes, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn dispatch_list_envelopes_by_label() {
        let state = Arc::new(AppState::in_memory().await.unwrap());

        state
            .sync_engine
            .sync_account(state.default_provider().as_ref())
            .await
            .unwrap();

        // Get labels first
        let labels_msg = IpcMessage {
            id: 10,
            payload: IpcPayload::Request(Request::ListLabels { account_id: None }),
        };
        let resp = handle_request(&state, &labels_msg).await;
        let labels = match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Labels { labels },
            }) => labels,
            other => panic!("Expected Labels, got {:?}", other),
        };

        // Find Inbox label
        let inbox = labels
            .iter()
            .find(|l| l.name == "Inbox")
            .expect("Inbox label missing");

        // Fetch envelopes by Inbox label
        let msg = IpcMessage {
            id: 11,
            payload: IpcPayload::Request(Request::ListEnvelopes {
                label_id: Some(inbox.id.clone()),
                account_id: None,
                limit: 100,
                offset: 0,
            }),
        };
        let resp = handle_request(&state, &msg).await;
        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Envelopes { envelopes },
            }) => {
                assert!(
                    !envelopes.is_empty(),
                    "Inbox label should have envelopes, got 0. Inbox label_id={}",
                    inbox.id
                );
            }
            IpcPayload::Response(Response::Error { message }) => {
                panic!("Got error response: {message}");
            }
            other => panic!("Expected Envelopes, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn dispatch_list_labels_without_accounts_returns_empty() {
        let state = Arc::new(AppState::in_memory_without_accounts().await.unwrap());

        let msg = IpcMessage {
            id: 12,
            payload: IpcPayload::Request(Request::ListLabels { account_id: None }),
        };
        let resp = handle_request(&state, &msg).await;

        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Labels { labels },
            }) => assert!(labels.is_empty()),
            other => panic!("Expected Labels, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn dispatch_list_envelopes_without_accounts_returns_empty() {
        let state = Arc::new(AppState::in_memory_without_accounts().await.unwrap());

        let msg = IpcMessage {
            id: 13,
            payload: IpcPayload::Request(Request::ListEnvelopes {
                label_id: None,
                account_id: None,
                limit: 100,
                offset: 0,
            }),
        };
        let resp = handle_request(&state, &msg).await;

        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Envelopes { envelopes },
            }) => assert!(envelopes.is_empty()),
            other => panic!("Expected Envelopes, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn dispatch_create_label_persists_and_returns_label() {
        let state = Arc::new(AppState::in_memory().await.unwrap());
        let account_id = state.default_account_id();

        let create_msg = IpcMessage {
            id: 14,
            payload: IpcPayload::Request(Request::CreateLabel {
                name: "Urgent".to_string(),
                color: Some("#ff6600".to_string()),
                account_id: Some(account_id.clone()),
            }),
        };
        let resp = handle_request(&state, &create_msg).await;
        let created = match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Label { label },
            }) => label,
            other => panic!("Expected Label, got {:?}", other),
        };
        assert_eq!(created.name, "Urgent");
        assert_eq!(created.color.as_deref(), Some("#ff6600"));
        assert_eq!(created.account_id, account_id);

        let list_msg = IpcMessage {
            id: 15,
            payload: IpcPayload::Request(Request::ListLabels {
                account_id: Some(account_id),
            }),
        };
        let resp = handle_request(&state, &list_msg).await;
        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Labels { labels },
            }) => {
                assert!(labels.iter().any(|label| label.name == "Urgent"));
            }
            other => panic!("Expected Labels, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn dispatch_upsert_and_list_rules() {
        let state = Arc::new(AppState::in_memory().await.unwrap());
        let now = chrono::Utc::now();
        let rule = serde_json::json!({
            "id": "rule-1",
            "name": "Archive newsletters",
            "enabled": true,
            "priority": 10,
            "conditions": {"type":"field","field":"has_label","label":"newsletters"},
            "actions": [{"type":"archive"}],
            "created_at": now,
            "updated_at": now
        });

        let upsert_msg = IpcMessage {
            id: 20,
            payload: IpcPayload::Request(Request::UpsertRule { rule: rule.clone() }),
        };
        let resp = handle_request(&state, &upsert_msg).await;
        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::RuleData { rule: returned },
            }) => {
                assert_eq!(returned["name"], "Archive newsletters");
            }
            other => panic!("Expected RuleData, got {:?}", other),
        }

        let list_msg = IpcMessage {
            id: 21,
            payload: IpcPayload::Request(Request::ListRules),
        };
        let resp = handle_request(&state, &list_msg).await;
        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Rules { rules },
            }) => {
                assert_eq!(rules.len(), 1);
                assert_eq!(rules[0]["id"], "rule-1");
            }
            other => panic!("Expected Rules, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn dispatch_dry_run_rules_returns_matching_messages() {
        let state = Arc::new(AppState::in_memory().await.unwrap());
        state
            .sync_engine
            .sync_account(state.default_provider().as_ref())
            .await
            .unwrap();
        let now = chrono::Utc::now();
        let rule = serde_json::json!({
            "id": "rule-1",
            "name": "Mark unread",
            "enabled": true,
            "priority": 10,
            "conditions": {"type":"field","field":"is_unread"},
            "actions": [{"type":"mark_read"}],
            "created_at": now,
            "updated_at": now
        });
        let _ = handle_request(
            &state,
            &IpcMessage {
                id: 22,
                payload: IpcPayload::Request(Request::UpsertRule { rule }),
            },
        )
        .await;

        let dry_run_msg = IpcMessage {
            id: 23,
            payload: IpcPayload::Request(Request::DryRunRules {
                rule: Some("rule-1".to_string()),
                all: false,
                after: None,
            }),
        };
        let resp = handle_request(&state, &dry_run_msg).await;
        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::RuleDryRun { results },
            }) => {
                assert_eq!(results.len(), 1);
                assert!(!results[0]["matches"].as_array().unwrap().is_empty());
            }
            other => panic!("Expected RuleDryRun, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn dispatch_upsert_rule_form_and_get_rule_form() {
        let state = Arc::new(AppState::in_memory().await.unwrap());

        let upsert_msg = IpcMessage {
            id: 231,
            payload: IpcPayload::Request(Request::UpsertRuleForm {
                existing_rule: None,
                name: "Archive unread".into(),
                condition: "is:unread".into(),
                action: "archive".into(),
                priority: 25,
                enabled: true,
            }),
        };
        let resp = handle_request(&state, &upsert_msg).await;
        let rule_id = match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::RuleData { rule },
            }) => {
                assert_eq!(rule["name"], "Archive unread");
                rule["id"].as_str().unwrap().to_string()
            }
            other => panic!("Expected RuleData, got {:?}", other),
        };

        let get_form_msg = IpcMessage {
            id: 232,
            payload: IpcPayload::Request(Request::GetRuleForm { rule: rule_id }),
        };
        let resp = handle_request(&state, &get_form_msg).await;
        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::RuleFormData { form },
            }) => {
                assert_eq!(form.name, "Archive unread");
                assert_eq!(form.condition, "is:unread");
                assert_eq!(form.action, "archive");
                assert_eq!(form.priority, 25);
                assert!(form.enabled);
            }
            other => panic!("Expected RuleFormData, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn dispatch_rename_label_updates_visible_label() {
        let state = Arc::new(AppState::in_memory().await.unwrap());
        let account_id = state.default_account_id();

        let create_msg = IpcMessage {
            id: 14,
            payload: IpcPayload::Request(Request::CreateLabel {
                name: "Projects".to_string(),
                color: None,
                account_id: Some(account_id.clone()),
            }),
        };
        let _ = handle_request(&state, &create_msg).await;

        let rename_msg = IpcMessage {
            id: 15,
            payload: IpcPayload::Request(Request::RenameLabel {
                old: "Projects".to_string(),
                new: "Client Work".to_string(),
                account_id: Some(account_id.clone()),
            }),
        };
        let resp = handle_request(&state, &rename_msg).await;
        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Label { label },
            }) => {
                assert_eq!(label.name, "Client Work");
                assert_eq!(label.provider_id, "Client Work");
            }
            other => panic!("Expected Label, got {:?}", other),
        }

        let list_msg = IpcMessage {
            id: 16,
            payload: IpcPayload::Request(Request::ListLabels {
                account_id: Some(account_id),
            }),
        };
        let resp = handle_request(&state, &list_msg).await;
        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Labels { labels },
            }) => {
                assert!(labels.iter().any(|label| label.name == "Client Work"));
                assert!(!labels.iter().any(|label| label.name == "Projects"));
            }
            other => panic!("Expected Labels, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn dispatch_delete_label_removes_it_from_store() {
        let state = Arc::new(AppState::in_memory().await.unwrap());
        let account_id = state.default_account_id();

        let create_msg = IpcMessage {
            id: 17,
            payload: IpcPayload::Request(Request::CreateLabel {
                name: "Temporary".to_string(),
                color: None,
                account_id: Some(account_id.clone()),
            }),
        };
        let _ = handle_request(&state, &create_msg).await;

        let delete_msg = IpcMessage {
            id: 18,
            payload: IpcPayload::Request(Request::DeleteLabel {
                name: "Temporary".to_string(),
                account_id: Some(account_id.clone()),
            }),
        };
        let resp = handle_request(&state, &delete_msg).await;
        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Ack,
            }) => {}
            other => panic!("Expected Ack, got {:?}", other),
        }

        let list_msg = IpcMessage {
            id: 19,
            payload: IpcPayload::Request(Request::ListLabels {
                account_id: Some(account_id),
            }),
        };
        let resp = handle_request(&state, &list_msg).await;
        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Labels { labels },
            }) => {
                assert!(!labels.iter().any(|label| label.name == "Temporary"));
            }
            other => panic!("Expected Labels, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn dispatch_count_after_sync() {
        let state = Arc::new(AppState::in_memory().await.unwrap());

        state
            .sync_engine
            .sync_account(state.default_provider().as_ref())
            .await
            .unwrap();

        let msg = IpcMessage {
            id: 3,
            payload: IpcPayload::Request(Request::Count {
                query: "deployment".to_string(),
                mode: None,
            }),
        };
        let resp = handle_request(&state, &msg).await;

        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Count { count },
            }) => {
                assert!(count > 0, "Expected non-zero count for 'deployment'");
            }
            other => panic!("Expected Count, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn dispatch_list_saved_searches_empty() {
        let state = Arc::new(AppState::in_memory().await.unwrap());

        let msg = IpcMessage {
            id: 4,
            payload: IpcPayload::Request(Request::ListSavedSearches),
        };
        let resp = handle_request(&state, &msg).await;

        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::SavedSearches { searches },
            }) => {
                assert!(searches.is_empty());
            }
            other => panic!("Expected empty SavedSearches, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn dispatch_create_and_list_saved_searches() {
        let state = Arc::new(AppState::in_memory().await.unwrap());

        // Create
        let create_msg = IpcMessage {
            id: 5,
            payload: IpcPayload::Request(Request::CreateSavedSearch {
                name: "Important".to_string(),
                query: "is:starred".to_string(),
                search_mode: crate::mxr_core::SearchMode::Lexical,
            }),
        };
        let resp = handle_request(&state, &create_msg).await;
        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::SavedSearchData { search },
            }) => {
                assert_eq!(search.name, "Important");
                assert_eq!(search.query, "is:starred");
                assert_eq!(search.search_mode, crate::mxr_core::SearchMode::Lexical);
            }
            other => panic!("Expected SavedSearchData, got {:?}", other),
        }

        // List
        let list_msg = IpcMessage {
            id: 6,
            payload: IpcPayload::Request(Request::ListSavedSearches),
        };
        let resp = handle_request(&state, &list_msg).await;
        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::SavedSearches { searches },
            }) => {
                assert_eq!(searches.len(), 1);
                assert_eq!(searches[0].name, "Important");
                assert_eq!(
                    searches[0].search_mode,
                    crate::mxr_core::SearchMode::Lexical
                );
            }
            other => panic!("Expected SavedSearches, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn dispatch_create_saved_search_persists_requested_mode() {
        let state = Arc::new(AppState::in_memory().await.unwrap());
        let create_msg = IpcMessage {
            id: 51,
            payload: IpcPayload::Request(Request::CreateSavedSearch {
                name: "Hybrid".to_string(),
                query: "deployment".to_string(),
                search_mode: crate::mxr_core::SearchMode::Hybrid,
            }),
        };

        let resp = handle_request(&state, &create_msg).await;
        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::SavedSearchData { search },
            }) => {
                assert_eq!(search.search_mode, crate::mxr_core::SearchMode::Hybrid);
            }
            other => panic!("Expected SavedSearchData, got {:?}", other),
        }

        let saved = state
            .store
            .get_saved_search_by_name("Hybrid")
            .await
            .unwrap()
            .expect("saved search");
        assert_eq!(saved.search_mode, crate::mxr_core::SearchMode::Hybrid);
    }

    #[tokio::test]
    async fn dispatch_run_saved_search_returns_results() {
        let state = Arc::new(AppState::in_memory().await.unwrap());
        state
            .sync_engine
            .sync_account(state.default_provider().as_ref())
            .await
            .unwrap();

        let create = IpcMessage {
            id: 200,
            payload: IpcPayload::Request(Request::CreateSavedSearch {
                name: "Deploy".into(),
                query: "deployment".into(),
                search_mode: crate::mxr_core::SearchMode::Lexical,
            }),
        };
        handle_request(&state, &create).await;

        let msg = IpcMessage {
            id: 201,
            payload: IpcPayload::Request(Request::RunSavedSearch {
                name: "Deploy".into(),
                limit: 10,
            }),
        };
        let resp = handle_request(&state, &msg).await;
        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::SearchResults { results, .. },
            }) => assert!(!results.is_empty(), "saved search should return results"),
            other => panic!("Expected SearchResults, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn dispatch_status() {
        let state = Arc::new(AppState::in_memory().await.unwrap());

        let msg = IpcMessage {
            id: 7,
            payload: IpcPayload::Request(Request::GetStatus),
        };
        let resp = handle_request(&state, &msg).await;

        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data:
                    ResponseData::Status {
                        uptime_secs: _,
                        accounts,
                        total_messages: _,
                        daemon_pid,
                        sync_statuses,
                        protocol_version,
                        daemon_version,
                        daemon_build_id,
                        repair_required,
                    },
            }) => {
                assert!(!accounts.is_empty());
                assert!(daemon_pid.is_some());
                assert!(!sync_statuses.is_empty());
                assert!(protocol_version >= crate::mxr_protocol::IPC_PROTOCOL_VERSION);
                assert!(daemon_version.is_some());
                assert!(daemon_build_id.is_some());
                assert!(!repair_required);
            }
            other => panic!("Expected Status, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn dispatch_status_does_not_block_when_search_is_busy() {
        let state = Arc::new(AppState::in_memory().await.unwrap());
        let _search_guard = state.search.lock().await;

        let msg = IpcMessage {
            id: 8,
            payload: IpcPayload::Request(Request::GetStatus),
        };

        let resp = tokio::time::timeout(
            std::time::Duration::from_millis(250),
            handle_request(&state, &msg),
        )
        .await
        .expect("status should not block on a busy search index");

        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Status { .. },
            }) => {}
            other => panic!("Expected Status, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn dispatch_doctor_report() {
        let state = Arc::new(AppState::in_memory().await.unwrap());

        let msg = IpcMessage {
            id: 81,
            payload: IpcPayload::Request(Request::GetDoctorReport),
        };
        let resp = handle_request(&state, &msg).await;
        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::DoctorReport { report },
            }) => {
                assert!(report.database_path.contains("mxr.db"));
                assert!(report.index_path.contains("search_index"));
                assert!(report.daemon_version.is_some());
                assert!(report.daemon_build_id.is_some());
            }
            other => panic!("Expected DoctorReport, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn dispatch_sync_status() {
        let state = Arc::new(AppState::in_memory().await.unwrap());
        let account_id = state.default_account_id();

        let msg = IpcMessage {
            id: 82,
            payload: IpcPayload::Request(Request::GetSyncStatus { account_id }),
        };
        let resp = handle_request(&state, &msg).await;
        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::SyncStatus { sync },
            }) => {
                assert!(!sync.account_name.is_empty());
                assert!(sync.current_cursor_summary.is_some());
            }
            other => panic!("Expected SyncStatus, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn dispatch_search_returns_results() {
        let state = Arc::new(AppState::in_memory().await.unwrap());

        // Sync first so search index is populated
        state
            .sync_engine
            .sync_account(state.default_provider().as_ref())
            .await
            .unwrap();

        let msg = IpcMessage {
            id: 10,
            payload: IpcPayload::Request(Request::Search {
                query: "deployment".to_string(),
                limit: 10,
                offset: 0,
                mode: None,
                sort: None,
                explain: false,
            }),
        };
        let resp = handle_request(&state, &msg).await;

        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::SearchResults { results, .. },
            }) => {
                assert!(
                    !results.is_empty(),
                    "Search for 'deployment' should return results"
                );
                assert_eq!(results[0].mode, crate::mxr_core::SearchMode::Lexical);
            }
            other => panic!("Expected SearchResults, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn dispatch_search_explain_returns_execution_details() {
        let state = Arc::new(AppState::in_memory().await.unwrap());
        state
            .sync_engine
            .sync_account(state.default_provider().as_ref())
            .await
            .unwrap();

        let msg = IpcMessage {
            id: 11,
            payload: IpcPayload::Request(Request::Search {
                query: "deployment".to_string(),
                limit: 5,
                offset: 0,
                mode: Some(crate::mxr_core::SearchMode::Lexical),
                sort: Some(crate::mxr_core::types::SortOrder::DateDesc),
                explain: true,
            }),
        };
        let resp = handle_request(&state, &msg).await;

        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data:
                    ResponseData::SearchResults {
                        results,
                        explain: Some(explain),
                        ..
                    },
            }) => {
                assert!(!results.is_empty());
                assert_eq!(explain.requested_mode, crate::mxr_core::SearchMode::Lexical);
                assert_eq!(explain.executed_mode, crate::mxr_core::SearchMode::Lexical);
                assert_eq!(explain.dense_candidates, 0);
                assert_eq!(explain.final_results as usize, results.len());
                assert!(!explain.results.is_empty());
            }
            other => panic!(
                "Expected SearchResults with explain payload, got {:?}",
                other
            ),
        }
    }

    #[tokio::test]
    async fn dispatch_structured_search_in_semantic_mode_falls_back_to_lexical() {
        let state = Arc::new(AppState::in_memory().await.unwrap());
        state
            .sync_engine
            .sync_account(state.default_provider().as_ref())
            .await
            .unwrap();

        let msg = IpcMessage {
            id: 13,
            payload: IpcPayload::Request(Request::Search {
                query: "is:unread".to_string(),
                limit: 10,
                offset: 0,
                mode: Some(crate::mxr_core::SearchMode::Semantic),
                sort: Some(crate::mxr_core::types::SortOrder::DateDesc),
                explain: false,
            }),
        };
        let resp = handle_request(&state, &msg).await;
        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::SearchResults { results, .. },
            }) => {
                assert!(!results.is_empty());
            }
            other => panic!("Expected SearchResults, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn dispatch_structured_search_in_semantic_mode_explains_fallback() {
        let state = Arc::new(AppState::in_memory().await.unwrap());
        state
            .sync_engine
            .sync_account(state.default_provider().as_ref())
            .await
            .unwrap();

        let msg = IpcMessage {
            id: 14,
            payload: IpcPayload::Request(Request::Search {
                query: "is:unread".to_string(),
                limit: 10,
                offset: 0,
                mode: Some(crate::mxr_core::SearchMode::Semantic),
                sort: Some(crate::mxr_core::types::SortOrder::DateDesc),
                explain: true,
            }),
        };
        let resp = handle_request(&state, &msg).await;
        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data:
                    ResponseData::SearchResults {
                        explain: Some(explain),
                        ..
                    },
            }) => {
                assert_eq!(
                    explain.requested_mode,
                    crate::mxr_core::SearchMode::Semantic
                );
                assert_eq!(explain.executed_mode, crate::mxr_core::SearchMode::Lexical);
                assert!(explain
                    .notes
                    .iter()
                    .any(|note| note.contains("no semantic text terms")));
            }
            other => panic!(
                "Expected SearchResults with explain payload, got {:?}",
                other
            ),
        }
    }

    #[tokio::test]
    async fn dispatch_search_rejects_invalid_structured_query() {
        let state = Arc::new(AppState::in_memory().await.unwrap());
        let msg = IpcMessage {
            id: 12,
            payload: IpcPayload::Request(Request::Search {
                query: "older:30q".to_string(),
                limit: 10,
                offset: 0,
                mode: None,
                sort: None,
                explain: false,
            }),
        };
        let resp = handle_request(&state, &msg).await;

        match resp.payload {
            IpcPayload::Response(Response::Error { message }) => {
                assert!(message.contains("Invalid search query"));
                assert!(message.contains("invalid date"));
            }
            other => panic!("Expected Error, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn dispatch_get_body_after_sync() {
        let state = Arc::new(AppState::in_memory().await.unwrap());

        state
            .sync_engine
            .sync_account(state.default_provider().as_ref())
            .await
            .unwrap();

        // Get first envelope
        let envelopes_msg = IpcMessage {
            id: 11,
            payload: IpcPayload::Request(Request::ListEnvelopes {
                label_id: None,
                account_id: None,
                limit: 1,
                offset: 0,
            }),
        };
        let resp = handle_request(&state, &envelopes_msg).await;
        let message_id = match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Envelopes { envelopes },
            }) => {
                assert!(!envelopes.is_empty());
                envelopes[0].id.clone()
            }
            other => panic!("Expected Envelopes, got {:?}", other),
        };

        // Get body for that envelope
        let body_msg = IpcMessage {
            id: 12,
            payload: IpcPayload::Request(Request::GetBody {
                message_id: message_id.clone(),
            }),
        };
        let resp = handle_request(&state, &body_msg).await;

        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Body { body },
            }) => {
                assert!(
                    body.text_plain.is_some(),
                    "Body should have text_plain content"
                );
            }
            other => panic!("Expected Body, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn dispatch_list_bodies_omits_missing_rows() {
        let state = Arc::new(AppState::in_memory().await.unwrap());
        let missing_id = crate::mxr_core::MessageId::new();

        let msg = IpcMessage {
            id: 13,
            payload: IpcPayload::Request(Request::ListBodies {
                message_ids: vec![missing_id],
            }),
        };
        let resp = handle_request(&state, &msg).await;

        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Bodies { bodies },
            }) => {
                assert!(
                    bodies.is_empty(),
                    "missing body rows should be omitted so clients can retry"
                );
            }
            other => panic!("Expected Bodies, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn dispatch_list_bodies_preserves_attachments() {
        let state = Arc::new(AppState::in_memory().await.unwrap());
        let id = sync_and_get_first_id(&state).await;
        let attachment_id = crate::mxr_core::AttachmentId::new();

        state
            .store
            .insert_body(&crate::mxr_core::types::MessageBody {
                message_id: id.clone(),
                text_plain: Some("hello".into()),
                text_html: None,
                attachments: vec![crate::mxr_core::types::AttachmentMeta {
                    id: attachment_id.clone(),
                    message_id: id.clone(),
                    filename: "report.pdf".into(),
                    mime_type: "application/pdf".into(),
                    size_bytes: 1024,
                    local_path: None,
                    provider_id: "att-1".into(),
                }],
                fetched_at: chrono::Utc::now(),
                metadata: Default::default(),
            })
            .await
            .unwrap();

        let msg = IpcMessage {
            id: 14,
            payload: IpcPayload::Request(Request::ListBodies {
                message_ids: vec![id.clone()],
            }),
        };
        let resp = handle_request(&state, &msg).await;

        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Bodies { bodies },
            }) => {
                assert_eq!(bodies.len(), 1);
                assert_eq!(bodies[0].attachments.len(), 1);
                assert_eq!(bodies[0].attachments[0].id, attachment_id);
                assert_eq!(bodies[0].attachments[0].filename, "report.pdf");
            }
            other => panic!("Expected Bodies, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn dispatch_download_attachment_persists_local_path() {
        let state = AppState::in_memory().await.unwrap();
        state.set_attachment_dir_for_tests(
            std::env::temp_dir().join(format!("mxr-attachments-test-{}", uuid::Uuid::new_v4())),
        );
        let state = Arc::new(state);

        state
            .sync_engine
            .sync_account(state.default_provider().as_ref())
            .await
            .unwrap();

        let list_msg = IpcMessage {
            id: 14,
            payload: IpcPayload::Request(Request::ListEnvelopes {
                label_id: None,
                account_id: None,
                limit: 200,
                offset: 0,
            }),
        };
        let resp = handle_request(&state, &list_msg).await;
        let envelope = match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Envelopes { envelopes },
            }) => envelopes
                .into_iter()
                .find(|envelope| envelope.has_attachments)
                .expect("fixture should include an attachment"),
            other => panic!("Expected Envelopes, got {:?}", other),
        };

        let body_msg = IpcMessage {
            id: 15,
            payload: IpcPayload::Request(Request::GetBody {
                message_id: envelope.id.clone(),
            }),
        };
        let resp = handle_request(&state, &body_msg).await;
        let attachment_id = match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Body { body },
            }) => body.attachments[0].id.clone(),
            other => panic!("Expected Body, got {:?}", other),
        };

        let download_msg = IpcMessage {
            id: 16,
            payload: IpcPayload::Request(Request::DownloadAttachment {
                message_id: envelope.id.clone(),
                attachment_id: attachment_id.clone(),
            }),
        };
        let resp = handle_request(&state, &download_msg).await;
        let path = match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::AttachmentFile { file },
            }) => std::path::PathBuf::from(file.path),
            other => panic!("Expected AttachmentFile, got {:?}", other),
        };

        assert!(path.exists(), "downloaded attachment should exist on disk");

        let body = state
            .store
            .get_body(&envelope.id)
            .await
            .unwrap()
            .expect("body should remain cached");
        let attachment = body
            .attachments
            .iter()
            .find(|attachment| attachment.id == attachment_id)
            .expect("attachment should still exist");
        assert_eq!(attachment.local_path.as_ref(), Some(&path));

        let _ = std::fs::remove_dir_all(state.attachment_dir());
    }

    /// Helper: sync, list envelopes, return first envelope's id.
    async fn sync_and_get_first_id(state: &Arc<AppState>) -> crate::mxr_core::MessageId {
        state
            .sync_engine
            .sync_account(state.default_provider().as_ref())
            .await
            .unwrap();

        let msg = IpcMessage {
            id: 100,
            payload: IpcPayload::Request(Request::ListEnvelopes {
                label_id: None,
                account_id: None,
                limit: 1,
                offset: 0,
            }),
        };
        let resp = handle_request(state, &msg).await;
        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Envelopes { envelopes },
            }) => {
                assert!(!envelopes.is_empty());
                envelopes[0].id.clone()
            }
            other => panic!("Expected Envelopes, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn dispatch_mutation_star() {
        let state = Arc::new(AppState::in_memory().await.unwrap());
        let id = sync_and_get_first_id(&state).await;

        let msg = IpcMessage {
            id: 1,
            payload: IpcPayload::Request(Request::Mutation(MutationCommand::Star {
                message_ids: vec![id.clone()],
                starred: true,
            })),
        };
        let resp = handle_request(&state, &msg).await;
        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Ack,
            }) => {}
            other => panic!("Expected Ack, got {:?}", other),
        }

        // Verify flag is set
        let get_msg = IpcMessage {
            id: 2,
            payload: IpcPayload::Request(Request::GetEnvelope { message_id: id }),
        };
        let resp = handle_request(&state, &get_msg).await;
        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Envelope { envelope },
            }) => {
                assert!(
                    envelope
                        .flags
                        .contains(crate::mxr_core::types::MessageFlags::STARRED),
                    "Expected STARRED flag to be set, got {:?}",
                    envelope.flags
                );
            }
            other => panic!("Expected Envelope, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn dispatch_mutation_set_read() {
        let state = Arc::new(AppState::in_memory().await.unwrap());
        let id = sync_and_get_first_id(&state).await;

        let msg = IpcMessage {
            id: 1,
            payload: IpcPayload::Request(Request::Mutation(MutationCommand::SetRead {
                message_ids: vec![id.clone()],
                read: true,
            })),
        };
        let resp = handle_request(&state, &msg).await;
        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Ack,
            }) => {}
            other => panic!("Expected Ack, got {:?}", other),
        }

        let get_msg = IpcMessage {
            id: 2,
            payload: IpcPayload::Request(Request::GetEnvelope { message_id: id }),
        };
        let resp = handle_request(&state, &get_msg).await;
        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Envelope { envelope },
            }) => {
                assert!(
                    envelope
                        .flags
                        .contains(crate::mxr_core::types::MessageFlags::READ),
                    "Expected READ flag to be set, got {:?}",
                    envelope.flags
                );
            }
            other => panic!("Expected Envelope, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn dispatch_mutation_archive() {
        let state = Arc::new(AppState::in_memory().await.unwrap());
        let id = sync_and_get_first_id(&state).await;

        let msg = IpcMessage {
            id: 1,
            payload: IpcPayload::Request(Request::Mutation(MutationCommand::Archive {
                message_ids: vec![id.clone()],
            })),
        };
        let resp = handle_request(&state, &msg).await;
        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Ack,
            }) => {}
            other => panic!("Expected Ack, got {:?}", other),
        }

        let events = state
            .store
            .list_events(10, None, Some("mutation"))
            .await
            .unwrap();
        let id_str = id.as_str();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].message_id.as_deref(), Some(id_str.as_str()));
        assert!(events[0].summary.contains("Archived"));
    }

    #[tokio::test]
    async fn dispatch_mutation_read_and_archive() {
        let state = Arc::new(AppState::in_memory().await.unwrap());
        let id = sync_and_get_first_id(&state).await;

        let msg = IpcMessage {
            id: 1,
            payload: IpcPayload::Request(Request::Mutation(MutationCommand::ReadAndArchive {
                message_ids: vec![id.clone()],
            })),
        };
        let resp = handle_request(&state, &msg).await;
        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Ack,
            }) => {}
            other => panic!("Expected Ack, got {:?}", other),
        }

        let envelope = state
            .store
            .get_envelope(&id)
            .await
            .unwrap()
            .expect("message should still exist");
        assert!(envelope
            .flags
            .contains(crate::mxr_core::types::MessageFlags::READ));

        let label_ids = state.store.get_message_label_ids(&id).await.unwrap();
        assert!(!label_ids
            .iter()
            .any(|label_id| label_id.as_str() == "INBOX"));

        let events = state
            .store
            .list_events(10, None, Some("mutation"))
            .await
            .unwrap();
        assert!(events[0].summary.contains("read and archived"));
    }

    #[tokio::test]
    async fn dispatch_mutation_trash() {
        let state = Arc::new(AppState::in_memory().await.unwrap());
        let id = sync_and_get_first_id(&state).await;

        let msg = IpcMessage {
            id: 1,
            payload: IpcPayload::Request(Request::Mutation(MutationCommand::Trash {
                message_ids: vec![id],
            })),
        };
        let resp = handle_request(&state, &msg).await;
        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Ack,
            }) => {}
            other => panic!("Expected Ack, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn dispatch_prepare_reply() {
        let state = Arc::new(AppState::in_memory().await.unwrap());
        let id = sync_and_get_first_id(&state).await;

        // Fetch body first so it's cached
        let body_msg = IpcMessage {
            id: 1,
            payload: IpcPayload::Request(Request::GetBody {
                message_id: id.clone(),
            }),
        };
        handle_request(&state, &body_msg).await;

        let msg = IpcMessage {
            id: 2,
            payload: IpcPayload::Request(Request::PrepareReply {
                message_id: id,
                reply_all: false,
            }),
        };
        let resp = handle_request(&state, &msg).await;
        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::ReplyContext { context },
            }) => {
                assert!(!context.reply_to.is_empty(), "reply_to should be non-empty");
                assert!(!context.subject.is_empty(), "subject should be non-empty");
            }
            other => panic!("Expected ReplyContext, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn dispatch_prepare_reply_all() {
        let state = Arc::new(AppState::in_memory().await.unwrap());
        let id = sync_and_get_first_id(&state).await;

        // Fetch body first
        let body_msg = IpcMessage {
            id: 1,
            payload: IpcPayload::Request(Request::GetBody {
                message_id: id.clone(),
            }),
        };
        handle_request(&state, &body_msg).await;

        let msg = IpcMessage {
            id: 2,
            payload: IpcPayload::Request(Request::PrepareReply {
                message_id: id,
                reply_all: true,
            }),
        };
        let resp = handle_request(&state, &msg).await;
        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::ReplyContext { context },
            }) => {
                assert!(!context.reply_to.is_empty(), "reply_to should be non-empty");
                assert!(!context.subject.is_empty(), "subject should be non-empty");
                // cc may or may not be empty depending on the message, but the field should exist
            }
            other => panic!("Expected ReplyContext, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn dispatch_prepare_reply_renders_html_context() {
        let state = Arc::new(AppState::in_memory().await.unwrap());
        let id = sync_and_get_first_id(&state).await;

        state
            .store
            .insert_body(&crate::mxr_core::types::MessageBody {
                message_id: id.clone(),
                text_plain: None,
                text_html: Some("<p>Hello <b>world</b></p>".into()),
                attachments: vec![],
                fetched_at: chrono::Utc::now(),
                metadata: Default::default(),
            })
            .await
            .unwrap();

        let msg = IpcMessage {
            id: 2,
            payload: IpcPayload::Request(Request::PrepareReply {
                message_id: id,
                reply_all: false,
            }),
        };
        let resp = handle_request(&state, &msg).await;
        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::ReplyContext { context },
            }) => {
                assert!(context.thread_context.contains("Hello world"));
                assert!(!context.thread_context.contains("<p>"));
            }
            other => panic!("Expected ReplyContext, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn dispatch_prepare_forward() {
        let state = Arc::new(AppState::in_memory().await.unwrap());
        let id = sync_and_get_first_id(&state).await;

        // Fetch body first
        let body_msg = IpcMessage {
            id: 1,
            payload: IpcPayload::Request(Request::GetBody {
                message_id: id.clone(),
            }),
        };
        handle_request(&state, &body_msg).await;

        let msg = IpcMessage {
            id: 2,
            payload: IpcPayload::Request(Request::PrepareForward { message_id: id }),
        };
        let resp = handle_request(&state, &msg).await;
        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::ForwardContext { context },
            }) => {
                assert!(!context.subject.is_empty(), "subject should be non-empty");
                assert!(
                    !context.forwarded_content.is_empty(),
                    "forwarded_content should be non-empty"
                );
            }
            other => panic!("Expected ForwardContext, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn modify_labels_persists_to_store_immediately() {
        let state = Arc::new(AppState::in_memory().await.unwrap());
        let id = sync_and_get_first_id(&state).await;

        let create = IpcMessage {
            id: 1,
            payload: IpcPayload::Request(Request::CreateLabel {
                name: "Follow Up".into(),
                color: None,
                account_id: None,
            }),
        };
        let label = match handle_request(&state, &create).await.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Label { label },
            }) => label,
            other => panic!("Expected Label response, got {:?}", other),
        };

        let modify = IpcMessage {
            id: 2,
            payload: IpcPayload::Request(Request::Mutation(MutationCommand::ModifyLabels {
                message_ids: vec![id.clone()],
                add: vec![label.name.clone()],
                remove: vec![],
            })),
        };
        match handle_request(&state, &modify).await.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Ack,
            }) => {}
            other => panic!("Expected Ack, got {:?}", other),
        }

        let label_ids = state.store.get_message_label_ids(&id).await.unwrap();
        assert!(label_ids.iter().any(|label_id| label_id == &label.id));
    }

    #[tokio::test]
    async fn get_thread_includes_message_label_provider_ids() {
        let state = Arc::new(AppState::in_memory().await.unwrap());
        let id = sync_and_get_first_id(&state).await;
        let envelope = state.store.get_envelope(&id).await.unwrap().unwrap();

        let create = IpcMessage {
            id: 1,
            payload: IpcPayload::Request(Request::CreateLabel {
                name: "Recruiters".into(),
                color: None,
                account_id: None,
            }),
        };
        let label = match handle_request(&state, &create).await.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Label { label },
            }) => label,
            other => panic!("Expected Label response, got {:?}", other),
        };

        state.store.add_message_label(&id, &label.id).await.unwrap();

        let msg = IpcMessage {
            id: 2,
            payload: IpcPayload::Request(Request::GetThread {
                thread_id: envelope.thread_id,
            }),
        };
        let resp = handle_request(&state, &msg).await;
        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Thread { messages, .. },
            }) => {
                let message = messages
                    .into_iter()
                    .find(|message| message.id == id)
                    .unwrap();
                assert!(message
                    .label_provider_ids
                    .iter()
                    .any(|provider_id| provider_id == &label.provider_id));
            }
            other => panic!("Expected Thread response, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn list_envelopes_includes_message_label_provider_ids() {
        let state = Arc::new(AppState::in_memory().await.unwrap());
        let id = sync_and_get_first_id(&state).await;

        let create = IpcMessage {
            id: 1,
            payload: IpcPayload::Request(Request::CreateLabel {
                name: "Recruiters".into(),
                color: None,
                account_id: None,
            }),
        };
        let label = match handle_request(&state, &create).await.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Label { label },
            }) => label,
            other => panic!("Expected Label response, got {:?}", other),
        };

        state.store.add_message_label(&id, &label.id).await.unwrap();

        let msg = IpcMessage {
            id: 2,
            payload: IpcPayload::Request(Request::ListEnvelopes {
                label_id: None,
                account_id: None,
                limit: 200,
                offset: 0,
            }),
        };
        let resp = handle_request(&state, &msg).await;
        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Envelopes { envelopes },
            }) => {
                let envelope = envelopes
                    .into_iter()
                    .find(|envelope| envelope.id == id)
                    .unwrap();
                assert!(envelope
                    .label_provider_ids
                    .iter()
                    .any(|provider_id| provider_id == &label.provider_id));
            }
            other => panic!("Expected Envelopes response, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn list_accounts_surfaces_runtime_accounts_without_config_entries() {
        let state = Arc::new(AppState::in_memory().await.unwrap());

        let msg = IpcMessage {
            id: 1,
            payload: IpcPayload::Request(Request::ListAccounts),
        };
        let resp = handle_request(&state, &msg).await;
        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Accounts { accounts },
            }) => {
                assert_eq!(accounts.len(), 1);
                assert_eq!(accounts[0].email, "user@example.com");
                assert_eq!(accounts[0].source, AccountSourceData::Runtime);
                assert_eq!(accounts[0].editable, AccountEditModeData::RuntimeOnly);
                assert!(accounts[0].is_default);
            }
            other => panic!("Expected Accounts response, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn dispatch_send_draft() {
        let state = Arc::new(AppState::in_memory().await.unwrap());

        let draft = crate::mxr_core::types::Draft {
            id: crate::mxr_core::DraftId::new(),
            account_id: state.default_account_id(),
            reply_headers: None,
            to: vec![crate::mxr_core::types::Address {
                name: None,
                email: "test@example.com".to_string(),
            }],
            cc: vec![],
            bcc: vec![],
            subject: "Test subject".to_string(),
            body_markdown: "Test body".to_string(),
            attachments: vec![],
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };

        let msg = IpcMessage {
            id: 1,
            payload: IpcPayload::Request(Request::SendDraft { draft }),
        };
        let resp = handle_request(&state, &msg).await;
        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Ack,
            }) => {}
            other => panic!("Expected Ack, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn dispatch_snooze_and_list() {
        let state = Arc::new(AppState::in_memory().await.unwrap());
        let id = sync_and_get_first_id(&state).await;

        // Snooze
        let wake_at = chrono::Utc::now() + chrono::Duration::hours(24);
        let msg = IpcMessage {
            id: 1,
            payload: IpcPayload::Request(Request::Snooze {
                message_id: id.clone(),
                wake_at,
            }),
        };
        let resp = handle_request(&state, &msg).await;
        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Ack,
            }) => {}
            other => panic!("Expected Ack for Snooze, got {:?}", other),
        }

        // List snoozed - should have 1
        let msg = IpcMessage {
            id: 2,
            payload: IpcPayload::Request(Request::ListSnoozed),
        };
        let resp = handle_request(&state, &msg).await;
        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::SnoozedMessages { snoozed },
            }) => {
                assert_eq!(snoozed.len(), 1, "Expected 1 snoozed message");
            }
            other => panic!("Expected SnoozedMessages, got {:?}", other),
        }

        // Unsnooze
        let msg = IpcMessage {
            id: 3,
            payload: IpcPayload::Request(Request::Unsnooze { message_id: id }),
        };
        let resp = handle_request(&state, &msg).await;
        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Ack,
            }) => {}
            other => panic!("Expected Ack for Unsnooze, got {:?}", other),
        }

        // List snoozed - should have 0
        let msg = IpcMessage {
            id: 4,
            payload: IpcPayload::Request(Request::ListSnoozed),
        };
        let resp = handle_request(&state, &msg).await;
        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::SnoozedMessages { snoozed },
            }) => {
                assert_eq!(
                    snoozed.len(),
                    0,
                    "Expected 0 snoozed messages after unsnooze"
                );
            }
            other => panic!("Expected SnoozedMessages, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn snooze_removes_inbox_and_unsnooze_restores_it() {
        let state = Arc::new(AppState::in_memory().await.unwrap());
        let id = sync_and_get_first_id(&state).await;
        let envelope = state.store.get_envelope(&id).await.unwrap().unwrap();
        let inbox = state
            .store
            .list_labels_by_account(&envelope.account_id)
            .await
            .unwrap()
            .into_iter()
            .find(|label| label.provider_id == "INBOX")
            .unwrap();

        let before = state.store.get_message_label_ids(&id).await.unwrap();
        assert!(before.iter().any(|label_id| label_id == &inbox.id));

        let snooze = IpcMessage {
            id: 1,
            payload: IpcPayload::Request(Request::Snooze {
                message_id: id.clone(),
                wake_at: chrono::Utc::now() + chrono::Duration::hours(4),
            }),
        };
        match handle_request(&state, &snooze).await.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Ack,
            }) => {}
            other => panic!("Expected Ack, got {:?}", other),
        }

        let snoozed_labels = state.store.get_message_label_ids(&id).await.unwrap();
        assert!(!snoozed_labels.iter().any(|label_id| label_id == &inbox.id));

        let unsnooze = IpcMessage {
            id: 2,
            payload: IpcPayload::Request(Request::Unsnooze {
                message_id: id.clone(),
            }),
        };
        match handle_request(&state, &unsnooze).await.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Ack,
            }) => {}
            other => panic!("Expected Ack, got {:?}", other),
        }

        let restored_labels = state.store.get_message_label_ids(&id).await.unwrap();
        assert!(restored_labels.iter().any(|label_id| label_id == &inbox.id));
    }

    #[tokio::test]
    async fn dispatch_set_flags() {
        let state = Arc::new(AppState::in_memory().await.unwrap());
        let id = sync_and_get_first_id(&state).await;

        use crate::mxr_core::types::MessageFlags;
        let flags = MessageFlags::READ | MessageFlags::STARRED;
        let msg = IpcMessage {
            id: 1,
            payload: IpcPayload::Request(Request::SetFlags {
                message_id: id.clone(),
                flags,
            }),
        };
        let resp = handle_request(&state, &msg).await;
        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Ack,
            }) => {}
            other => panic!("Expected Ack, got {:?}", other),
        }

        // Verify flags
        let get_msg = IpcMessage {
            id: 2,
            payload: IpcPayload::Request(Request::GetEnvelope { message_id: id }),
        };
        let resp = handle_request(&state, &get_msg).await;
        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Envelope { envelope },
            }) => {
                assert_eq!(
                    envelope.flags, flags,
                    "Expected flags {:?}, got {:?}",
                    flags, envelope.flags
                );
            }
            other => panic!("Expected Envelope, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn dispatch_unsubscribe_no_method() {
        let state = Arc::new(AppState::in_memory().await.unwrap());
        let id = sync_and_get_first_id(&state).await;

        // The first envelope from FakeProvider fixtures uses UnsubscribeMethod::None
        let msg = IpcMessage {
            id: 1,
            payload: IpcPayload::Request(Request::Unsubscribe { message_id: id }),
        };
        let resp = handle_request(&state, &msg).await;
        match resp.payload {
            IpcPayload::Response(Response::Error { message }) => {
                assert!(
                    message.contains("unsubscribe"),
                    "Expected error about unsubscribe, got: {}",
                    message
                );
            }
            other => panic!("Expected Error for no unsubscribe method, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn dispatch_unsubscribe_mailto_sends_via_provider() {
        let (state, fake) = AppState::in_memory_with_fake().await.unwrap();
        let state = Arc::new(state);
        state
            .sync_engine
            .sync_account(state.default_provider().as_ref())
            .await
            .unwrap();

        let mailto_id = state
            .store
            .list_envelopes_by_account(&state.default_account_id(), 200, 0)
            .await
            .unwrap()
            .into_iter()
            .find(|envelope| matches!(envelope.unsubscribe, UnsubscribeMethod::Mailto { .. }))
            .map(|envelope| envelope.id)
            .expect("mailto fixture");

        let msg = IpcMessage {
            id: 1,
            payload: IpcPayload::Request(Request::Unsubscribe {
                message_id: mailto_id,
            }),
        };
        let resp = handle_request(&state, &msg).await;
        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Ack,
            }) => {}
            other => panic!("Expected Ack for mailto unsubscribe, got {:?}", other),
        }

        let sent = fake.sent_drafts();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].to[0].email, "unsub@changelog.com");
        assert_eq!(sent[0].subject, "unsubscribe");
    }

    #[tokio::test]
    async fn dispatch_mutation_nonexistent_message() {
        let state = Arc::new(AppState::in_memory().await.unwrap());

        let fake_id = crate::mxr_core::MessageId::new();
        let msg = IpcMessage {
            id: 1,
            payload: IpcPayload::Request(Request::Mutation(MutationCommand::Star {
                message_ids: vec![fake_id],
                starred: true,
            })),
        };
        let resp = handle_request(&state, &msg).await;
        match resp.payload {
            IpcPayload::Response(Response::Error { message }) => {
                assert!(
                    message.contains("not found") || message.contains("Not found"),
                    "Expected 'not found' error, got: {}",
                    message
                );
            }
            other => panic!("Expected Error, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn dispatch_list_drafts_empty() {
        let state = Arc::new(AppState::in_memory().await.unwrap());

        let msg = IpcMessage {
            id: 1,
            payload: IpcPayload::Request(Request::ListDrafts),
        };
        let resp = handle_request(&state, &msg).await;
        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Drafts { drafts },
            }) => {
                assert!(drafts.is_empty(), "Expected empty drafts list");
            }
            other => panic!("Expected Drafts, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn dispatch_saved_search_delete() {
        let state = Arc::new(AppState::in_memory().await.unwrap());

        // Create a saved search
        let create_msg = IpcMessage {
            id: 20,
            payload: IpcPayload::Request(Request::CreateSavedSearch {
                name: "ToDelete".to_string(),
                query: "is:unread".to_string(),
                search_mode: crate::mxr_core::SearchMode::Lexical,
            }),
        };
        let resp = handle_request(&state, &create_msg).await;
        match &resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::SavedSearchData { search },
            }) => {
                assert_eq!(search.name, "ToDelete");
            }
            other => panic!("Expected SavedSearchData, got {:?}", other),
        }

        // Verify it's in the list
        let list_msg = IpcMessage {
            id: 21,
            payload: IpcPayload::Request(Request::ListSavedSearches),
        };
        let resp = handle_request(&state, &list_msg).await;
        match &resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::SavedSearches { searches },
            }) => {
                assert_eq!(searches.len(), 1);
                assert_eq!(searches[0].name, "ToDelete");
            }
            other => panic!("Expected SavedSearches with 1 item, got {:?}", other),
        }

        // Delete it
        let delete_msg = IpcMessage {
            id: 22,
            payload: IpcPayload::Request(Request::DeleteSavedSearch {
                name: "ToDelete".to_string(),
            }),
        };
        let resp = handle_request(&state, &delete_msg).await;
        match &resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Ack,
            }) => {}
            other => panic!("Expected Ack, got {:?}", other),
        }

        // Verify it's gone
        let list_msg2 = IpcMessage {
            id: 23,
            payload: IpcPayload::Request(Request::ListSavedSearches),
        };
        let resp = handle_request(&state, &list_msg2).await;
        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::SavedSearches { searches },
            }) => {
                assert!(
                    searches.is_empty(),
                    "Saved searches should be empty after delete"
                );
            }
            other => panic!("Expected empty SavedSearches, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn dispatch_export_thread_markdown() {
        let state = Arc::new(AppState::in_memory().await.unwrap());

        // Sync to get messages
        state
            .sync_engine
            .sync_account(state.default_provider().as_ref())
            .await
            .unwrap();

        // Get an envelope to find its thread_id
        let list_msg = IpcMessage {
            id: 1,
            payload: IpcPayload::Request(Request::ListEnvelopes {
                label_id: None,
                account_id: None,
                limit: 1,
                offset: 0,
            }),
        };
        let resp = handle_request(&state, &list_msg).await;
        let thread_id = match &resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Envelopes { envelopes },
            }) => envelopes[0].thread_id.clone(),
            other => panic!("Expected Envelopes, got {:?}", other),
        };

        // Export the thread as markdown
        let export_msg = IpcMessage {
            id: 2,
            payload: IpcPayload::Request(Request::ExportThread {
                thread_id,
                format: crate::mxr_core::types::ExportFormat::Markdown,
            }),
        };
        let resp = handle_request(&state, &export_msg).await;
        match &resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::ExportResult { content },
            }) => {
                assert!(
                    content.starts_with("# Thread:"),
                    "Should be markdown: {}",
                    content
                );
                assert!(content.contains("Exported from mxr"));
            }
            other => panic!("Expected ExportResult, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn dispatch_sync_now_acknowledges() {
        let state = Arc::new(AppState::in_memory().await.unwrap());

        let msg = IpcMessage {
            id: 300,
            payload: IpcPayload::Request(Request::SyncNow { account_id: None }),
        };
        let resp = handle_request(&state, &msg).await;

        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Ack,
            }) => {}
            other => panic!("Expected Ack, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn dispatch_export_thread_json_is_valid() {
        let state = Arc::new(AppState::in_memory().await.unwrap());
        state
            .sync_engine
            .sync_account(state.default_provider().as_ref())
            .await
            .unwrap();

        let list_msg = IpcMessage {
            id: 1,
            payload: IpcPayload::Request(Request::ListEnvelopes {
                label_id: None,
                account_id: None,
                limit: 1,
                offset: 0,
            }),
        };
        let resp = handle_request(&state, &list_msg).await;
        let thread_id = match &resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Envelopes { envelopes },
            }) => envelopes[0].thread_id.clone(),
            other => panic!("Expected Envelopes, got {:?}", other),
        };

        let export_msg = IpcMessage {
            id: 2,
            payload: IpcPayload::Request(Request::ExportThread {
                thread_id,
                format: crate::mxr_core::types::ExportFormat::Json,
            }),
        };
        let resp = handle_request(&state, &export_msg).await;
        match &resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::ExportResult { content },
            }) => {
                let parsed: serde_json::Value =
                    serde_json::from_str(content).expect("Export JSON should be valid");
                assert!(parsed["message_count"].as_u64().unwrap() >= 1);
                assert!(parsed["subject"].is_string());
            }
            other => panic!("Expected ExportResult, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn dispatch_get_headers_includes_standards_metadata() {
        let state = Arc::new(AppState::in_memory().await.unwrap());
        let id = sync_and_get_first_id(&state).await;

        let mut body = state.store.get_body(&id).await.unwrap().unwrap();
        body.metadata.list_id = Some("fixtures.example.com".into());
        body.metadata.auth_results = vec!["mx.example.net; dkim=pass".into()];
        body.metadata.content_language = vec!["en".into(), "fr".into()];
        state.store.insert_body(&body).await.unwrap();

        let msg = IpcMessage {
            id: 3,
            payload: IpcPayload::Request(Request::GetHeaders {
                message_id: id.clone(),
            }),
        };
        let resp = handle_request(&state, &msg).await;

        let headers = match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Headers { headers },
            }) => headers,
            other => panic!("Expected Headers, got {:?}", other),
        };

        assert!(headers.iter().any(|(name, _)| name == "From"));
        assert!(headers.iter().any(|(name, _)| name == "Subject"));
        assert!(headers
            .iter()
            .any(|(name, value)| name == "List-Id" && value == "fixtures.example.com"));
        assert!(headers.iter().any(|(name, value)| {
            name == "Authentication-Results" && value == "mx.example.net; dkim=pass"
        }));
        assert!(headers
            .iter()
            .any(|(name, value)| { name == "Content-Language" && value == "en, fr" }));
    }

    #[tokio::test]
    async fn dispatch_export_search_json_is_valid() {
        let state = Arc::new(AppState::in_memory().await.unwrap());
        state
            .sync_engine
            .sync_account(state.default_provider().as_ref())
            .await
            .unwrap();

        let msg = IpcMessage {
            id: 4,
            payload: IpcPayload::Request(Request::ExportSearch {
                query: "deployment".into(),
                format: crate::mxr_core::types::ExportFormat::Json,
            }),
        };
        let resp = handle_request(&state, &msg).await;

        match &resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::ExportResult { content },
            }) => {
                let parsed: serde_json::Value =
                    serde_json::from_str(content).expect("Export JSON should be valid");
                let messages = parsed["messages"]
                    .as_array()
                    .expect("export search should include messages");
                assert!(!messages.is_empty(), "export search should return results");
            }
            other => panic!("Expected ExportResult, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn dispatch_save_draft_to_server() {
        let state = Arc::new(AppState::in_memory().await.unwrap());

        let draft = crate::mxr_core::types::Draft {
            id: crate::mxr_core::DraftId::new(),
            account_id: state.default_account_id(),
            reply_headers: None,
            to: vec![crate::mxr_core::types::Address {
                name: Some("Recipient".into()),
                email: "recipient@example.com".into(),
            }],
            cc: vec![],
            bcc: vec![],
            subject: "Saved draft".into(),
            body_markdown: "Body".into(),
            attachments: vec![],
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };

        let msg = IpcMessage {
            id: 5,
            payload: IpcPayload::Request(Request::SaveDraftToServer { draft }),
        };
        let resp = handle_request(&state, &msg).await;

        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Ack,
            }) => {}
            other => panic!("Expected Ack, got {:?}", other),
        }
    }
}
