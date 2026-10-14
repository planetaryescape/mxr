//! Which tool calls run straight away and which wait for the user.
//!
//! This is the assistant's authority boundary, enforced in code before a
//! tool runs. The model can't talk its way past it: a call is classified
//! from its tool name and arguments alone, and anything not listed here
//! waits for approval. Mail content can describe an action but never grant
//! one; only the user's Approve does.

use serde_json::{Map, Value};

/// What happens to one tool call.
#[derive(Debug, Clone, PartialEq)]
pub enum Gate {
    /// Reads, previews and other calls that change nothing the user would
    /// need to undo.
    Run,
    /// A change. The call waits for the user. `preview` is a read-only call
    /// whose result shows the user what the change will do.
    Approve { preview: Option<(String, Value)> },
}

/// How a tool tells a preview from the real change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Rule {
    /// Never changes mail, drafts, rules or settings.
    Read,
    /// Previews unless `confirm` is `true`.
    Confirm,
    /// Changes unless `dry_run` is `true`. A missing `dry_run` is the real
    /// change, so it waits.
    DryRun,
    /// Previews without a `preview_token`; applies one with it.
    PreviewToken,
    /// Always a change.
    Always,
}

/// Every MCP tool the assistant can see. A tool added to the MCP server
/// without an entry here waits for approval, and a test fails until it is
/// classified.
const RULES: &[(&str, Rule)] = &[
    ("mxr_archive_outcome", Rule::Read),
    // Retains a reviewed selection; dispatches nothing until submit.
    ("mxr_archive_preview", Rule::Read),
    ("mxr_archive_submit", Rule::Confirm),
    ("mxr_arrivals", Rule::Read),
    ("mxr_arrivals_list", Rule::Read),
    ("mxr_attachment_chunk", Rule::Read),
    ("mxr_attachment_metadata", Rule::Read),
    ("mxr_categories", Rule::Read),
    ("mxr_category_assignment_apply", Rule::Always),
    ("mxr_category_assignment_preview", Rule::Read),
    ("mxr_category_create", Rule::DryRun),
    ("mxr_category_delete", Rule::PreviewToken),
    ("mxr_category_rename", Rule::DryRun),
    ("mxr_category_rule", Rule::PreviewToken),
    ("mxr_category_undo", Rule::Always),
    ("mxr_category_why", Rule::Read),
    ("mxr_copy_draft_to_provider", Rule::Confirm),
    ("mxr_corrections", Rule::Read),
    ("mxr_delete_draft", Rule::Confirm),
    // Writes a suggestion; nothing is saved or sent.
    ("mxr_draft_assist", Rule::Read),
    ("mxr_draft_attachment_begin", Rule::Confirm),
    ("mxr_draft_attachment_chunk", Rule::Confirm),
    ("mxr_draft_attachment_finalize", Rule::Confirm),
    ("mxr_draft_preview", Rule::Read),
    ("mxr_get_draft", Rule::Read),
    ("mxr_got_it", Rule::Confirm),
    ("mxr_list_drafts", Rule::Read),
    ("mxr_list_messages", Rule::Read),
    ("mxr_list_place", Rule::Read),
    ("mxr_list_scheduled_sends", Rule::Read),
    ("mxr_messages", Rule::Read),
    ("mxr_move", Rule::Confirm),
    ("mxr_mutate", Rule::Confirm),
    ("mxr_mutation_preview", Rule::Read),
    ("mxr_person", Rule::Read),
    ("mxr_prepare_reply", Rule::Read),
    ("mxr_read_message", Rule::Read),
    ("mxr_read_thread", Rule::Read),
    ("mxr_reading_edition", Rule::Read),
    ("mxr_reading_highlight", Rule::Confirm),
    ("mxr_reading_highlights", Rule::Read),
    // `confirm` fetches the article from the web.
    ("mxr_reading_item", Rule::Confirm),
    ("mxr_reading_later", Rule::DryRun),
    ("mxr_reading_marginalia", Rule::Confirm),
    ("mxr_records", Rule::Read),
    ("mxr_records_ask", Rule::Read),
    ("mxr_records_export_preview", Rule::Read),
    ("mxr_records_subscriptions", Rule::Read),
    ("mxr_recovery_activate", Rule::Confirm),
    ("mxr_recovery_prepare_draft", Rule::Confirm),
    ("mxr_recovery_status", Rule::Read),
    ("mxr_rule_form", Rule::Read),
    ("mxr_rules", Rule::Read),
    // A new local draft: nothing leaves the machine and the user can delete it.
    ("mxr_save_draft", Rule::Read),
    ("mxr_search", Rule::Read),
    ("mxr_send_draft", Rule::Confirm),
    ("mxr_send_outcome", Rule::Read),
    ("mxr_sorting_rule", Rule::PreviewToken),
    ("mxr_status", Rule::Read),
    ("mxr_sweep_preview", Rule::Read),
    // Fetches mail the user already has; changes nothing in the mailbox.
    ("mxr_sync", Rule::Read),
    ("mxr_sync_draft_to_provider", Rule::Confirm),
    ("mxr_thread_context", Rule::Read),
    ("mxr_thread_gists", Rule::Read),
    ("mxr_todo_get", Rule::Read),
    ("mxr_todo_list", Rule::Read),
    ("mxr_todo_resolve", Rule::Confirm),
    ("mxr_todo_set_state", Rule::Confirm),
    // Replaces the user's draft content.
    ("mxr_update_draft", Rule::Always),
    ("mxr_updates_digest", Rule::Read),
    ("mxr_updates_let_go_preview", Rule::Read),
    ("mxr_workspace", Rule::Read),
];

