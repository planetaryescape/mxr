//! `mxr reading`: newsletters you chose, as an edition you visit.
//!
//! The daemon builds the edition, the reader text and every preview; this
//! prints them for people, or passes the JSON through for scripts and
//! agents.

use crate::cli::{MarginaliaAction, OutputFormat, PlaceArgs, ReadingAction};
use crate::commands::{expect_response, resolve_optional_account};
use crate::ipc_client::IpcClient;
use crate::output::{jsonl, resolve_format, terminal_block};
use mxr_core::id::{AccountId, ThreadId};
use mxr_protocol::{
    reading_copy, MailPlaceData, MarginaliaConnectionData, MarginaliaSendData,
    MarginaliaStatusData, MarginaliaSyncData, ModeDoneOutcomeData, ModeKindData,
    ReadingBandGroupData, ReadingEditionData, ReadingFetchData, ReadingHighlightChangeData,
    ReadingItemData, ReadingItemDetailData, ReadingLaterOutcomeData, ReadingParagraphData, Request,
    Response, ResponseData,
};
use std::fmt::Write as _;

macro_rules! expect_data {
    ($response:expr, $variant:ident { $($field:ident),+ }) => {
        expect_response($response, |response| match response {
            Response::Ok {
                data: ResponseData::$variant { $($field),+ , .. },
            } => Some(($($field),+)),
            _ => None,
        })?
    };
}

pub async fn run(
    action: Option<ReadingAction>,
    account: Option<String>,
    format: Option<OutputFormat>,
) -> anyhow::Result<()> {
    if let Some(ReadingAction::Senders {
        sender,
        limit,
        offset,
        messages,
    }) = action
    {
        return crate::commands::places::list(
            MailPlaceData::Reading,
            PlaceArgs {
                account,
                sender,
                limit,
                offset,
                messages,
                format,
            },
        )
        .await;
    }
    let mut client = IpcClient::connect().await?;
    let account_id = resolve_optional_account(&mut client, account.as_deref()).await?;
    let format = resolve_format(format);
    match action.unwrap_or(ReadingAction::Edition { peek: false }) {
        ReadingAction::Edition { peek } => {
            let edition = get_edition(&mut client, account_id, !peek).await?;
            print!("{}", render_edition(&edition, format)?);
        }
        ReadingAction::Later {
            add,
            remove,
            dry_run,
        } => {
            if add.is_empty() && remove.is_empty() {
                let edition = get_edition(&mut client, account_id, false).await?;
                print!("{}", render_later(&edition, format)?);
            } else {
                let later = remove.is_empty();
                let item_keys = if later { add } else { remove };
                set_later(&mut client, item_keys, later, dry_run, format).await?;
            }
        }
        ReadingAction::Open {
            item,
            article,
            refresh,
        } => open(&mut client, item, article, refresh, format).await?,
        ReadingAction::LetGo {
            items,
            all,
            dry_run,
        } => let_go(&mut client, account_id, items, all, dry_run, format).await?,
        ReadingAction::Highlight {
            item,
            quote,
            note,
            article,
        } => {
            let highlight = expect_data!(
                client
                    .request(Request::SaveHighlight {
                        item_key: item,
                        quote,
                        note,
                        view: Some(if article { "article" } else { "issue" }.to_string()),
                    })
                    .await?,
                ReadingHighlight { highlight }
            );
            match format {
                OutputFormat::Json | OutputFormat::Jsonl => {
                    println!("{}", serde_json::to_string_pretty(&highlight)?);
                }
                _ => println!(
                    "Saved a highlight from {} ({}).",
                    terminal_block(&highlight.title),
                    highlight.id
                ),
            }
        }
        ReadingAction::Note {
            highlight,
            text,
            dry_run,
        } => {
            let change = expect_data!(
                client
                    .request(Request::UpdateHighlight {
                        highlight_id: highlight,
                        note: text,
                        dry_run,
                    })
                    .await?,
                ReadingHighlightChanged { change }
            );
            print_highlight_change(&change, format)?;
        }
        ReadingAction::Unhighlight { highlight, dry_run } => {
            let change = expect_data!(
                client
                    .request(Request::RemoveHighlight {
                        highlight_id: highlight,
                        dry_run,
                    })
                    .await?,
                ReadingHighlightChanged { change }
            );
            print_highlight_change(&change, format)?;
        }
        ReadingAction::Export { markdown } => {
            let (highlights, text) = expect_data!(
                client
                    .request(Request::ExportReadingHighlights {
                        account_id: account_id.clone(),
                    })
                    .await?,
                ReadingHighlights {
                    highlights,
                    markdown
                }
            );
            // Markdown unless JSON was asked for by name.
            match (markdown, format) {
                (false, OutputFormat::Json) => {
                    println!("{}", serde_json::to_string_pretty(&highlights)?);
                }
                (false, OutputFormat::Jsonl) => println!("{}", jsonl(&highlights)?),
                _ => print!("{text}"),
            }
        }
        ReadingAction::Sources => {
            let edition = get_edition(&mut client, account_id, false).await?;
            match format {
                OutputFormat::Json => {
                    println!("{}", serde_json::to_string_pretty(&edition.sources)?);
                }
                OutputFormat::Jsonl => println!("{}", jsonl(&edition.sources)?),
                _ => print!("{}", sources_text(&edition)),
            }
        }
        ReadingAction::Marginalia { action } => marginalia(&mut client, action, format).await?,
        ReadingAction::Senders { .. } => unreachable!("handled above"),
    }
    Ok(())
}

