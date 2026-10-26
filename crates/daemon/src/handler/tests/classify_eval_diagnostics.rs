//! Bounded diagnostics for synthetic stored-output grading failures.
use super::*;
use mxr_protocol::{ClassifyEvalMismatchData, ClassifyEvalSummaryData};
use std::collections::BTreeMap;

/// Synthetic test diagnostics must survive a floor panic. Sample distinct ask
/// mismatch shapes, then read their actual stored facets and projections.
pub(super) async fn emit_ask_diagnostics(
    state: &AppState,
    summary: &ClassifyEvalSummaryData,
    mismatches: &[ClassifyEvalMismatchData],
) {
    if let Some(asks) = &summary.asks {
        eprintln!("{}", classify_eval_floor::report(asks));
    }
    let mut clusters = BTreeMap::new();
    for mismatch in mismatches.iter().filter(|m| m.question == "ask") {
        clusters
            .entry((&mismatch.scenario, &mismatch.expected, &mismatch.actual))
            .or_insert(mismatch);
    }
    eprintln!(
        "ask diagnostic clusters: {} (at most 32 sampled)",
        clusters.len()
    );
    let providers: Vec<_> = clusters
        .values()
        .take(32)
        .map(|m| m.provider_id.as_str())
        .collect();
    for mismatch in clusters.values().take(32) {
        eprintln!("ask mismatch {}", serde_json::to_string(mismatch).unwrap());
    }
    let ids: Vec<_> = state
        .store
        .classify_eval_candidates(&[], &providers)
        .await
        .unwrap()
        .into_iter()
        .map(|c| c.message_id)
        .collect();
    for entry in state
        .store
        .classify_eval_packet(&ids)
        .await
        .unwrap()
        .into_iter()
        .take(64)
    {
        let units: Vec<_> = entry
            .asks
            .as_ref()
            .into_iter()
            .flat_map(|a| {
                a.units.iter().take(8).map(|u| {
                    serde_json::json!({"answer": u.answer, "tags": u.tags,
                "zone": a.facts.zones[u.zone].kind,
                "quote": a.quote(u).map(|q| q.chars().take(320).collect::<String>())})
                })
            })
            .collect();
        eprintln!(
            "stored ask diagnostic {}",
            serde_json::json!({
                "account": entry.envelope.account_id, "message": entry.envelope.id,
                "provider": entry.envelope.provider_id, "from": entry.envelope.from,
                "subject": entry.envelope.subject, "units": units,
                "policy": entry.asks.as_ref().map(|a| &a.policy_version),
                "fingerprint": entry.asks.as_ref().map(|a| &a.input_fingerprint),
                "projection_pending": entry.projection_pending,
                "todos": entry.projected_todos.iter().take(8).map(|t| serde_json::json!({
                    "unit": t.unit_provenance, "todo": format!("{:?}", t.todo),
                    "user_edited": t.user_edited,
                })).collect::<Vec<_>>(),
            })
        );
    }
}
