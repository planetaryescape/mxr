---
title: Open Reading items in Marginalia
description: Send a newsletter or article from Reading to Marginalia, read it with Marginalia's tools, and get the highlights you make there back in mxr.
---

[Marginalia](https://github.com/planetaryescape/marginalia) is a reading app
with themes, margin notes, a reading companion and highlights. mxr's Reading
is where your newsletters arrive. When you want Marginalia's tools for an
item, press `M`: mxr sends the item there and opens it. Highlights you make
in Marginalia come back to the same item in mxr, where search and the
Markdown export find them.

Nothing goes to Marginalia until you press `M` on an item. The first press
sends that item's text, which gives it to Marginalia and whichever AI
provider your Marginalia account uses. Every later press only opens it.

## Connect Marginalia

You need Marginalia's address and a personal token.

1. In Marginalia, open **Account**, then **Connected apps**. Name the token
   (for example "mxr on my laptop") and copy it. It starts with `mrg_` and
   is shown once.
2. In mxr's web app, open **Settings**, then **Marginalia**. Enter the
   address, such as `https://reader.example.com`, paste the token and
   choose **Connect**.

From the command line, the token is read from a hidden prompt, or from
standard input when you pipe it in:

```bash
mxr reading marginalia connect --url https://reader.example.com
printf '%s' "$MARGINALIA_TOKEN" | mxr reading marginalia connect --url https://reader.example.com
```

mxr checks the token with Marginalia before saving it. The address goes in
config as `reading.marginalia_url`; the token goes in mxr's secrets file,
never in config or logs. Only `https` addresses are accepted, apart from
`http://localhost` for a copy of Marginalia on your own machine.

```bash
mxr reading marginalia status
mxr reading marginalia disconnect
```

Disconnecting removes the token. What you already sent stays in Marginalia.

## Open an item

In the edition or the reader, press `M`, or choose **Open in Marginalia**
in the reader. The web app opens a new tab; the TUI opens your browser.

What gets sent:

| The item | What Marginalia receives |
|---|---|
| An essay or issue | The issue as cleaned HTML, with its source as the author and its date |
| A digest link or teaser whose article is saved | The saved article, with its byline and address |
| A digest link whose article isn't saved | The article, fetched first from its site, named before it is contacted |

Preview a send from the command line. A dry run names the text and where it
would go, and contacts nothing:

```bash
mxr reading marginalia send ITEM --dry-run
mxr reading marginalia send ITEM --open
```

An item is sent once. Pressing `M` again opens the same article in
Marginalia; `mxr reading open ITEM --format json` shows it under
`item.marginalia`.

## Highlights come back

Highlights you make in Marginalia are read back into mxr:

- when you open a sent item in mxr's web reader,
- after each sync, for items sent in the last 60 days, at most every half
  hour,
- when you ask:

```bash
mxr reading marginalia sync
mxr reading marginalia sync ITEM
```

They appear in the reader marked "from Marginalia", and in
`mxr reading export`. A highlight deleted in Marginalia is removed from mxr
at the next read. Highlights you make in mxr are never changed by this, and
nothing you do in mxr changes Marginalia.

If you delete an article in Marginalia, mxr notices on the next read and
forgets that it was sent; pressing `M` again sends a fresh copy. Deleting
the email in mxr deletes its highlights in mxr, including the ones read back
from Marginalia, and leaves the article in Marginalia.

## For agents

The MCP tool `mxr_reading_marginalia` reports the connection
(`action = "status"`), sends an item (`action = "send"`, a dry run unless
`confirm = true`) and reads highlights back (`action = "sync"`). Connecting
needs the user's token, so it is done in the CLI or web app.