async fn get_edition(
    client: &mut IpcClient,
    account_id: Option<AccountId>,
    mark_visit: bool,
) -> anyhow::Result<ReadingEditionData> {
    Ok(expect_data!(
        client
            .request(Request::GetReadingEdition {
                account_id,
                mark_visit,
            })
            .await?,
        ReadingEdition { edition }
    ))
}

fn all_items(edition: &ReadingEditionData) -> impl Iterator<Item = &ReadingItemData> {
    edition.bands.iter().flat_map(|band| band.items.iter())
}

fn render_edition(edition: &ReadingEditionData, format: OutputFormat) -> anyhow::Result<String> {
    Ok(match format {
        OutputFormat::Json => format!("{}\n", serde_json::to_string_pretty(edition)?),
        OutputFormat::Jsonl => format!("{}\n", jsonl(&all_items(edition).collect::<Vec<_>>())?),
        OutputFormat::Ids => all_items(edition)
            .map(|item| format!("{}\n", item.item_key))
            .collect(),
        OutputFormat::Csv => csv(edition),
        OutputFormat::Table => edition_text(edition),
    })
}

fn csv(edition: &ReadingEditionData) -> String {
    let mut out = String::from("band,item_key,source,title,minutes,shape,links\n");
    for band in &edition.bands {
        for item in &band.items {
            let _ = writeln!(
                out,
                "{},{},{},{},{},{:?},{}",
                serde_json::to_string(&band.band)
                    .unwrap_or_default()
                    .trim_matches('"'),
                item.item_key,
                csv_field(&item.source),
                csv_field(&item.title),
                item.minutes,
                item.shape,
                item.links.len()
            );
        }
    }
    out
}

fn csv_field(text: &str) -> String {
    format!("\"{}\"", text.replace('"', "\"\""))
}

/// "3 min".
fn minutes(n: u32) -> String {
    format!("{n} min")
}

fn item_lines(out: &mut String, item: &ReadingItemData) {
    let mut meta = vec![item.source.clone()];
    if !item.links.is_empty() {
        meta.push(format!("{} links", item.links.len()));
    }
    meta.push(minutes(item.minutes));
    if let Some(engagement) = &item.engagement {
        meta.push(engagement.clone());
    }
    let lead = if item.lead { "* " } else { "  " };
    let _ = writeln!(out, "{lead}{}", item.title);
    let _ = writeln!(out, "    {}", meta.join(" · "));
    if let Some(standfirst) = &item.standfirst {
        let _ = writeln!(out, "    {standfirst}");
    }
    for link in item.links.iter().take(4) {
        let via = if link.tracked { "via " } else { "" };
        let later = if link.on_later { "  (on Later)" } else { "" };
        let _ = writeln!(out, "    > {}  {via}{}{later}", link.title, link.domain);
        let _ = writeln!(out, "      {}", link.item_key);
    }
    if item.links.len() > 4 {
        let _ = writeln!(out, "    + {} more", item.links.len() - 4);
    }
    if let Some(offer) = &item.unsubscribe_offer {
        let _ = writeln!(
            out,
            "    {offer}. D to unsubscribe in the app, or mxr unsubscribe."
        );
    }
    let _ = writeln!(out, "    {}", item.item_key);
}

