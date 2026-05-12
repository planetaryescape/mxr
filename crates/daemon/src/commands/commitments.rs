use crate::cli::{CommitmentStatusArg, CommitmentsAction, OutputFormat};
use crate::commands::resolve_account;
use crate::ipc_client::IpcClient;
use crate::output::resolve_format;
use mxr_protocol::*;

pub async fn run(
    action: Option<CommitmentsAction>,
    contact: Option<String>,
    status: Option<CommitmentStatusArg>,
    account: Option<String>,
    format: Option<OutputFormat>,
) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    match action {
        Some(CommitmentsAction::Resolve { id }) => {
            let resp = client
                .request(Request::ResolveCommitment { commitment_id: id })
                .await?;
            match resp {
                Response::Ok { .. } => println!("resolved"),
                Response::Error { message, .. } => anyhow::bail!(message),
            }
        }
        None => {
            let account_id = resolve_account(&mut client, account.as_deref()).await?;
            let resp = client
                .request(Request::ListCommitments {
                    account_id,
                    email: contact,
                    status: status.map(status_data),
                })
                .await?;
            print_list(resp, resolve_format(format))?;
        }
    }
    Ok(())
}

fn print_list(resp: Response, fmt: OutputFormat) -> anyhow::Result<()> {
    match resp {
        Response::Ok {
            data: ResponseData::CommitmentList { commitments },
        } => match fmt {
            OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&commitments)?),
            OutputFormat::Jsonl => {
                for commitment in commitments {
                    println!("{}", serde_json::to_string(&commitment)?);
                }
            }
            OutputFormat::Ids => {
                for commitment in commitments {
                    println!("{}", commitment.id);
                }
            }
            _ => {
                for commitment in commitments {
                    println!(
                        "{}\t{}\t{:?}\t{}",
                        commitment.id, commitment.email, commitment.direction, commitment.what
                    );
                }
            }
        },
        Response::Error { message, .. } => anyhow::bail!(message),
        _ => anyhow::bail!("Unexpected response"),
    }
    Ok(())
}

fn status_data(value: CommitmentStatusArg) -> CommitmentStatusData {
    match value {
        CommitmentStatusArg::Open => CommitmentStatusData::Open,
        CommitmentStatusArg::Resolved => CommitmentStatusData::Resolved,
        CommitmentStatusArg::Expired => CommitmentStatusData::Expired,
    }
}
