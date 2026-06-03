use crate::cli::{BodyViewArg, OutputFormat};
use crate::commands::expect_response;
use crate::commands::resolve_optional_account;
use crate::commands::selection::{resolve_message_ids, SelectionLimit};
use crate::ipc_client::IpcClient;
use crate::output::resolve_format;
use mxr_core::MessageId;
use mxr_protocol::*;

fn render_body_view(
    body: &mxr_core::types::MessageBody,
    selected_view: &BodyViewArg,
    html_command: Option<String>,
) -> String {
    match selected_view {
        BodyViewArg::Reader => {
            let config = mxr_reader::ReaderConfig {
                html_command,
                ..Default::default()
            };
            if let Some(text) = body.text_plain.as_deref() {
                mxr_reader::clean(Some(text), None, &config).content
            } else if let Some(html) = body.text_html.as_deref() {
                mxr_reader::clean(None, Some(html), &config).content
            } else if let Some(summary) = body.best_effort_readable_summary() {
                mxr_reader::clean(Some(&summary), None, &config).content
            } else {
                "(no body)".to_string()
            }
        }
        BodyViewArg::Raw => {
            if let Some(text) = body.text_plain.as_deref() {
                text.to_string()
            } else if let Some(html) = body.text_html.as_deref() {
                html.to_string()
            } else if let Some(summary) = body.best_effort_readable_summary() {
                summary
            } else {
                "(no body)".to_string()
            }
        }
        BodyViewArg::Html => body
            .text_html
            .clone()
            .unwrap_or_else(|| "(no HTML body)".to_string()),
        BodyViewArg::Headers => unreachable!("headers handled separately"),
    }
}

pub struct CatRunOptions {
    pub message_id: Option<String>,
    pub search: Option<String>,
    pub account: Option<String>,
    pub first: bool,
    pub limit: Option<u32>,
    pub view: Option<BodyViewArg>,
    pub assets: bool,
    pub raw: bool,
    pub html: bool,
    pub format: Option<OutputFormat>,
}

pub async fn run(options: CatRunOptions) -> anyhow::Result<()> {
    let CatRunOptions {
        message_id,
        search,
        account,
        first,
        limit,
        view,
        assets,
        raw,
        html,
        format,
    } = options;
    let mut client = IpcClient::connect().await?;
    let account_id = resolve_optional_account(&mut client, account.as_deref()).await?;
    let ids = resolve_message_ids(
        &mut client,
        message_id.into_iter().collect(),
        search,
        account_id.as_ref(),
        SelectionLimit::from_flags(first, limit),
    )
    .await?;
    if ids.is_empty() {
        anyhow::bail!("No messages matched");
    }

    let fmt = resolve_format(format);

    if assets {
        return print_assets_for_ids(&mut client, &ids, fmt).await;
    }

    match fmt {
        OutputFormat::Json => {
            let mut bodies = Vec::with_capacity(ids.len());
            for id in &ids {
                bodies.push(fetch_body(&mut client, id.clone()).await?);
            }
            if ids.len() == 1 {
                println!("{}", serde_json::to_string_pretty(&bodies[0])?);
            } else {
                println!("{}", serde_json::to_string_pretty(&bodies)?);
            }
        }
        OutputFormat::Jsonl => {
            for id in &ids {
                let body = fetch_body(&mut client, id.clone()).await?;
                println!("{}", serde_json::to_string(&body)?);
            }
        }
        OutputFormat::Csv => {
            let mut writer = csv::Writer::from_writer(Vec::new());
            writer.write_record(["message_id", "has_text", "has_html", "attachments"])?;
            for id in &ids {
                let body = fetch_body(&mut client, id.clone()).await?;
                writer.write_record(&[
                    id.as_str().clone(),
                    body.text_plain.is_some().to_string(),
                    body.text_html.is_some().to_string(),
                    body.attachments.len().to_string(),
                ])?;
            }
            println!("{}", String::from_utf8(writer.into_inner()?)?.trim_end());
        }
        OutputFormat::Ids => {
            for id in &ids {
                println!("{id}");
            }
        }
        OutputFormat::Table => {
            let selected_view = view.unwrap_or_else(|| {
                if raw {
                    BodyViewArg::Raw
                } else if html {
                    BodyViewArg::Html
                } else if mxr_config::load_config().map_or(true, |config| config.render.reader_mode)
                {
                    BodyViewArg::Reader
                } else {
                    BodyViewArg::Raw
                }
            });

            for (index, id) in ids.iter().enumerate() {
                if ids.len() > 1 {
                    if index > 0 {
                        println!();
                    }
                    println!("--- {} ---", id.as_str());
                }
                if matches!(selected_view, BodyViewArg::Headers) {
                    let headers = fetch_headers(&mut client, id.clone()).await?;
                    for (key, value) in &headers {
                        println!("{key}: {value}");
                    }
                    continue;
                }
                let body = fetch_body(&mut client, id.clone()).await?;
                let html_command = mxr_config::load_config()
                    .ok()
                    .and_then(|config| config.render.html_command);
                println!("{}", render_body_view(&body, &selected_view, html_command));
            }
        }
    }
    Ok(())
}

async fn fetch_body(
    client: &mut IpcClient,
    id: MessageId,
) -> anyhow::Result<mxr_core::types::MessageBody> {
    let resp = client.request(Request::GetBody { message_id: id }).await?;
    expect_response(resp, |r| match r {
        Response::Ok {
            data: ResponseData::Body { body },
        } => Some(body),
        _ => None,
    })
}

async fn fetch_headers(
    client: &mut IpcClient,
    id: MessageId,
) -> anyhow::Result<Vec<(String, String)>> {
    let resp = client
        .request(Request::GetHeaders { message_id: id })
        .await?;
    expect_response(resp, |r| match r {
        Response::Ok {
            data: ResponseData::Headers { headers },
        } => Some(headers),
        _ => None,
    })
}