fn band_text(out: &mut String, band: &ReadingBandGroupData) {
    match &band.note {
        Some(note) => {
            let _ = writeln!(out, "{}  {note}", band.label.to_uppercase());
        }
        None => {
            let _ = writeln!(out, "{}", band.label.to_uppercase());
        }
    }
    for item in &band.items {
        item_lines(out, item);
    }
    out.push('\n');
}

fn edition_text(edition: &ReadingEditionData) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "Reading  {}", edition.header);
    let _ = writeln!(out, "Later {}\n", edition.later_count);
    if let Some(empty) = &edition.empty {
        let _ = writeln!(out, "{}\n", empty.line);
    }
    for (i, band) in edition.bands.iter().enumerate() {
        band_text(&mut out, band);
        if i == 0 && edition.left_off_here {
            let _ = writeln!(out, "── {} ──\n", reading_copy::LEFT_OFF);
        }
    }
    if edition.expired_now > 0 {
        let _ = writeln!(
            out,
            "{} faded since the last edition: done in Reading, still in your mail.",
            edition.expired_now
        );
    }
    let _ = writeln!(
        out,
        "mxr reading open ITEM to read · later --add ITEM · let-go ITEM · sources"
    );
    terminal_block(&out)
}

fn render_later(edition: &ReadingEditionData, format: OutputFormat) -> anyhow::Result<String> {
    Ok(match format {
        OutputFormat::Json => format!("{}\n", serde_json::to_string_pretty(&edition.later)?),
        OutputFormat::Jsonl => format!("{}\n", jsonl(&edition.later)?),
        OutputFormat::Ids => edition
            .later
            .iter()
            .map(|item| format!("{}\n", item.item_key))
            .collect(),
        _ => {
            let mut out = format!("Later {}  Later never fades.\n\n", edition.later_count);
            if edition.later.is_empty() {
                out.push_str("Nothing saved. b in the app, or mxr reading later --add ITEM.\n");
            }
            for item in &edition.later {
                item_lines(&mut out, item);
                if item.still_want_it {
                    let _ = writeln!(out, "    {}", reading_copy::STILL_WANT_IT);
                }
            }
            terminal_block(&out)
        }
    })
}

fn sources_text(edition: &ReadingEditionData) -> String {
    let mut out = String::from("Sources, best read first\n\n");
    for source in &edition.sources {
        let gap = source.median_gap_days.map_or_else(
            || "one issue so far".to_string(),
            |d| match d.round() as i64 {
                ..=1 => "about daily".to_string(),
                n => format!("every {n} days"),
            },
        );
        let _ = writeln!(out, "{}  <{}>", source.name, source.sender_email);
        let _ = writeln!(
            out,
            "    {} · {gap} · fades after {:.0} days",
            source.evidence, source.window_days
        );
        if source.suggest_unsubscribe {
            let _ = writeln!(
                out,
                "    Unsubscribe? mxr unsubscribe --purge {}",
                source.sender_email
            );
        }
    }
    terminal_block(&out)
}

async fn set_later(
    client: &mut IpcClient,
    item_keys: Vec<String>,
    later: bool,
    dry_run: bool,
    format: OutputFormat,
) -> anyhow::Result<()> {
    let (items, later_count, copy) = expect_data!(
        client
            .request(Request::SetReadingLater {
                item_keys,
                later,
                dry_run,
            })
            .await?,
        ReadingLater {
            items,
            later_count,
            copy
        }
    );
    match format {
        OutputFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "dry_run": dry_run,
                "items": items,
                "later_count": later_count,
                "copy": copy,
            }))?
        ),
        OutputFormat::Jsonl => println!("{}", jsonl(&items)?),
        _ => print!("{}", later_text(&items, dry_run, &copy)),
    }
    Ok(())
}