/// Classify one call. `arguments` is what the model sent, minus `reason`.
pub fn classify(tool: &str, arguments: &Value) -> Gate {
    let rule = RULES
        .iter()
        .find(|(name, _)| *name == tool)
        .map(|(_, rule)| *rule);
    let Some(rule) = rule else {
        return Gate::Approve { preview: None };
    };
    let flag = |key: &str| arguments.get(key);
    match rule {
        Rule::Read => Gate::Run,
        // Only a literal `false` or nothing at all is a preview: a string
        // "true" or any other value is treated as the real change.
        Rule::Confirm => match flag("confirm") {
            None | Some(Value::Null) | Some(Value::Bool(false)) => Gate::Run,
            Some(_) => Gate::Approve {
                preview: Some((
                    tool.to_string(),
                    with(arguments, "confirm", Value::Bool(false)),
                )),
            },
        },
        Rule::DryRun => match flag("dry_run") {
            Some(Value::Bool(true)) => Gate::Run,
            _ => Gate::Approve {
                preview: Some((
                    tool.to_string(),
                    with(arguments, "dry_run", Value::Bool(true)),
                )),
            },
        },
        Rule::PreviewToken => match flag("preview_token") {
            None | Some(Value::Null) => Gate::Run,
            Some(Value::String(token)) if token.is_empty() => Gate::Run,
            // The token binds the selection an earlier preview showed; a
            // fresh preview could differ, so none is shown here.
            Some(_) => Gate::Approve { preview: None },
        },
        Rule::Always => Gate::Approve { preview: None },
    }
}

/// Whether `tool` has an entry in [`RULES`].
pub fn is_classified(tool: &str) -> bool {
    RULES.iter().any(|(name, _)| *name == tool)
}

fn with(arguments: &Value, key: &str, value: Value) -> Value {
    let mut object = arguments.as_object().cloned().unwrap_or_else(Map::new);
    object.insert(key.to_string(), value);
    Value::Object(object)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn reads_run_and_unknown_tools_wait() {
        assert_eq!(classify("mxr_search", &json!({"query": "x"})), Gate::Run);
        assert_eq!(
            classify("mxr_new_thing", &json!({})),
            Gate::Approve { preview: None }
        );
    }

    #[test]
    fn confirm_tools_preview_unless_confirm_is_anything_but_false() {
        assert_eq!(
            classify("mxr_mutate", &json!({"message_ids": ["a"]})),
            Gate::Run
        );
        assert_eq!(
            classify("mxr_mutate", &json!({"confirm": false})),
            Gate::Run
        );
        for confirm in [json!(true), json!("true"), json!(1)] {
            let gate = classify(
                "mxr_mutate",
                &json!({"action": "archive", "confirm": confirm}),
            );
            assert_eq!(
                gate,
                Gate::Approve {
                    preview: Some((
                        "mxr_mutate".into(),
                        json!({"action": "archive", "confirm": false})
                    ))
                }
            );
        }
    }

    #[test]
    fn dry_run_tools_wait_when_dry_run_is_missing() {
        assert_eq!(
            classify("mxr_reading_later", &json!({"dry_run": true})),
            Gate::Run
        );
        assert!(matches!(
            classify("mxr_reading_later", &json!({"item_keys": ["k"]})),
            Gate::Approve { preview: Some(_) }
        ));
        assert!(matches!(
            classify("mxr_reading_later", &json!({"dry_run": "true"})),
            Gate::Approve { .. }
        ));
    }

    #[test]
    fn applying_a_preview_token_waits_without_a_fresh_preview() {
        assert_eq!(
            classify("mxr_sorting_rule", &json!({"name": "x"})),
            Gate::Run
        );
        assert_eq!(
            classify("mxr_sorting_rule", &json!({"preview_token": "tok"})),
            Gate::Approve { preview: None }
        );
    }

    #[test]
    fn sending_always_waits() {
        assert!(matches!(
            classify("mxr_send_draft", &json!({"draft_id": "d", "confirm": true})),
            Gate::Approve { .. }
        ));
        assert!(matches!(
            classify("mxr_update_draft", &json!({})),
            Gate::Approve { preview: None }
        ));
    }
}
