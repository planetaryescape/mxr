use crate::cli::{OutputFormat, RulesAction};
use crate::ipc_client::IpcClient;
use crate::mxr_protocol::*;
use crate::mxr_rules::{Conditions, FieldCondition, Rule, RuleAction, RuleId, StringMatch};
use crate::mxr_search::ast::{FilterKind, QueryField, QueryNode, SizeOp};
use crate::mxr_search::parse_query;
use crate::output::resolve_format;

fn parse_action(value: &str) -> anyhow::Result<RuleAction> {
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
    anyhow::bail!("Unsupported action: {value}")
}

fn query_to_conditions(node: QueryNode) -> anyhow::Result<Conditions> {
    Ok(match node {
        QueryNode::And(left, right) => Conditions::And {
            conditions: vec![query_to_conditions(*left)?, query_to_conditions(*right)?],
        },
        QueryNode::Or(left, right) => Conditions::Or {
            conditions: vec![query_to_conditions(*left)?, query_to_conditions(*right)?],
        },
        QueryNode::Not(node) => Conditions::Not {
            condition: Box::new(query_to_conditions(*node)?),
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
                anyhow::bail!("field is not supported in rules conditions yet")
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
            anyhow::bail!("is:answered is not supported in rules conditions yet")
        }
        QueryNode::Text(value) | QueryNode::Phrase(value) => {
            Conditions::Field(FieldCondition::BodyContains {
                pattern: StringMatch::Contains(value),
            })
        }
        QueryNode::DateRange { bound, date } => {
            let date = match date {
                crate::mxr_search::ast::DateValue::Specific(date) => {
                    chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(
                        date.and_hms_opt(0, 0, 0).unwrap(),
                        chrono::Utc,
                    )
                }
                _ => anyhow::bail!("Relative dates are not supported in rules add yet"),
            };
            match bound {
                crate::mxr_search::ast::DateBound::After => {
                    Conditions::Field(FieldCondition::DateAfter { date })
                }
                crate::mxr_search::ast::DateBound::Before => {
                    Conditions::Field(FieldCondition::DateBefore { date })
                }
                crate::mxr_search::ast::DateBound::Exact => Conditions::And {
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

fn render_rules(rules: &[serde_json::Value], format: OutputFormat) -> anyhow::Result<String> {
    Ok(match format {
        OutputFormat::Json => serde_json::to_string_pretty(rules)?,
        _ => {
            if rules.is_empty() {
                "No rules".to_string()
            } else {
                let mut out = format!(
                    "{:<36} {:<8} {:<8} {}\n",
                    "ID", "ENABLED", "PRIORITY", "NAME"
                );
                out.push_str(&format!("{}\n", "-".repeat(80)));
                for rule in rules {
                    out.push_str(&format!(
                        "{:<36} {:<8} {:<8} {}\n",
                        rule["id"].as_str().unwrap_or(""),
                        rule["enabled"].as_bool().unwrap_or(false),
                        rule["priority"].as_i64().unwrap_or_default(),
                        rule["name"].as_str().unwrap_or(""),
                    ));
                }
                out.trim_end().to_string()
            }
        }
    })
}

async fn get_rule(client: &mut IpcClient, key: String) -> anyhow::Result<serde_json::Value> {
    match client.request(Request::GetRule { rule: key }).await? {
        Response::Ok {
            data: ResponseData::RuleData { rule },
        } => Ok(rule),
        Response::Error { message } => anyhow::bail!("{}", message),
        _ => anyhow::bail!("Unexpected response"),
    }
}

pub async fn run(action: Option<RulesAction>, format: Option<OutputFormat>) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;

    match action.unwrap_or(RulesAction::List) {
        RulesAction::List => match client.request(Request::ListRules).await? {
            Response::Ok {
                data: ResponseData::Rules { rules },
            } => println!("{}", render_rules(&rules, resolve_format(format))?),
            Response::Error { message } => anyhow::bail!("{}", message),
            _ => anyhow::bail!("Unexpected response"),
        },
        RulesAction::Show { rule } => {
            let rule = get_rule(&mut client, rule).await?;
            println!("{}", serde_json::to_string_pretty(&rule)?);
        }
        RulesAction::Add {
            name,
            condition,
            action,
            priority,
        } => {
            let ast = parse_query(&condition).map_err(|e| anyhow::anyhow!(e.to_string()))?;
            let rule = Rule {
                id: RuleId::new(),
                name,
                enabled: true,
                priority,
                conditions: query_to_conditions(ast)?,
                actions: vec![parse_action(&action)?],
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
            };
            let value = serde_json::to_value(rule)?;
            match client.request(Request::UpsertRule { rule: value }).await? {
                Response::Ok {
                    data: ResponseData::RuleData { rule },
                } => println!("{}", rule["id"].as_str().unwrap_or("")),
                Response::Error { message } => anyhow::bail!("{}", message),
                _ => anyhow::bail!("Unexpected response"),
            }
        }
        RulesAction::Edit {
            rule,
            name,
            condition,
            action,
            priority,
            enable,
            disable,
        } => {
            let mut current: Rule = serde_json::from_value(get_rule(&mut client, rule).await?)?;
            if let Some(name) = name {
                current.name = name;
            }
            if let Some(condition) = condition {
                let ast = parse_query(&condition).map_err(|e| anyhow::anyhow!(e.to_string()))?;
                current.conditions = query_to_conditions(ast)?;
            }
            if let Some(action) = action {
                current.actions = vec![parse_action(&action)?];
            }
            if let Some(priority) = priority {
                current.priority = priority;
            }
            if enable {
                current.enabled = true;
            }
            if disable {
                current.enabled = false;
            }
            current.updated_at = chrono::Utc::now();
            let value = serde_json::to_value(current)?;
            match client.request(Request::UpsertRule { rule: value }).await? {
                Response::Ok {
                    data: ResponseData::RuleData { rule },
                } => println!("{}", serde_json::to_string_pretty(&rule)?),
                Response::Error { message } => anyhow::bail!("{}", message),
                _ => anyhow::bail!("Unexpected response"),
            }
        }
        RulesAction::Validate { condition, action } => {
            let ast = parse_query(&condition).map_err(|e| anyhow::anyhow!(e.to_string()))?;
            let conditions = query_to_conditions(ast)?;
            let action = parse_action(&action)?;
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "valid": true,
                    "conditions": conditions,
                    "action": action,
                }))?
            );
        }
        RulesAction::Enable { rule } => {
            let mut current: Rule = serde_json::from_value(get_rule(&mut client, rule).await?)?;
            current.enabled = true;
            current.updated_at = chrono::Utc::now();
            let value = serde_json::to_value(current)?;
            match client.request(Request::UpsertRule { rule: value }).await? {
                Response::Ok {
                    data: ResponseData::RuleData { rule },
                } => println!("{}", rule["enabled"].as_bool().unwrap_or(false)),
                Response::Error { message } => anyhow::bail!("{}", message),
                _ => anyhow::bail!("Unexpected response"),
            }
        }
        RulesAction::Disable { rule } => {
            let mut current: Rule = serde_json::from_value(get_rule(&mut client, rule).await?)?;
            current.enabled = false;
            current.updated_at = chrono::Utc::now();
            let value = serde_json::to_value(current)?;
            match client.request(Request::UpsertRule { rule: value }).await? {
                Response::Ok {
                    data: ResponseData::RuleData { rule },
                } => println!("{}", rule["enabled"].as_bool().unwrap_or(false)),
                Response::Error { message } => anyhow::bail!("{}", message),
                _ => anyhow::bail!("Unexpected response"),
            }
        }
        RulesAction::Delete { rule } => match client.request(Request::DeleteRule { rule }).await? {
            Response::Ok {
                data: ResponseData::Ack,
            } => println!("Deleted"),
            Response::Error { message } => anyhow::bail!("{}", message),
            _ => anyhow::bail!("Unexpected response"),
        },
        RulesAction::DryRun { rule, all, after } => match client
            .request(Request::DryRunRules { rule, all, after })
            .await?
        {
            Response::Ok {
                data: ResponseData::RuleDryRun { results },
            } => println!("{}", serde_json::to_string_pretty(&results)?),
            Response::Error { message } => anyhow::bail!("{}", message),
            _ => anyhow::bail!("Unexpected response"),
        },
        RulesAction::History { rule, limit } => match client
            .request(Request::ListRuleHistory { rule, limit })
            .await?
        {
            Response::Ok {
                data: ResponseData::RuleHistory { entries },
            } => println!("{}", serde_json::to_string_pretty(&entries)?),
            Response::Error { message } => anyhow::bail!("{}", message),
            _ => anyhow::bail!("Unexpected response"),
        },
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_action_archive() {
        assert!(matches!(
            parse_action("archive").unwrap(),
            RuleAction::Archive
        ));
    }

    #[test]
    fn query_to_conditions_label() {
        let ast = parse_query("label:newsletters").unwrap();
        let conditions = query_to_conditions(ast).unwrap();
        match conditions {
            Conditions::Field(FieldCondition::HasLabel { label }) => {
                assert_eq!(label, "newsletters");
            }
            other => panic!("unexpected conditions: {:?}", other),
        }
    }

    #[test]
    fn parse_action_shell_hook() {
        assert!(matches!(
            parse_action("shell:notify-send mxr").unwrap(),
            RuleAction::ShellHook { .. }
        ));
    }
}
