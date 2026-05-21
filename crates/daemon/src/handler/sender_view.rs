//! Sender profile: per-(account, email) relationship aggregates.

use super::{relationship_profile, HandlerResult};
use crate::state::AppState;
use mxr_core::id::AccountId;
use mxr_protocol::{
    ResponseData, SenderEmailReferenceData, SenderProfileData, SenderUnansweredQuestionData,
    SenderWeeklyActivityData,
};

pub(super) async fn get_sender_profile(
    state: &AppState,
    account_id: &AccountId,
    email: &str,
) -> HandlerResult {
    let profile = state
        .store
        .get_sender_profile(account_id, email)
        .await
        .map_err(|e| e.to_string())?;
    let data = match profile {
        Some(p) => {
            let relationship =
                relationship_profile::load_relationship_profile(state, account_id, email).await?;
            let recent_messages = state
                .store
                .list_recent_sender_messages(account_id, email, 12)
                .await
                .map_err(|e| e.to_string())?
                .into_iter()
                .map(|message| SenderEmailReferenceData {
                    message_id: message.message_id,
                    thread_id: message.thread_id,
                    subject: message.subject,
                    snippet: message.snippet,
                    from_name: message.from_name,
                    from_email: message.from_email,
                    date: message.date,
                    direction: message.direction,
                    has_attachments: message.has_attachments,
                })
                .collect();
            Some(SenderProfileData {
                account_id: p.account_id,
                email: p.email,
                display_name: p.display_name,
                first_seen_at: p.first_seen_at,
                last_seen_at: p.last_seen_at,
                last_inbound_at: p.last_inbound_at,
                last_outbound_at: p.last_outbound_at,
                total_inbound: p.total_inbound,
                total_outbound: p.total_outbound,
                replied_count: p.replied_count,
                cadence_days_p50: p.cadence_days_p50,
                is_list_sender: p.is_list_sender,
                list_id: p.list_id,
                open_thread_count: p.open_thread_count,
                inbound_storage_bytes: p.inbound_storage_bytes,
                outbound_storage_bytes: p.outbound_storage_bytes,
                attachment_count: p.attachment_count,
                attachment_bytes: p.attachment_bytes,
                unanswered_question: p.unanswered_question.map(|question| {
                    SenderUnansweredQuestionData {
                        message_id: question.message_id,
                        thread_id: question.thread_id,
                        subject: question.subject,
                        received_at: question.received_at,
                        days_waiting: question.days_waiting,
                    }
                }),
                response_histogram: p.response_histogram,
                weekly_activity: p
                    .weekly_activity
                    .into_iter()
                    .map(|week| SenderWeeklyActivityData {
                        week_start: week.week_start,
                        inbound_count: week.inbound_count,
                        outbound_count: week.outbound_count,
                    })
                    .collect(),
                recent_messages,
                relationship,
            })
        }
        None => None,
    };
    Ok(ResponseData::SenderProfile { profile: data })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_fixtures::TestEnvelopeBuilder;
    use mxr_core::types::MessageDirection;
    use mxr_store::ContactRelationshipSummaryRecord;

    #[tokio::test]
    async fn sender_profile_includes_relationship_profile_when_available() {
        let state = AppState::in_memory().await.unwrap();
        let account_id = state.default_account_id();
        let message = TestEnvelopeBuilder::new()
            .account_id(account_id.clone())
            .provider_id("customer-inbound")
            .sender_address("Customer", "customer@example.com")
            .subject("Pricing rollout")
            .snippet("Can you clarify pricing timing?")
            .build();
        state
            .store
            .upsert_envelope_with_direction(&message, MessageDirection::Inbound)
            .await
            .unwrap();
        state.store.refresh_contacts().await.unwrap();
        state
            .store
            .upsert_contact_relationship_summary(&ContactRelationshipSummaryRecord {
                account_id: account_id.clone(),
                email: "customer@example.com".to_string(),
                text: "Customer prefers short pricing updates.".to_string(),
                model: "test-model".to_string(),
                known_topics: vec!["pricing".to_string()],
                computed_at: chrono::Utc::now(),
                source_hash: "relationship-v1".to_string(),
                last_error: None,
            })
            .await
            .unwrap();

        let response = get_sender_profile(&state, &account_id, "CUSTOMER@example.com")
            .await
            .unwrap();

        let ResponseData::SenderProfile {
            profile: Some(profile),
        } = response
        else {
            panic!("expected sender profile");
        };
        let relationship = profile.relationship.expect("relationship profile");
        let summary = relationship.summary.expect("relationship summary");
        assert_eq!(relationship.email, "customer@example.com");
        assert_eq!(summary.text, "Customer prefers short pricing updates.");
        assert_eq!(summary.known_topics, vec!["pricing"]);
    }
}
