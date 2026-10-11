use crate::cli::{CategoriesAction, OutputFormat};
use crate::commands::resolve_optional_account;
use crate::ipc_client::IpcClient;
use crate::output::{jsonl, resolve_format};
use mxr_core::{CategoryId, MessageId, PersonalCategory};
use mxr_protocol::{Request, Response, ResponseData};
use std::str::FromStr;

pub async fn run(
    action: Option<CategoriesAction>,
    account: Option<String>,
    format: Option<OutputFormat>,
) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    let account_id = resolve_optional_account(&mut client, account.as_deref()).await?;
    let format = resolve_format(format);
    let response = match action {
        None => {
            client
                .request(Request::ListCategories { account_id })
                .await?
        }
        Some(CategoriesAction::Create { name, dry_run }) => {
            let preview = preview_category_name(&mut client, None, &name).await?;
            if dry_run {
                preview
            } else {
                ensure_category_name_available(preview)?;
                client.request(Request::CreateCategory { name }).await?
            }
        }
        Some(CategoriesAction::Rename {
            category_id,
            new_name,
            dry_run,
        }) => {
            let category_id = parse_category_id(&category_id)?;
            let preview =
                preview_category_name(&mut client, Some(category_id.clone()), &new_name).await?;
            if dry_run {
                preview
            } else {
                ensure_category_name_available(preview)?;
                client
                    .request(Request::RenameCategory {
                        category_id,
                        new_name,
                    })
                    .await?
            }
        }
        Some(CategoriesAction::Show {
            category_id,
            limit,
            offset,
        }) => {
            client
                .request(Request::ListCategoryMessages {
                    category_id: parse_category_id(&category_id)?,
                    account_id,
                    limit,
                    offset,
                })
                .await?
        }
        Some(CategoriesAction::Assign {
            message_ids,
            add,
            remove,
            preview_token: Some(preview_token),
        }) => {
            if !message_ids.is_empty() || !add.is_empty() || !remove.is_empty() {
                anyhow::bail!("pass only --preview-token when applying a category assignment")
            }
            client
                .request(Request::ApplyCategoryMembership { preview_token })
                .await?
        }
        Some(CategoriesAction::Assign {
            message_ids,
            add,
            remove,
            preview_token: None,
        }) => {
            let messages = message_ids
                .iter()
                .map(|id| parse_message_id(id))
                .collect::<anyhow::Result<Vec<_>>>()?;
            let additions = add
                .iter()
                .map(|id| parse_category_id(id))
                .collect::<anyhow::Result<Vec<_>>>()?;
            let removals = remove
                .iter()
                .map(|id| parse_category_id(id))
                .collect::<anyhow::Result<Vec<_>>>()?;
            client
                .request(Request::PreviewCategoryMembership {
                    account_id,
                    message_ids: messages,
                    add_category_ids: additions,
                    remove_category_ids: removals,
                })
                .await?
        }
        Some(CategoriesAction::Delete {
            category_id: _,
            preview_token: Some(preview_token),
        }) => {
            client
                .request(Request::DeleteCategory { preview_token })
                .await?
        }
        Some(CategoriesAction::Delete {
            category_id: Some(category_id),
            preview_token: None,
        }) => {
            client
                .request(Request::PreviewDeleteCategory {
                    category_id: parse_category_id(&category_id)?,
                })
                .await?
        }
        Some(CategoriesAction::Delete {
            category_id: None,
            preview_token: None,
        }) => anyhow::bail!("category ID is required"),
        Some(CategoriesAction::Undo { mutation_id }) => {
            client
                .request(Request::UndoCategoryMembership { mutation_id })
                .await?
        }
    };
    match response {
        Response::Error { message, .. } => anyhow::bail!("{message}"),
        Response::Ok { data } => print_response(data, format)?,
    }
    Ok(())
}

async fn preview_category_name(
    client: &mut IpcClient,
    category_id: Option<CategoryId>,
    name: &str,
) -> anyhow::Result<Response> {
    Ok(client
        .request(Request::PreviewCategoryName {
            category_id,
            name: name.to_string(),
        })
        .await?)
}

fn ensure_category_name_available(response: Response) -> anyhow::Result<()> {
    match response {
        Response::Ok {
            data:
                ResponseData::CategoryNamePreview {
                    available: true, ..
                },
        } => Ok(()),
        Response::Ok {
            data:
                ResponseData::CategoryNamePreview {
                    available: false, ..
                },
        } => anyhow::bail!("a category with that name already exists"),
        Response::Error { message, .. } => anyhow::bail!("{message}"),
        _ => anyhow::bail!("unexpected response to category name preview"),
    }
}

fn parse_category_id(input: &str) -> anyhow::Result<CategoryId> {
    CategoryId::from_str(input).map_err(|_| anyhow::anyhow!("invalid category ID: {input}"))
}

fn parse_message_id(input: &str) -> anyhow::Result<MessageId> {
    MessageId::from_str(input).map_err(|_| anyhow::anyhow!("invalid message ID: {input}"))
}

fn print_response(data: ResponseData, format: OutputFormat) -> anyhow::Result<()> {
    match data {
        ResponseData::Categories { categories } => print_categories(&categories, format),
        ResponseData::CategoryMessages {
            envelopes,
            total,
            has_more,
        } => {
            if format == OutputFormat::Json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(
                        &serde_json::json!({"envelopes":envelopes,"total":total,"has_more":has_more})
                    )?
                );
            } else if format == OutputFormat::Jsonl {
                println!("{}", jsonl(&envelopes)?);
            } else {
                for envelope in envelopes {
                    println!(
                        "{}\t{}\t{}",
                        envelope.id, envelope.from.email, envelope.subject
                    );
                }
            }
            Ok(())
        }
        other => {
            if format == OutputFormat::Json {
                println!("{}", serde_json::to_string_pretty(&other)?);
            } else {
                println!("{}", serde_json::to_string(&other)?);
            }
            Ok(())
        }
    }
}

fn print_categories(categories: &[PersonalCategory], format: OutputFormat) -> anyhow::Result<()> {
    match format {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(categories)?),
        OutputFormat::Jsonl => println!("{}", jsonl(categories)?),
        _ if categories.is_empty() => println!("No categories"),
        _ => {
            println!("{:<36} {:<28} {:>8} {:>8}", "ID", "NAME", "UNREAD", "TOTAL");
            for category in categories {
                println!(
                    "{:<36} {:<28} {:>8} {:>8}",
                    category.id, category.name, category.unread_count, category.total_count
                );
            }
        }
    }
    Ok(())
}
