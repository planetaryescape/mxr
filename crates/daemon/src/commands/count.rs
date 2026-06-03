use crate::cli::{OutputFormat, SearchGroupByArg, SearchModeArg};
use crate::commands::{expect_response, resolve_optional_account};
use crate::ipc_client::IpcClient;
use crate::output::resolve_format;
use mxr_protocol::*;

pub async fn run(
    query: String,
    account: Option<String>,
    mode: Option<SearchModeArg>,
    group_by: Option<SearchGroupByArg>,
    format: Option<OutputFormat>,
    quiet: bool,
) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    let account_id = resolve_optional_account(&mut client, account.as_deref()).await?;
    if let Some(group_by) = group_by {
        let resp = client
            .request(Request::SearchAggregation {
                query: query.clone(),
                account_id,
                mode: mode.map(Into::into),
                group_by: group_by.into(),
                limit: None,
            })
            .await?;
        let (group_by, total, groups) = expect_response(resp, |r| match r {
            Response::Ok {
                data:
                    ResponseData::SearchAggregation {
                        group_by,
                        total,
                        groups,
                        ..
                    },
            } => Some((group_by, total, groups)),
            _ => None,
        })?;
        crate::commands::search::render_aggregation(
            resolve_format(format),
            &query,
            group_by,
            total,
            &groups,
        )?;
        return Ok(());
    }
    let resp = client
        .request(Request::Count {
            query: query.clone(),
            account_id,
            mode: mode.map(Into::into),
        })
        .await?;

    let count = expect_response(resp, |r| match r {
        Response::Ok {
            data: ResponseData::Count { count },
        } => Some(count),
        _ => None,
    })?;
    print!("{}", render_count_output(&query, count, format, quiet)?);
    Ok(())
}

fn render_count_output(
    query: &str,
    count: u32,
    format: Option<OutputFormat>,
    quiet: bool,
) -> anyhow::Result<String> {
    if quiet {
        return Ok(format!("{count}\n"));
    }

    match resolve_format(format) {
        OutputFormat::Json | OutputFormat::Jsonl => Ok(format!(
            "{}\n",
            serde_json::to_string(&serde_json::json!({"query": query, "count": count}))?
        )),
        _ => Ok(format!("{count}\n")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quiet_output_is_bare_integer_even_when_json_requested() {
        let rendered = render_count_output("is:unread", 42, Some(OutputFormat::Json), true)
            .expect("render count");
        assert_eq!(rendered, "42\n");
    }
}
