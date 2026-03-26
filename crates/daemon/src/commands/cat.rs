use crate::cli::{BodyViewArg, OutputFormat};
use crate::commands::expect_response;
use crate::ipc_client::IpcClient;
use crate::mxr_core::MessageId;
use crate::mxr_protocol::*;
use crate::output::resolve_format;

fn render_body_view(
    body: &crate::mxr_core::types::MessageBody,
    selected_view: &BodyViewArg,
    html_command: Option<String>,
) -> String {
    match selected_view {
        BodyViewArg::Reader => {
            let config = crate::mxr_reader::ReaderConfig {
                html_command,
                ..Default::default()
            };
            if let Some(text) = body.text_plain.as_deref() {
                crate::mxr_reader::clean(Some(text), None, &config).content
            } else if let Some(html) = body.text_html.as_deref() {
                crate::mxr_reader::clean(None, Some(html), &config).content
            } else {
                "(no body)".to_string()
            }
        }
        BodyViewArg::Raw => {
            if let Some(text) = body.text_plain.as_deref() {
                text.to_string()
            } else if let Some(html) = body.text_html.as_deref() {
                html.to_string()
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

pub async fn run(
    message_id: String,
    view: Option<BodyViewArg>,
    assets: bool,
    raw: bool,
    html: bool,
    format: Option<OutputFormat>,
) -> anyhow::Result<()> {
    let mid = MessageId::from_uuid(uuid::Uuid::parse_str(&message_id)?);
    let mut client = IpcClient::connect().await?;
    let fmt = resolve_format(format);

    if assets {
        let allow_remote = crate::mxr_config::load_config()
            .map(|config| config.render.html_remote_content)
            .unwrap_or(true);
        let resp = client
            .request(Request::GetHtmlImageAssets {
                message_id: mid,
                allow_remote,
            })
            .await?;
        let assets = expect_response(resp, |r| match r {
            Response::Ok {
                data: ResponseData::HtmlImageAssets { assets, .. },
            } => Some(assets),
            _ => None,
        })?;
        match fmt {
            OutputFormat::Json => {
                println!("{}", serde_json::to_string_pretty(&assets)?);
            }
            _ => {
                for asset in &assets {
                    let path = asset
                        .path
                        .as_ref()
                        .map(|path| path.display().to_string())
                        .unwrap_or_else(|| "-".into());
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
        return Ok(());
    }

    match fmt {
        OutputFormat::Json => {
            let resp = client
                .request(Request::GetBody {
                    message_id: mid.clone(),
                })
                .await?;
            let body = expect_response(resp, |r| match r {
                Response::Ok {
                    data: ResponseData::Body { body },
                } => Some(body),
                _ => None,
            })?;
            println!("{}", serde_json::to_string_pretty(&body)?);
        }
        _ => {
            let selected_view = view.unwrap_or_else(|| {
                if raw {
                    BodyViewArg::Raw
                } else if html {
                    BodyViewArg::Html
                } else if crate::mxr_config::load_config()
                    .map(|config| config.render.reader_mode)
                    .unwrap_or(true)
                {
                    BodyViewArg::Reader
                } else {
                    BodyViewArg::Raw
                }
            });

            if matches!(selected_view, BodyViewArg::Headers) {
                let resp = client
                    .request(Request::GetHeaders {
                        message_id: mid.clone(),
                    })
                    .await?;
                let headers = expect_response(resp, |r| match r {
                    Response::Ok {
                        data: ResponseData::Headers { headers },
                    } => Some(headers),
                    _ => None,
                })?;
                for (key, value) in &headers {
                    println!("{key}: {value}");
                }
                return Ok(());
            }

            let resp = client.request(Request::GetBody { message_id: mid }).await?;
            let body = expect_response(resp, |r| match r {
                Response::Ok {
                    data: ResponseData::Body { body },
                } => Some(body),
                _ => None,
            })?;

            let html_command = crate::mxr_config::load_config()
                .ok()
                .and_then(|config| config.render.html_command);
            println!("{}", render_body_view(&body, &selected_view, html_command));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mxr_core::types::{MessageBody, MessageMetadata};
    use chrono::Utc;

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
}