fn later_text(items: &[ReadingLaterOutcomeData], dry_run: bool, copy: &str) -> String {
    let mut out = String::new();
    for item in items {
        let title = item.title.as_deref().unwrap_or(&item.item_key);
        match (&item.error, item.changed) {
            (Some(error), _) => {
                let _ = writeln!(out, "{}: {error}", item.item_key);
            }
            (None, false) => {
                let _ = writeln!(out, "{title}: already as asked");
            }
            (None, true) if dry_run => {
                let action = if item.on_later {
                    "would go on"
                } else {
                    "would come off"
                };
                let _ = writeln!(out, "{title}: {action} Later");
            }
            (None, true) => {
                let action = if item.on_later { "on" } else { "off" };
                let _ = writeln!(out, "{title}: {action} Later");
            }
        }
    }
    if !dry_run {
        let _ = writeln!(out, "{copy}");
    }
    terminal_block(&out)
}

async fn open(
    client: &mut IpcClient,
    item: String,
    article: bool,
    refresh: bool,
    format: OutputFormat,
) -> anyhow::Result<()> {
    let mut detail = expect_data!(
        client
            .request(Request::GetReadingItem {
                item_key: item.clone(),
            })
            .await?,
        ReadingItem { item }
    );
    let mut fetched: Option<ReadingFetchData> = None;
    if article {
        let saved = detail.article.is_some() && !refresh;
        if !saved {
            let domain = detail.item.domain.clone().unwrap_or_default();
            // Name the site before it learns you clicked.
            eprintln!("Fetching from {domain}…");
        }
        let fetch = expect_data!(
            client
                .request(Request::FetchArticle {
                    item_key: item,
                    refresh,
                })
                .await?,
            ReadingArticle { fetch }
        );
        if let Some(article) = &fetch.article {
            detail.article = Some(article.clone());
        }
        fetched = Some(fetch);
    }
    match format {
        OutputFormat::Json | OutputFormat::Jsonl => println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "item": detail,
                "fetch": fetched,
            }))?
        ),
        _ => print!("{}", reader_text(&detail, article, fetched.as_ref())),
    }
    Ok(())
}

/// Greedy word wrap: a reader column needs nothing more, so no crate.
fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        if !line.is_empty() && line.chars().count() + 1 + word.chars().count() > width {
            lines.push(std::mem::take(&mut line));
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(word);
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}

/// Wrapped to a 66-character measure.
fn paragraphs_text(out: &mut String, paragraphs: &[ReadingParagraphData]) {
    for paragraph in paragraphs {
        let prefix = match paragraph.kind.as_str() {
            "list_item" => "- ",
            "quote" => "> ",
            _ => "",
        };
        let text = if paragraph.kind == "heading" {
            paragraph.text.to_uppercase()
        } else {
            format!("{prefix}{}", paragraph.text)
        };
        for line in wrap(&text, 66) {
            let _ = writeln!(out, "{line}");
        }
        out.push('\n');
    }
}