async fn print_assets_for_ids(
    client: &mut IpcClient,
    ids: &[MessageId],
    fmt: OutputFormat,
) -> anyhow::Result<()> {
    let allow_remote =
        mxr_config::load_config().map_or(true, |config| config.render.html_remote_content);

    match fmt {
        OutputFormat::Json => {
            let mut all = Vec::new();
            for id in ids {
                let resp = client
                    .request(Request::GetHtmlImageAssets {
                        message_id: id.clone(),
                        allow_remote,
                    })
                    .await?;
                let assets = expect_response(resp, |r| match r {
                    Response::Ok {
                        data: ResponseData::HtmlImageAssets { assets, .. },
                    } => Some(assets),
                    _ => None,
                })?;
                all.push(serde_json::json!({ "message_id": id.as_str(), "assets": assets }));
            }
            if ids.len() == 1 {
                println!("{}", serde_json::to_string_pretty(&all[0])?);
            } else {
                println!("{}", serde_json::to_string_pretty(&all)?);
            }
        }
        OutputFormat::Jsonl => {
            for id in ids {
                let resp = client
                    .request(Request::GetHtmlImageAssets {
                        message_id: id.clone(),
                        allow_remote,
                    })
                    .await?;
                let assets = expect_response(resp, |r| match r {
                    Response::Ok {
                        data: ResponseData::HtmlImageAssets { assets, .. },
                    } => Some(assets),
                    _ => None,
                })?;
                println!(
                    "{}",
                    serde_json::to_string(
                        &serde_json::json!({ "message_id": id.as_str(), "assets": assets })
                    )?
                );
            }
        }
        OutputFormat::Csv => {
            let mut writer = csv::Writer::from_writer(Vec::new());
            writer.write_record(["message_id", "status", "kind", "source", "path"])?;
            for id in ids {
                let resp = client
                    .request(Request::GetHtmlImageAssets {
                        message_id: id.clone(),
                        allow_remote,
                    })
                    .await?;
                let assets = expect_response(resp, |r| match r {
                    Response::Ok {
                        data: ResponseData::HtmlImageAssets { assets, .. },
                    } => Some(assets),
                    _ => None,
                })?;
                for asset in &assets {
                    writer.write_record(&[
                        id.as_str().clone(),
                        format!("{:?}", asset.status).to_lowercase(),
                        format!("{:?}", asset.kind).to_lowercase(),
                        asset.source.clone(),
                        asset
                            .path
                            .as_ref()
                            .map_or_else(|| "-".into(), |p| p.display().to_string()),
                    ])?;
                }
            }
            let bytes = writer.into_inner()?;
            println!("{}", String::from_utf8(bytes)?.trim_end());
        }
        OutputFormat::Ids | OutputFormat::Table => {
            for (index, id) in ids.iter().enumerate() {
                if ids.len() > 1 {
                    if index > 0 {
                        println!();
                    }
                    println!("--- {} ---", id.as_str());
                }
                let resp = client
                    .request(Request::GetHtmlImageAssets {
                        message_id: id.clone(),
                        allow_remote,
                    })
                    .await?;
                let assets = expect_response(resp, |r| match r {
                    Response::Ok {
                        data: ResponseData::HtmlImageAssets { assets, .. },
                    } => Some(assets),
                    _ => None,
                })?;
                for asset in &assets {
                    let path = asset
                        .path
                        .as_ref()
                        .map_or_else(|| "-".into(), |p| p.display().to_string());
                    println!(
                        "{:<10} {:<18} {} -> {}",
                        format!("{:?}", asset.status).to_lowercase(),
                        format!("{:?}", asset.kind).to_lowercase(),
                        asset.source,
                        path,
                    );
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use mxr_core::types::{MessageBody, MessageMetadata};

    fn body(text_plain: Option<&str>, text_html: Option<&str>) -> MessageBody {
        MessageBody {
            message_id: MessageId::new(),
            text_plain: text_plain.map(str::to_string),
            text_html: text_html.map(str::to_string),
            attachments: vec![],
            fetched_at: Utc::now(),
            metadata: MessageMetadata::default(),
        }
    }

    #[test]
    fn raw_view_returns_exact_plain_body() {
        let body = body(
            Some("Hello team,\n> exact quote\n-- \nSignature"),
            Some("<p>ignored</p>"),
        );
        assert_eq!(
            render_body_view(&body, &BodyViewArg::Raw, None),
            "Hello team,\n> exact quote\n-- \nSignature"
        );
    }

    #[test]
    fn raw_view_falls_back_to_exact_html_when_plain_is_missing() {
        let body = body(None, Some("<p>Hello <strong>html</strong></p>"));
        assert_eq!(
            render_body_view(&body, &BodyViewArg::Raw, None),
            "<p>Hello <strong>html</strong></p>"
        );
    }

    #[test]
    fn html_view_returns_only_exact_html() {
        let body = body(Some("plain fallback"), Some("<p>Hello html</p>"));
        assert_eq!(
            render_body_view(&body, &BodyViewArg::Html, None),
            "<p>Hello html</p>"
        );
    }

    #[test]
    fn reader_view_renders_html_only_body_as_text() {
        let body = body(
            None,
            Some("<html><body><h1>Order confirmed</h1><p>Thanks for shopping.</p></body></html>"),
        );

        let rendered = render_body_view(&body, &BodyViewArg::Reader, Some("cat".into()));

        assert!(rendered.contains("Order confirmed"));
        assert!(rendered.contains("Thanks for shopping."));
        assert!(!rendered.contains("<h1>"));
        assert!(!rendered.contains("</"));
    }
}
