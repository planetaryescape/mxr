//! Opt-in native state packets for source/authority and projection regressions.
use crate::state::AppState;
use mxr_core::AccountId;

pub(super) async fn trace(state: &AppState, account: &AccountId, stage: &str) {
    if std::env::var_os("MXR_CLASSIFY_TRACE").is_none() {
        return;
    }
    for (kind, sql) in [
        ("todos", "SELECT json_object('id',id,'source',source_message_id,'key',dedup_key,'state',state,'kind',kind,'due',due_at,'expired_at_birth',expired_at_birth,'commitment',commitment_id,'edited',user_edited,'fields',json(field_sources)) FROM todos WHERE account_id=? LIMIT 64"),
        ("commitments", "SELECT json_object('id',id,'source',evidence_msg_id,'direction',direction,'status',status,'what',what,'due',by_when,'date_source',by_when_source) FROM contact_commitments WHERE account_id=? LIMIT 64"),
        ("asks", "SELECT assessment_json FROM mail_assessments WHERE account_id=? AND policy_version LIKE 'asks-%' LIMIT 32"),
        ("generations", "SELECT json_object('thread',thread_id,'generation',effective_revision,'dirty',dirty) FROM assessment_effective_thread_inputs WHERE account_id=? LIMIT 64"),
        ("outbox", "SELECT json_object('source',message_id,'generation',input_revision,'fingerprint',input_fingerprint,'context',context_token) FROM asks_projection_outbox WHERE account_id=? LIMIT 64"),
    ] {
        let rows: Vec<String> = sqlx::query_scalar(sqlx::AssertSqlSafe(sql)).bind(account.as_str()).fetch_all(state.store.reader()).await.expect("native projection diagnostic packet");
        eprintln!("projection {stage} account={account} {kind}: {rows:?}");
    }
}
