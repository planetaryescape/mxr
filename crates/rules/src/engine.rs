use crate::condition::MessageView;
use crate::{Rule, RuleAction, RuleId};
use serde::Serialize;

/// The rule engine: evaluates rules against messages.
pub struct RuleEngine {
    rules: Vec<Rule>,
}

/// Result of evaluating all rules against a single message.
#[derive(Debug, Clone, Serialize)]
pub struct EvaluationResult {
    pub message_id: String,
    pub actions: Vec<RuleAction>,
    pub matched_rules: Vec<RuleId>,
}

/// Result of a dry-run evaluation.
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
        // Sort by priority (lower = first)
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

    /// Evaluate all enabled rules against a batch of messages.
    pub fn evaluate_batch(&self, messages: &[(&dyn MessageView, &str)]) -> Vec<EvaluationResult> {
        messages
            .iter()
            .map(|(msg, id)| self.evaluate(*msg, id))
            .filter(|r| !r.actions.is_empty())
            .collect()
    }

    /// Dry-run: evaluate a specific rule against messages without applying actions.
    pub fn dry_run(
        &self,
        rule_id: &RuleId,
        messages: &[(&dyn MessageView, &str, &str, &str)], // (msg, id, from, subject)
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

// Keep the existing conservative sorting policy, including mixed-action rules,
// identical in live evaluation and explicit previews of disabled rules.
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
    fn explicitly_selected_disabled_rule_can_be_previewed() {
        let mut rule = archive_newsletters_rule();
        rule.enabled = false;
        let id = rule.id.clone();
        let engine = RuleEngine::new(vec![rule]);
        let msg = newsletter_msg();
        assert!(engine.evaluate(&msg, "fixture").actions.is_empty());
        assert_eq!(
            engine
                .dry_run(&id, &[(&msg, "fixture", "sender", "subject")])
                .unwrap()
                .matches
                .len(),
            1
        );
    }

    #[test]
    fn engine_accumulates_actions_from_multiple_matching_rules() {
        let engine = RuleEngine::new(vec![archive_newsletters_rule(), mark_read_unsub_rule()]);
        let msg = newsletter_msg();
        let result = engine.evaluate(&msg, "msg_1");

        assert_eq!(result.actions.len(), 2);
        assert_eq!(result.matched_rules.len(), 2);
    }

    #[test]
    fn engine_returns_empty_when_no_rules_match() {
        let engine = RuleEngine::new(vec![archive_newsletters_rule()]);
        let msg = work_email_msg();
        let result = engine.evaluate(&msg, "msg_1");

        assert!(result.actions.is_empty());
        assert!(result.matched_rules.is_empty());
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

        // Insert in wrong order — engine should sort by priority
        let engine = RuleEngine::new(vec![low, high]);
        let msg = newsletter_msg();
        let result = engine.evaluate(&msg, "msg_1");

        // High priority (Archive) should come first
        assert!(matches!(result.actions[0], RuleAction::Archive));
        assert!(matches!(result.actions[1], RuleAction::MarkRead));
    }

    #[test]
    fn batch_evaluate_filters_non_matching() {
        let engine = RuleEngine::new(vec![archive_newsletters_rule()]);
        let nl = newsletter_msg();
        let work = work_email_msg();

        let messages: Vec<(&dyn MessageView, &str)> =
            vec![(&nl as &dyn MessageView, "msg_1"), (&work, "msg_2")];

        let results = engine.evaluate_batch(&messages);

        // Only the newsletter should match
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].message_id, "msg_1");
    }

    #[test]
    fn batch_evaluate_returns_empty_for_no_matches() {
        let engine = RuleEngine::new(vec![archive_newsletters_rule()]);
        let work = work_email_msg();

        let messages: Vec<(&dyn MessageView, &str)> = vec![(&work as &dyn MessageView, "msg_1")];

        let results = engine.evaluate_batch(&messages);
        assert!(results.is_empty());
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

    #[test]
    fn dry_run_returns_none_for_unknown_rule() {
        let engine = RuleEngine::new(vec![archive_newsletters_rule()]);
        let result = engine.dry_run(&RuleId("nonexistent".into()), &[]);
        assert!(result.is_none());
    }

    #[test]
    fn engine_preserves_message_id_in_result() {
        let engine = RuleEngine::new(vec![archive_newsletters_rule()]);
        let msg = newsletter_msg();
        let result = engine.evaluate(&msg, "specific_msg_id_123");
        assert_eq!(result.message_id, "specific_msg_id_123");
    }

    #[test]
    fn multiple_actions_per_rule() {
        let rule = Rule {
            account_id: None,
            id: RuleId("r_multi".into()),
            name: "Multi-action rule".into(),
            enabled: true,
            priority: 1,
            conditions: Conditions::Field(FieldCondition::HasUnsubscribe),
            actions: vec![
                RuleAction::Archive,
                RuleAction::MarkRead,
                RuleAction::AddLabel {
                    label: "auto-archived".into(),
                },
            ],
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let engine = RuleEngine::new(vec![rule]);
        let msg = newsletter_msg();
        let result = engine.evaluate(&msg, "msg_1");

        assert_eq!(result.actions.len(), 3);
    }
    #[test]
    fn sorting_winner_is_stable_and_account_scoped() {
        let account = mxr_core::AccountId::new();
        let other = mxr_core::AccountId::new();
        let mut a = archive_newsletters_rule();
        a.id = RuleId("a".into());
        a.account_id = Some(account.clone());
        a.actions = vec![RuleAction::SetTreatment {
            treatment: crate::Treatment::Reading,
        }];
        let mut b = a.clone();
        b.id = RuleId("b".into());
        b.actions = vec![
            RuleAction::SetTreatment {
                treatment: crate::Treatment::Messages,
            },
            RuleAction::Star,
        ];
        for rules in [vec![a.clone(), b.clone()], vec![b.clone(), a.clone()]] {
            let engine = RuleEngine::new(rules);
            assert_eq!(
                engine
                    .treatment(&newsletter_msg(), &account)
                    .unwrap()
                    .0
                    .id
                    .0,
                "a"
            );
            assert!(engine.treatment(&newsletter_msg(), &other).is_none());
            assert_eq!(engine.evaluate(&newsletter_msg(), "id").actions.len(), 3);
        }
        a.enabled = false;
        assert_eq!(
            RuleEngine::new(vec![a, b])
                .treatment(&newsletter_msg(), &account)
                .unwrap()
                .0
                .id
                .0,
            "b"
        );
    }

    #[test]
    fn unfetched_body_does_not_match_under_not_or_trigger_mixed_actions() {
        let mut message = newsletter_msg();
        message.body = None;
        let mut rule = archive_newsletters_rule();
        rule.account_id = Some(mxr_core::AccountId::new());
        rule.conditions = Conditions::Not {
            condition: Box::new(Conditions::Field(FieldCondition::BodyContains {
                pattern: StringMatch::Contains("sale".into()),
            })),
        };
        rule.actions = vec![
            RuleAction::SetTreatment {
                treatment: crate::Treatment::Reading,
            },
            RuleAction::Archive,
        ];
        let account = rule.account_id.clone().unwrap();
        let engine = RuleEngine::new(vec![rule]);
        assert!(engine.treatment(&message, &account).is_none());
        assert!(engine.evaluate(&message, "id").actions.is_empty());
        message.body = Some("ordinary newsletter".into());
        assert!(engine.treatment(&message, &account).is_some());
    }
    #[test]
    fn equal_priority_ordinary_actions_retain_storage_order() {
        let mut first = archive_newsletters_rule();
        first.id = RuleId("z-first".into());
        first.actions = vec![RuleAction::AddLabel {
            label: "first".into(),
        }];
        let mut second = first.clone();
        second.id = RuleId("a-second".into());
        second.actions = vec![RuleAction::AddLabel {
            label: "second".into(),
        }];
        let engine = RuleEngine::new(vec![first, second]);
        let result = engine.evaluate(&newsletter_msg(), "id");
        let labels: Vec<_> = result
            .actions
            .iter()
            .filter_map(|a| match a {
                RuleAction::AddLabel { label } => Some(label.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(labels, vec!["first", "second"]);
    }
}
