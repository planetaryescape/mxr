//! Keep recovery identities when a selection changes after a request reached its owner.
use crate::{ClientError, Response, ResponseData};
use mxr_protocol::{ArchiveWorkspaceData, IpcErrorKind, Request};
use serde_json::{json, Value};

pub(super) fn identity(request: &Request, owner: Option<&ArchiveWorkspaceData>) -> Value {
    let mut value = json!({"original_workspace":owner});
    match request {
        Request::SendDraft {
            draft,
            send_identity,
            ..
        } => {
            value["account_id"] = json!(draft.account_id);
            value["draft_id"] = json!(draft.id);
            value["send_identity"] = json!(send_identity);
        }
        Request::SendStoredDraft {
            draft_id,
            send_identity,
            ..
        } => {
            value["draft_id"] = json!(draft_id);
            value["send_identity"] = json!(send_identity);
        }
        Request::PreviewArchive { identity, .. }
        | Request::SubmitArchive { identity, .. }
        | Request::GetArchiveOperation { identity } => {
            value["archive_identity"] = json!(identity);
        }
        Request::StartSelectionJob { job_id, .. } | Request::GetJob { job_id } => {
            value["job_id"] = json!(job_id);
        }
        Request::Mutation {
            client_correlation_id,
            ..
        }
        | Request::StartMutationJob {
            client_correlation_id,
            ..
        } => {
            value["client_correlation_id"] = json!(client_correlation_id);
        }
        _ => (),
    }
    value
}

pub(super) fn interrupted(mut recovery: Value, response: &Response) -> ClientError {
    recovery["response_received"] = json!(true);
    if let Response::Ok { data } = response {
        match data {
            ResponseData::Draft { draft } => {
                recovery["accepted_draft_id"] = json!(draft.id);
                recovery["accepted_draft_revision"] = json!(draft.revision);
            }
            ResponseData::MutationResult { result } => {
                recovery["mutation_id"] = json!(result.mutation_id);
            }
            ResponseData::SendReceipt {
                local_message_id, ..
            } => {
                recovery["sent_message_id"] = json!(local_message_id);
            }
            _ => (),
        }
    }
    ClientError::Daemon {
        message: "Saved connection changed after the request reached its original mailbox. The operation may have applied. Keep its original workspace and operation identity, reopen that mailbox and query the outcome before retrying; never submit an uncertain send as a new send.".into(),
        kind:IpcErrorKind::InvalidRequest,
        code:"selection_changed_after_request".into(),
        retryable:false,
        details:Some(recovery),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn selection_failure_retains_send_identity_without_approval_credentials(
    ) -> Result<(), &'static str> {
        let owner = ArchiveWorkspaceData {
            workspace_id: "original".into(),
            recovery_generation: "epoch".into(),
        };
        let request = Request::SendStoredDraft {
            draft_id: mxr_core::DraftId::new(),
            send_identity: Some(mxr_core::SendIdentity {
                operation_id: "original-operation".into(),
                expected_revision: 7,
            }),
            override_safety_token: Some("private-approval".into()),
            managed_attachments_reviewed: true,
        };
        let error = interrupted(
            identity(&request, Some(&owner)),
            &Response::Ok {
                data: ResponseData::Ack,
            },
        );
        let ClientError::Daemon {
            code,
            retryable,
            details: Some(details),
            ..
        } = error
        else {
            return Err("expected a typed recovery error with recovery details");
        };
        assert_eq!(code, "selection_changed_after_request");
        assert!(!retryable);
        assert_eq!(details["original_workspace"]["workspace_id"], "original");
        assert_eq!(
            details["send_identity"]["operation_id"],
            "original-operation"
        );
        assert_eq!(details["send_identity"]["expected_revision"], 7);
        assert!(!details.to_string().contains("private-approval"));
        Ok(())
    }
}