fn reader_text(
    detail: &ReadingItemDetailData,
    article: bool,
    fetch: Option<&ReadingFetchData>,
) -> String {
    let item = &detail.item;
    let mut out = String::new();
    let _ = writeln!(out, "{}", item.title);
    // The article's own length once it is the text on screen.
    let left = match (&detail.article, article) {
        (Some(saved), true) => saved.minutes,
        _ => detail.minutes_left,
    };
    let _ = writeln!(
        out,
        "{} · {} · {} left\n",
        item.source,
        item.arrived_at.format("%-d %b"),
        minutes(left)
    );
    match (
        article,
        &detail.article,
        fetch.and_then(|f| f.error.as_deref()),
    ) {
        (true, Some(saved), _) => {
            if saved.contacted.is_empty() {
                let _ = writeln!(out, "Article from {} (a saved copy).\n", saved.final_url);
            } else {
                let _ = writeln!(
                    out,
                    "Article from {} (fetched from {}).\n",
                    saved.final_url,
                    saved.contacted.join(", ")
                );
            }
            paragraphs_text(&mut out, &saved.paragraphs);
        }
        (true, None, error) => {
            let _ = writeln!(
                out,
                "The article couldn't be read here: {}.",
                error.unwrap_or("no copy")
            );
            if let Some(url) = &item.url {
                let _ = writeln!(out, "Open it in a browser: {url}\n");
            }
        }
        (false, _, _) => paragraphs_text(&mut out, &detail.paragraphs),
    }
    let _ = writeln!(out, "── end ──");
    let _ = writeln!(out, "From this source: {}.", detail.source_data.evidence);
    if !detail.highlights.is_empty() {
        let _ = writeln!(out, "{} highlight(s) saved:", detail.highlights.len());
        for highlight in &detail.highlights {
            let _ = writeln!(
                out,
                "  {}  {}{}",
                highlight.id,
                preview(&highlight.quote),
                if highlight.origin == "mxr" {
                    ""
                } else {
                    "  (made in Marginalia)"
                }
            );
        }
    }
    terminal_block(&out)
}

/// The start of a passage, on one line.
fn preview(quote: &str) -> String {
    const SHOWN: usize = 60;
    let line = quote.split_whitespace().collect::<Vec<_>>().join(" ");
    if line.chars().count() <= SHOWN {
        format!("\"{line}\"")
    } else {
        format!("\"{}…\"", line.chars().take(SHOWN).collect::<String>())
    }
}

fn print_highlight_change(
    change: &ReadingHighlightChangeData,
    format: OutputFormat,
) -> anyhow::Result<()> {
    match format {
        OutputFormat::Json | OutputFormat::Jsonl => {
            println!("{}", serde_json::to_string_pretty(change)?);
        }
        _ => print!("{}", highlight_change_text(change)),
    }
    Ok(())
}

fn highlight_change_text(change: &ReadingHighlightChangeData) -> String {
    let highlight = &change.highlight;
    let what = format!(
        "{} from {} ({})",
        preview(&highlight.quote),
        highlight.title,
        highlight.id
    );
    let mut out = match (change.action.as_str(), change.dry_run) {
        ("remove", true) => format!("Would remove {what}.\n"),
        ("remove", false) => format!("Removed {what}.\n"),
        (_, dry_run) => {
            let now = highlight
                .note
                .as_deref()
                .map_or_else(|| "no note".to_string(), |note| format!("\"{note}\""));
            let verb = if dry_run { "Would set" } else { "Set" };
            format!("{verb} the note on {what} to {now}.\n")
        }
    };
    if let Some(id) = &change.mutation_id {
        let _ = writeln!(out, "Undo with: mxr undo {id}");
    } else if change.undo_unavailable {
        out.push_str("It can't be undone: the undo couldn't be saved.\n");
    }
    terminal_block(&out)
}

async fn let_go(
    client: &mut IpcClient,
    account_id: Option<AccountId>,
    items: Vec<String>,
    all: bool,
    dry_run: bool,
    format: OutputFormat,
) -> anyhow::Result<()> {
    let edition = get_edition(client, account_id, false).await?;
    let mut thread_ids: Vec<ThreadId> = Vec::new();
    if all {
        for item in all_items(&edition) {
            if !thread_ids.contains(&item.thread_id) {
                thread_ids.push(item.thread_id.clone());
            }
        }
    } else {
        if items.is_empty() {
            anyhow::bail!("name the items to let go of, or --all for the whole edition");
        }
        for key in &items {
            let detail = expect_data!(
                client
                    .request(Request::GetReadingItem {
                        item_key: key.clone(),
                    })
                    .await?,
                ReadingItem { item }
            );
            if !thread_ids.contains(&detail.item.thread_id) {
                thread_ids.push(detail.item.thread_id);
            }
        }
    }
    if thread_ids.is_empty() {
        println!("Nothing in the edition to let go of.");
        return Ok(());
    }
    let (outcomes, mutation_id) = expect_data!(
        client
            .request(Request::SetModeDone {
                thread_ids,
                mode: ModeKindData::Reading,
                dry_run,
                todo_ids: Vec::new(),
                sender: None,
            })
            .await?,
        ModeDone { items, mutation_id }
    );
    match format {
        OutputFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "dry_run": dry_run,
                "items": outcomes,
                "mutation_id": mutation_id,
            }))?
        ),
        OutputFormat::Jsonl => println!("{}", jsonl(&outcomes)?),
        _ => print!(
            "{}",
            let_go_text(&edition, &outcomes, dry_run, mutation_id.as_deref())
        ),
    }
    Ok(())
}

