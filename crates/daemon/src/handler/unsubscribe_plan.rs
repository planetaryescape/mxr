//! Plans the one unsubscribe request for the messages an email address selected.
//!
//! An address sends one request. It comes from the newest message with a usable
//! List-Unsubscribe method, preferring one-click, then an http(s) link, then
//! mailto. A body-link fallback is the last resort. A sender that runs several
//! lists is unsubscribed per message id instead.
//!
//! The dry run and the real run both call this plan, so the preview is the
//! request the real run sends.

use super::HandlerResult;
use crate::state::AppState;
use mxr_core::id::MessageId;
use mxr_core::types::{Envelope, UnsubscribeMethod};
use mxr_protocol::{ResponseData, UnsubscribePlanData, UnsubscribePlanRequestData};
use mxr_store::UNSUBSCRIBE_PENDING_METHOD;

pub(super) async fn plan_unsubscribe(state: &AppState, message_ids: &[MessageId]) -> HandlerResult {
    let mut envelopes = Vec::with_capacity(message_ids.len());
    for id in message_ids {
        if let Some(envelope) = state.store.get_envelope(id).await? {
            envelopes.push(envelope);
        }
    }

    let mut plan = UnsubscribePlanData {
        matched_messages: envelopes.len() as u32,
        request: None,
        already_unsubscribed: false,
    };
    let Some(envelope) = choose_message(envelopes) else {
        return Ok(ResponseData::UnsubscribePlan { plan });
    };
    match state.store.unsubscribe_receipt_method(&envelope.id).await? {
        // Confirmed: this sender was already unsubscribed through this message.
        Some(method) if method != UNSUBSCRIBE_PENDING_METHOD => {
            plan.already_unsubscribed = true;
        }
        // No receipt, or an attempt still in flight. An in-flight request stays
        // in the plan so the real run reports it as in progress, as it does today.
        _ => {
            plan.request = Some(UnsubscribePlanRequestData {
                message_id: envelope.id,
                method: envelope.unsubscribe,
            });
        }
    }
    Ok(ResponseData::UnsubscribePlan { plan })
}

/// Lower is preferred. Body links come from scraping the HTML, so they rank last.
fn method_rank(method: &UnsubscribeMethod) -> Option<u8> {
    match method {
        UnsubscribeMethod::OneClick { .. } => Some(0),
        UnsubscribeMethod::HttpLink { .. } => Some(1),
        UnsubscribeMethod::Mailto { .. } => Some(2),
        UnsubscribeMethod::BodyLink { .. } => Some(3),
        UnsubscribeMethod::None => None,
    }
}

/// The newest message whose method ranks best. Ties on rank go to the newest
/// date, then to the lower message id so the choice is stable.
fn choose_message(envelopes: Vec<Envelope>) -> Option<Envelope> {
    envelopes
        .into_iter()
        .filter(|envelope| method_rank(&envelope.unsubscribe).is_some())
        .min_by(|a, b| {
            method_rank(&a.unsubscribe)
                .cmp(&method_rank(&b.unsubscribe))
                .then_with(|| b.date.cmp(&a.date))
                .then_with(|| a.id.as_str().cmp(&b.id.as_str()))
        })
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

    fn one_click(url: &str) -> UnsubscribeMethod {
        UnsubscribeMethod::OneClick { url: url.into() }
    }

    fn mailto(address: &str) -> UnsubscribeMethod {
        UnsubscribeMethod::Mailto {
            address: address.into(),
            subject: None,
        }
    }

    const ONE: &str = "00000000-0000-0000-0000-000000000001";
    const TWO: &str = "00000000-0000-0000-0000-000000000002";
    const THREE: &str = "00000000-0000-0000-0000-000000000003";

    fn chosen(envelopes: Vec<Envelope>) -> Option<String> {
        choose_message(envelopes).map(|e| e.id.as_str())
    }

    #[test]
    fn the_newest_one_click_wins_over_a_newer_mailto() {
        let envelopes = vec![
            envelope(ONE, one_click("https://a/u/1"), 1),
            envelope(TWO, mailto("unsub@a.example"), 9),
        ];
        assert_eq!(chosen(envelopes).as_deref(), Some(ONE));
    }

    #[test]
    fn an_http_link_wins_over_mailto_and_a_body_link_ranks_last() {
        let envelopes = vec![
            envelope(
                ONE,
                UnsubscribeMethod::BodyLink {
                    url: "https://a/body".into(),
                },
                9,
            ),
            envelope(TWO, mailto("unsub@a.example"), 5),
            envelope(
                THREE,
                UnsubscribeMethod::HttpLink {
                    url: "https://a/link".into(),
                },
                1,
            ),
        ];
        assert_eq!(chosen(envelopes).as_deref(), Some(THREE));
    }

    #[test]
    fn among_equal_methods_the_newest_message_wins() {
        let envelopes = vec![
            envelope(ONE, one_click("https://a/u/1"), 1),
            envelope(TWO, one_click("https://a/u/2"), 3),
            envelope(THREE, one_click("https://a/u/3"), 2),
        ];
        assert_eq!(chosen(envelopes).as_deref(), Some(TWO));
    }

    #[test]
    fn messages_without_a_method_are_never_chosen() {
        let envelopes = vec![
            envelope(ONE, UnsubscribeMethod::None, 9),
            envelope(TWO, mailto("unsub@a.example"), 1),
        ];
        assert_eq!(chosen(envelopes).as_deref(), Some(TWO));
        assert_eq!(
            chosen(vec![envelope(ONE, UnsubscribeMethod::None, 1)]),
            None
        );
    }
}
