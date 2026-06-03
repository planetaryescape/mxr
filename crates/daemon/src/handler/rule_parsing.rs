use crate::state::AppState;
use mxr_core::types::system_labels;
use mxr_rules::{Conditions, FieldCondition, Rule, RuleAction, StringMatch};
use mxr_search::parse_query;

pub(super) fn parse_rule_value(value: serde_json::Value) -> Result<Rule, String> {
    serde_json::from_value(value).map_err(|e| e.to_string())
}

pub(super) async fn build_rule_from_form(
    state: &AppState,
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
            ?
            .map(|row| {
                serde_json::from_value::<Rule>(mxr_store::row_to_rule_json(&row))
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
        actions: parse_rule_actions_string(action)?,
        created_at: existing.as_ref().map_or(now, |rule| rule.created_at),
        updated_at: now,
    })
}

fn parse_rule_condition_string(input: &str) -> Result<Conditions, String> {
    let ast = parse_query(input)?;
    query_ast_to_conditions(ast)
}

fn query_ast_to_conditions(node: mxr_search::ast::QueryNode) -> Result<Conditions, String> {
    use mxr_search::ast::{DateBound, DateValue, FilterKind, QueryField, QueryNode, SizeOp};

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
            QueryField::Cc
            | QueryField::Bcc
            | QueryField::Filename
            | QueryField::List
            | QueryField::DeliveredTo
            | QueryField::Rfc822MsgId => {
                return Err("field is not supported in rules form".to_string())
            }
            _ => return Err("unknown field is not supported in rules form".to_string()),
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
            label: system_labels::DRAFT.to_string(),
        }),
        QueryNode::Filter(FilterKind::Sent) => Conditions::Field(FieldCondition::HasLabel {
            label: system_labels::SENT.to_string(),
        }),
        QueryNode::Filter(FilterKind::Trash) => Conditions::Field(FieldCondition::HasLabel {
            label: system_labels::TRASH.to_string(),
        }),
        QueryNode::Filter(FilterKind::Spam) => Conditions::Field(FieldCondition::HasLabel {
            label: system_labels::SPAM.to_string(),
        }),
        QueryNode::Filter(FilterKind::Inbox) => Conditions::Field(FieldCondition::HasLabel {
            label: system_labels::INBOX.to_string(),
        }),
        QueryNode::Filter(FilterKind::Archived) => Conditions::Field(FieldCondition::HasLabel {
            label: system_labels::ARCHIVE.to_string(),
        }),
        QueryNode::Filter(
            FilterKind::Answered
            | FilterKind::Anywhere
            | FilterKind::HasUserLabels
            | FilterKind::NoUserLabels
            | FilterKind::HasDrive
            | FilterKind::HasDocument
            | FilterKind::HasSpreadsheet
            | FilterKind::HasPresentation
            | FilterKind::HasYoutube
            | FilterKind::HasInlineImage,
        ) => {
            return Err("search filter is not supported in rules form".to_string())
        }
        QueryNode::Filter(FilterKind::Custom(_)) => {
            return Err("custom search filters are not supported in rules form".to_string())
        }
        // Defensive: HasCalendar/HasLink/HasLinkHeavy/NoLinks weren't
        // in the original guard list; treat as unsupported for now.
        QueryNode::Filter(_) => {
            return Err("search filter is not supported in rules form".to_string())
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
                _ => return Err("unknown date bound is not supported in rules form".to_string()),
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
            _ => return Err("unknown size op is not supported in rules form".to_string()),
        },
        QueryNode::Near { .. } => {
            return Err("AROUND is not supported in rules form".to_string())
        }
        QueryNode::Exact(_) => {
            return Err("+word exact-match is not supported in rules form".to_string())
        }
        _ => return Err("unknown query node is not supported in rules form".to_string()),
    })
}

pub(super) fn parse_rule_actions_string(value: &str) -> Result<Vec<RuleAction>, String> {
    let actions = value
        .split([',', ';'])
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(parse_rule_action_string)
        .collect::<Result<Vec<_>, _>>()?;
    if actions.is_empty() {
        return Err("rule action list is empty".to_string());
    }
    Ok(actions)
}

pub(super) fn parse_rule_action_string(value: &str) -> Result<RuleAction, String> {
    let trimmed = value.trim();
    let lower = trimmed.to_ascii_lowercase();
    if lower == "archive" {
        return Ok(RuleAction::Archive);
    }
    if lower == "trash" {
        return Ok(RuleAction::Trash);
    }
    if lower == "star" {
        return Ok(RuleAction::Star);
    }
    if matches!(lower.as_str(), "mark-read" | "read") {
        return Ok(RuleAction::MarkRead);
    }
    if matches!(lower.as_str(), "mark-unread" | "unread") {
        return Ok(RuleAction::MarkUnread);
    }
    if let Some(label) = strip_action_prefix(trimmed, "add-label:")
        .or_else(|| strip_action_prefix(trimmed, "label:"))
    {
        let label = label.trim();
        if label.is_empty() {
            return Err("label action requires a label".to_string());
        }
        return Ok(RuleAction::AddLabel {
            label: label.to_string(),
        });
    }
    if let Some(label) = strip_action_prefix(trimmed, "remove-label:")
        .or_else(|| strip_action_prefix(trimmed, "unlabel:"))
    {
        let label = label.trim();
        if label.is_empty() {
            return Err("remove-label action requires a label".to_string());
        }
        return Ok(RuleAction::RemoveLabel {
            label: label.to_string(),
        });
    }
    if let Some(command) = strip_action_prefix(trimmed, "shell:") {
        let command = command.trim();
        if command.is_empty() {
            return Err("shell action requires a command".to_string());
        }
        return Ok(RuleAction::ShellHook {
            command: command.to_string(),
        });
    }
    Err(format!("Unsupported action: {value}"))
}

fn strip_action_prefix<'a>(value: &'a str, prefix: &str) -> Option<&'a str> {
    value
        .get(..prefix.len())
        .is_some_and(|head| head.eq_ignore_ascii_case(prefix))
        .then(|| &value[prefix.len()..])
}

pub(super) fn rule_to_form_data(rule: &Rule) -> Result<mxr_protocol::RuleFormData, String> {
    let action = rule_actions_to_string(&rule.actions)?;
    Ok(mxr_protocol::RuleFormData {
        id: Some(rule.id.to_string()),
        name: rule.name.clone(),
        condition: conditions_to_query(&rule.conditions)?,
        action,
        priority: rule.priority,
        enabled: rule.enabled,
    })
}

pub(super) fn rule_actions_to_string(actions: &[RuleAction]) -> Result<String, String> {
    if actions.is_empty() {
        return Err("rule has no actions".to_string());
    }
    actions
        .iter()
        .map(rule_action_to_string)
        .collect::<Result<Vec<_>, _>>()
        .map(|parts| parts.join(","))
}

pub(super) fn rule_action_to_string(action: &RuleAction) -> Result<String, String> {
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

pub(super) fn conditions_to_query(conditions: &Conditions) -> Result<String, String> {
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