fn let_go_text(
    edition: &ReadingEditionData,
    outcomes: &[ModeDoneOutcomeData],
    dry_run: bool,
    mutation_id: Option<&str>,
) -> String {
    let mut out = String::new();
    for outcome in outcomes {
        let title = all_items(edition)
            .find(|item| item.thread_id == outcome.thread_id)
            .map_or_else(|| outcome.thread_id.to_string(), |item| item.title.clone());
        match &outcome.error {
            Some(error) => {
                let _ = writeln!(out, "{title}: not let go: {error}");
            }
            None if dry_run => {
                let _ = writeln!(out, "{title}: would be: {}", outcome.copy);
            }
            None => {
                let _ = writeln!(out, "{title}: {}", outcome.copy);
            }
        }
    }
    if let Some(id) = mutation_id {
        let _ = writeln!(out, "Undo with: mxr undo {id}");
    }
    terminal_block(&out)
}

/// The token, from a hidden prompt or from standard input when piped.
fn read_token() -> anyhow::Result<String> {
    use std::io::IsTerminal as _;
    let token = if std::io::stdin().is_terminal() {
        inquire::Password::new("Marginalia token (mrg_…):")
            .without_confirmation()
            .prompt()?
    } else {
        let mut line = String::new();
        std::io::stdin().read_line(&mut line)?;
        line
    };
    let token = token.trim().to_string();
    if token.is_empty() {
        anyhow::bail!("no token given; create one in Marginalia under Account, Connected apps");
    }
    Ok(token)
}

fn status_text(status: &MarginaliaStatusData) -> String {
    match (&status.url, status.connected) {
        (Some(url), true) => format!(
            "Marginalia is connected at {url}. {} item(s) sent.\n",
            status.sent
        ),
        _ => "Marginalia isn't connected. mxr reading marginalia connect --url https://…\n"
            .to_string(),
    }
}

async fn marginalia(
    client: &mut IpcClient,
    action: MarginaliaAction,
    format: OutputFormat,
) -> anyhow::Result<()> {
    let json = matches!(format, OutputFormat::Json | OutputFormat::Jsonl);
    let status_request = match &action {
        MarginaliaAction::Status => Some(Request::GetMarginaliaStatus),
        MarginaliaAction::Disconnect => Some(Request::SetMarginaliaConnection { connection: None }),
        MarginaliaAction::Connect { url } => Some(Request::SetMarginaliaConnection {
            connection: Some(MarginaliaConnectionData {
                url: url.clone(),
                token: read_token()?,
            }),
        }),
        MarginaliaAction::Send { .. } | MarginaliaAction::Sync { .. } => None,
    };
    if let Some(request) = status_request {
        let status = expect_data!(client.request(request).await?, MarginaliaStatus { status });
        if json {
            println!("{}", serde_json::to_string_pretty(&status)?);
        } else {
            print!("{}", status_text(&status));
        }
        return Ok(());
    }
    match action {
        MarginaliaAction::Status
        | MarginaliaAction::Disconnect
        | MarginaliaAction::Connect { .. } => {}
        MarginaliaAction::Send {
            item,
            dry_run,
            open,
        } => {
            let send = expect_data!(
                client
                    .request(Request::SendReadingToMarginalia {
                        item_key: item,
                        dry_run,
                    })
                    .await?,
                MarginaliaSend { send }
            );
            if json {
                println!("{}", serde_json::to_string_pretty(&send)?);
            } else {
                print!("{}", send_text(&send));
            }
            if open {
                if let Some(sent) = &send.marginalia {
                    open::that(&sent.read_url)?;
                }
            }
        }
        MarginaliaAction::Sync { item } => {
            let sync = expect_data!(
                client
                    .request(Request::SyncMarginaliaHighlights { item_key: item })
                    .await?,
                MarginaliaSync { sync }
            );
            if json {
                println!("{}", serde_json::to_string_pretty(&sync)?);
            } else {
                print!("{}", sync_text(&sync));
            }
        }
    }
    Ok(())
}

