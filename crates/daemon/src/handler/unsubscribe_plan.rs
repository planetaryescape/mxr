//! Plans the unsubscribe requests for the messages an email address selected.
//!
//! An address sends one request per distinct List-Unsubscribe target. A
//! target is the URL of a one-click or http(s) link, or the address of a
//! mailto. Each request uses the newest message that carries its target, so a
//! sender with hundreds of messages gets one request, not hundreds. A sender
//! whose messages name several lists gets one request per list.
//!
//! The dry run and the real run both call this plan, so the preview is the
//! set of requests the real run sends.

use super::HandlerResult;
use crate::state::AppState;
use mxr_core::id::MessageId;
use mxr_core::types::{Envelope, UnsubscribeMethod};
use mxr_protocol::{ResponseData, UnsubscribePlanData, UnsubscribePlanRequestData};
use mxr_store::UNSUBSCRIBE_PENDING_METHOD;
use std::collections::HashSet;

pub(super) async fn plan_unsubscribe(state: &AppState, message_ids: &[MessageId]) -> HandlerResult {
    let mut envelopes = Vec::with_capacity(message_ids.len());
    for id in message_ids {
        if let Some(envelope) = state.store.get_envelope(id).await? {
            envelopes.push(envelope);
        }
    }

    let mut plan = UnsubscribePlanData {
        requests: Vec::new(),
        already_unsubscribed: 0,
    };
    for envelope in newest_per_target(envelopes) {
        match state.store.unsubscribe_receipt_method(&envelope.id).await? {
            // Confirmed: this target was already unsubscribed, so nothing is sent.
            Some(method) if method != UNSUBSCRIBE_PENDING_METHOD => {
                plan.already_unsubscribed += 1;
            }
            // No receipt, or an attempt still in flight. An in-flight target stays
            // in the plan so the real run reports it as in progress, as it does today.
            _ => plan.requests.push(UnsubscribePlanRequestData {
                message_id: envelope.id,
                method: envelope.unsubscribe,
            }),
        }
    }
    Ok(ResponseData::UnsubscribePlan { plan })
}

/// Keeps the newest envelope for each distinct target, newest first. Envelopes
/// without a usable method carry no target and are dropped.
fn newest_per_target(mut envelopes: Vec<Envelope>) -> Vec<Envelope> {
    envelopes.sort_by(|a, b| {
        b.date
            .cmp(&a.date)
            .then_with(|| a.id.as_str().cmp(&b.id.as_str()))
    });
    let mut seen = HashSet::new();
    envelopes
        .into_iter()
        .filter(|envelope| target_key(&envelope.unsubscribe).is_some_and(|key| seen.insert(key)))
        .collect()
}

fn target_key(method: &UnsubscribeMethod) -> Option<String> {
    match method {
        UnsubscribeMethod::OneClick { url }
        | UnsubscribeMethod::HttpLink { url }
        | UnsubscribeMethod::BodyLink { url } => Some(format!("url:{url}")),
        // Mailto addresses are case-insensitive in practice; one address is one list.
        UnsubscribeMethod::Mailto { address, .. } => {
            Some(format!("mailto:{}", address.to_ascii_lowercase()))
        }
        UnsubscribeMethod::None => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn envelope(id: &str, unsubscribe: UnsubscribeMethod, day: i64) -> Envelope {
        let mut envelope = crate::test_fixtures::TestEnvelopeBuilder::new()
            .unsubscribe(unsubscribe)
            .date(chrono::DateTime::from_timestamp(day * 86_400, 0).unwrap())
            .build();
        envelope.id = MessageId::from_uuid(uuid::Uuid::parse_str(id).unwrap());
        envelope
    }

    fn url(value: &str) -> UnsubscribeMethod {
        UnsubscribeMethod::OneClick {
            url: value.to_owned(),
        }
    }

    #[test]
    fn one_target_across_many_messages_keeps_only_the_newest() {
        let envelopes = vec![
            envelope(
                "00000000-0000-0000-0000-000000000001",
                url("https://a/u"),
                1,
            ),
            envelope(
                "00000000-0000-0000-0000-000000000002",
                url("https://a/u"),
                3,
            ),
            envelope(
                "00000000-0000-0000-0000-000000000003",
                url("https://a/u"),
                2,
            ),
        ];
        let kept = newest_per_target(envelopes);
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].id.as_str(), "00000000-0000-0000-0000-000000000002");
    }

    #[test]
    fn distinct_targets_each_keep_their_newest_message() {
        let envelopes = vec![
            envelope(
                "00000000-0000-0000-0000-000000000001",
                url("https://a/one"),
                1,
            ),
            envelope(
                "00000000-0000-0000-0000-000000000002",
                url("https://a/two"),
                2,
            ),
            envelope(
                "00000000-0000-0000-0000-000000000003",
                url("https://a/one"),
                5,
            ),
        ];
        let kept = newest_per_target(envelopes);
        let ids: Vec<String> = kept.iter().map(|e| e.id.as_str()).collect();
        assert_eq!(
            ids,
            vec![
                "00000000-0000-0000-0000-000000000003".to_owned(),
                "00000000-0000-0000-0000-000000000002".to_owned(),
            ]
        );
    }

    #[test]
    fn messages_without_a_method_are_not_targets() {
        let envelopes = vec![
            envelope(
                "00000000-0000-0000-0000-000000000001",
                UnsubscribeMethod::None,
                9,
            ),
            envelope(
                "00000000-0000-0000-0000-000000000002",
                url("https://a/u"),
                1,
            ),
        ];
        let kept = newest_per_target(envelopes);
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].id.as_str(), "00000000-0000-0000-0000-000000000002");
    }

    #[test]
    fn mailto_targets_match_case_insensitively_and_ignore_subject() {
        let envelopes = vec![
            envelope(
                "00000000-0000-0000-0000-000000000001",
                UnsubscribeMethod::Mailto {
                    address: "Unsub@Example.com".into(),
                    subject: Some("old".into()),
                },
                1,
            ),
            envelope(
                "00000000-0000-0000-0000-000000000002",
                UnsubscribeMethod::Mailto {
                    address: "unsub@example.com".into(),
                    subject: None,
                },
                2,
            ),
        ];
        assert_eq!(newest_per_target(envelopes).len(), 1);
    }

    #[test]
    fn a_one_click_url_and_an_http_link_with_the_same_url_are_one_target() {
        let envelopes = vec![
            envelope(
                "00000000-0000-0000-0000-000000000001",
                UnsubscribeMethod::HttpLink {
                    url: "https://a/u".into(),
                },
                1,
            ),
            envelope(
                "00000000-0000-0000-0000-000000000002",
                url("https://a/u"),
                2,
            ),
        ];
        assert_eq!(newest_per_target(envelopes).len(), 1);
    }
}
