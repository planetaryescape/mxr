use crate::condition::MessageView;
use crate::{Rule, RuleAction, RuleId};
use serde::Serialize;

pub struct RuleEngine {
    rules: Vec<Rule>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EvaluationResult {
    pub message_id: String,
    pub actions: Vec<RuleAction>,
    pub matched_rules: Vec<RuleId>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DryRunResult {
    pub rule_id: RuleId,
    pub rule_name: String,
    pub matches: Vec<DryRunMatch>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DryRunMatch {
    pub message_id: String,
    pub from: String,
    pub subject: String,
    pub actions: Vec<RuleAction>,
}

impl RuleEngine {
    pub fn new(mut rules: Vec<Rule>) -> Self {
        rules.sort_by_key(|rule| rule.priority);
        Self { rules }
    }

    /// Automatic sorting is account-bound and never becomes a personal move.
    pub fn treatment<'a>(
        &'a self,
        msg: &dyn MessageView,
        account_id: &mxr_core::AccountId,
    ) -> Option<(&'a Rule, crate::Treatment)> {
        self.rules
            .iter()
            .filter(|rule| rule.enabled && rule.account_id.as_ref() == Some(account_id))
            .filter_map(|rule| {
                if rule.conditions.evaluate_known(msg) != Some(true) {
                    return None;
                }
                rule.actions.iter().find_map(|action| match action {
                    RuleAction::SetTreatment { treatment } => Some((rule, *treatment)),
                    _ => None,
                })
            })
            .min_by(|(a, _), (b, _)| {
                a.priority
                    .cmp(&b.priority)
                    .then_with(|| a.id.0.cmp(&b.id.0))
            })
    }

    pub fn rules(&self) -> &[Rule] {
        &self.rules
    }

    /// Evaluate all enabled rules against a message.
    /// Returns accumulated actions. Actions are NOT applied yet —
    /// the caller is responsible for executing them.
    pub fn evaluate(&self, msg: &dyn MessageView, message_id: &str) -> EvaluationResult {
        let mut actions = Vec::new();
        let mut matched_rules = Vec::new();

        for rule in &self.rules {
            if !rule.enabled {
                continue;
            }
            if rule_matches(rule, msg) {
                tracing::debug!(
                    rule_name = %rule.name,
                    message_id = %message_id,
                    "Rule matched"
                );
                actions.extend(rule.actions.clone());
                matched_rules.push(rule.id.clone());
            }
        }

        EvaluationResult {
            message_id: message_id.to_string(),
            actions,
            matched_rules,
        }
    }

    pub fn evaluate_batch(&self, messages: &[(&dyn MessageView, &str)]) -> Vec<EvaluationResult> {
        messages
            .iter()
            .map(|(msg, id)| self.evaluate(*msg, id))
            .filter(|r| !r.actions.is_empty())
            .collect()
    }

    pub fn dry_run(
        &self,
        rule_id: &RuleId,
        messages: &[(&dyn MessageView, &str, &str, &str)],
    ) -> Option<DryRunResult> {
        let rule = self.rules.iter().find(|r| &r.id == rule_id)?;

        let matches: Vec<DryRunMatch> = messages
            .iter()
            .filter(|(msg, _, _, _)| rule_matches(rule, *msg))
            .map(|(_, id, from, subject)| DryRunMatch {
                message_id: id.to_string(),
                from: from.to_string(),
                subject: subject.to_string(),
                actions: rule.actions.clone(),
            })
            .collect();

        Some(DryRunResult {
            rule_id: rule.id.clone(),
            rule_name: rule.name.clone(),
            matches,
        })
    }
}

fn rule_matches(rule: &Rule, msg: &dyn MessageView) -> bool {
    if rule
        .actions
        .iter()
        .any(|action| matches!(action, RuleAction::SetTreatment { .. }))
    {
        rule.conditions.evaluate_known(msg) == Some(true)
    } else {
        rule.conditions.evaluate(msg)
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap fixture setup for direct failures"
    )]

    use super::*;
    use crate::action::RuleAction;
    use crate::condition::*;
    use crate::tests::*;
    use crate::{Rule, RuleId};
    use chrono::Utc;

    fn archive_newsletters_rule() -> Rule {
        Rule {
            account_id: None,
            id: RuleId("r_archive".into()),
            name: "Archive newsletters".into(),
            enabled: true,
            priority: 10,
            conditions: Conditions::Field(FieldCondition::HasLabel {
                label: "newsletters".into(),
            }),
            actions: vec![RuleAction::Archive],
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    fn mark_read_unsub_rule() -> Rule {
        Rule {
            account_id: None,
            id: RuleId("r_markread".into()),
            name: "Mark read if unsubscribe available".into(),
            enabled: true,
            priority: 20,
            conditions: Conditions::Field(FieldCondition::HasUnsubscribe),
            actions: vec![RuleAction::MarkRead],
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn preview_and_live_share_action_dependent_missing_body_policy() {
        let body = Conditions::Field(FieldCondition::BodyContains {
            pattern: StringMatch::Contains("sale".into()),
        });
        let not_body = Conditions::Not {
            condition: Box::new(body.clone()),
        };
        let cases = vec![
            (not_body.clone(), [true, true, false], [false, true, false]),
            (
                Conditions::And {
                    conditions: vec![
                        Conditions::Field(FieldCondition::HasUnsubscribe),
                        not_body.clone(),
                    ],
                },
                [true, true, false],
                [false, true, false],
            ),
            (
                Conditions::Or {
                    conditions: vec![
                        Conditions::Field(FieldCondition::IsStarred),
                        not_body.clone(),
                    ],
                },
                [true, true, false],
                [false, true, false],
            ),
            (
                Conditions::Not {
                    condition: Box::new(Conditions::Or {
                        conditions: vec![body, Conditions::Field(FieldCondition::IsStarred)],
                    }),
                },
                [true, true, false],
                [false, true, false],
            ),
            (
                Conditions::Or {
                    conditions: vec![
                        Conditions::Field(FieldCondition::HasUnsubscribe),
                        not_body.clone(),
                    ],
                },
                [true, true, true],
                [true, true, true],
            ),
            (
                Conditions::And {
                    conditions: vec![Conditions::Field(FieldCondition::IsStarred), not_body],
                },
                [false, false, false],
                [false, false, false],
            ),
        ];
        for (condition, ordinary_expected, sorting_expected) in cases {
            for sorting in [false, true] {
                let mut rule = archive_newsletters_rule();
                rule.conditions = condition.clone();
                if sorting {
                    rule.actions.insert(
                        0,
                        RuleAction::SetTreatment {
                            treatment: crate::Treatment::Reading,
                        },
                    );
                }
                let id = rule.id.clone();
                let engine = RuleEngine::new(vec![rule]);
                for (index, text) in [None, Some(""), Some("sale")].into_iter().enumerate() {
                    let mut msg = newsletter_msg();
                    msg.body = text.map(str::to_string);
                    let expected = if sorting {
                        sorting_expected[index]
                    } else {
                        ordinary_expected[index]
                    };
                    let live = engine.evaluate(&msg, "fixture");
                    let preview = engine
                        .dry_run(&id, &[(&msg, "fixture", "sender", "subject")])
                        .unwrap();
                    assert_eq!(
                        !live.matched_rules.is_empty(),
                        expected,
                        "live sorting={sorting} body={text:?}"
                    );
                    assert_eq!(
                        !preview.matches.is_empty(),
                        expected,
                        "preview sorting={sorting} body={text:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn disabled_rules_are_skipped() {
        let mut rule = archive_newsletters_rule();
        rule.enabled = false;
        let engine = RuleEngine::new(vec![rule]);
        let msg = newsletter_msg();
        let result = engine.evaluate(&msg, "msg_1");

        assert!(result.actions.is_empty());
    }

    #[test]
    fn priority_determines_action_order() {
        let mut low = mark_read_unsub_rule();
        low.priority = 100;
        low.actions = vec![RuleAction::MarkRead];

        let mut high = archive_newsletters_rule();
        high.priority = 1;
        high.conditions = Conditions::Field(FieldCondition::HasUnsubscribe);
        high.actions = vec![RuleAction::Archive];

        let engine = RuleEngine::new(vec![low, high]);
        let msg = newsletter_msg();
        let result = engine.evaluate(&msg, "msg_1");

        assert!(matches!(result.actions[0], RuleAction::Archive));
        assert!(matches!(result.actions[1], RuleAction::MarkRead));
    }

    #[test]
    fn dry_run_shows_what_would_match() {
        let engine = RuleEngine::new(vec![archive_newsletters_rule()]);
        let nl = newsletter_msg();
        let work = work_email_msg();

        let messages: Vec<(&dyn MessageView, &str, &str, &str)> = vec![
            (
                &nl as &dyn MessageView,
                "msg_1",
                "newsletter@substack.com",
                "This Week in Rust",
            ),
            (&work, "msg_2", "boss@company.com", "Q1 Review"),
        ];

        let result = engine
            .dry_run(&RuleId("r_archive".into()), &messages)
            .unwrap();

        assert_eq!(result.rule_name, "Archive newsletters");
        assert_eq!(result.matches.len(), 1);
        assert_eq!(result.matches[0].message_id, "msg_1");
        assert_eq!(result.matches[0].from, "newsletter@substack.com");
    }
}