fn send_text(send: &MarginaliaSendData) -> String {
    let mut out = String::new();
    let title = terminal_block(&send.title);
    if send.dry_run && !send.already_sent {
        if let Some(site) = &send.fetch_first {
            let _ = writeln!(out, "Would fetch the article from {site} first.");
        }
        let _ = writeln!(
            out,
            "Would send \"{title}\" ({}, {} KB) to {}.",
            send.view,
            send.bytes.div_ceil(1024),
            send.host
        );
        return out;
    }
    let _ = writeln!(out, "{} \"{title}\"", send.copy);
    if let Some(sent) = &send.marginalia {
        let _ = writeln!(out, "Open it: {}", sent.read_url);
    }
    out
}

fn sync_text(sync: &MarginaliaSyncData) -> String {
    let mut out = format!(
        "Read highlights on {} item(s): {} new, {} removed.\n",
        sync.items, sync.added, sync.removed
    );
    if sync.forgotten > 0 {
        let _ = writeln!(
            out,
            "{} item(s) are no longer in Marginalia; sending again makes a new copy.",
            sync.forgotten
        );
    }
    for failure in &sync.failed {
        let _ = writeln!(out, "{}: {}", failure.item_key, failure.error);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{highlight_change_text, preview, wrap};
    use mxr_core::id::{AccountId, MessageId};
    use mxr_protocol::{ReadingHighlightChangeData, ReadingHighlightData};

    fn change(
        action: &str,
        dry_run: bool,
        mutation_id: Option<&str>,
    ) -> ReadingHighlightChangeData {
        ReadingHighlightChangeData {
            action: action.into(),
            highlight: ReadingHighlightData {
                id: "h-1".into(),
                item_key: "m:0".into(),
                account_id: AccountId::new(),
                message_id: MessageId::new(),
                view: "issue".into(),
                quote: "A delete is the absence of a row".into(),
                note: None,
                origin: "mxr".into(),
                title: "Tombstones".into(),
                source: "Weekly".into(),
                url: None,
                created_at: chrono::Utc::now(),
            },
            previous_note: None,
            dry_run,
            mutation_id: mutation_id.map(str::to_string),
            undo_unavailable: false,
        }
    }

    #[test]
    fn a_preview_says_what_would_go_and_a_run_says_how_to_undo_it() {
        let preview_text = highlight_change_text(&change("remove", true, None));
        assert!(
            preview_text.starts_with("Would remove \"A delete"),
            "{preview_text}"
        );
        assert!(!preview_text.contains("Undo"));
        let run_text = highlight_change_text(&change("remove", false, Some("mut-1")));
        assert!(run_text.starts_with("Removed "), "{run_text}");
        assert!(run_text.contains("mxr undo mut-1"));
        let cleared = highlight_change_text(&change("note", false, Some("mut-2")));
        assert!(cleared.contains("to no note"), "{cleared}");
    }

    #[test]
    fn a_long_passage_is_cut_on_one_line() {
        let long = format!("{}\n{}", "word ".repeat(30), "more");
        let shown = preview(&long);
        assert!(!shown.contains('\n') && shown.ends_with("…\""), "{shown}");
    }

    #[test]
    fn wrap_keeps_lines_within_the_measure() {
        let text = "Every reader since 2002 shipped the same window: a list of sources on the left and the item on the right.";
        let lines = wrap(text, 30);
        assert!(
            lines.iter().all(|line| line.chars().count() <= 30),
            "{lines:?}"
        );
        assert_eq!(lines.join(" "), text);
        assert!(wrap("", 30).is_empty());
    }
}
