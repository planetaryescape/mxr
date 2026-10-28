# Assistant follow-ups

Open work on the assistant (blueprint [25](../blueprint/25-assistant.md), D122). Status: nothing open.

## The TUI history list can't delete

Resolved. In the list `L` opens, `D` asks the daemon for a dry run of deleting the highlighted conversation, shows what goes, and deletes it on `y`, through the same `DeleteAssistantConversation` request the CLI and web use. Deleting the open conversation leaves a fresh one. A refusal, such as a conversation that is still running, shows under the list.

## TUI approvals don't show the exact call

Resolved. `x` on a waiting approval unfolds the tool and its arguments as indented JSON, and the raw preview when the card can't put it in words, as the web's "Exact call" does. The next approval starts folded.

## Toasts cover the TUI pane's hint line

Resolved. While the pane is open, toasts stack bottom-right of the area left of it, so the pane's key hints stay visible.

## Small models sometimes say they did what failed

Resolved. Each reply now carries `failed_call_ids`: the steps right before it that failed and that no later step among them redid (the same tool and arguments, apart from `confirm`, so a change mxr refused for lacking confirmation and then made with it doesn't count). The daemon works this out once, and every client names those steps under the reply: "This step failed, whatever the reply says:" in the web and TUI, "! failed, whatever the reply says:" in the CLI. On the `gemma4` conversation where the model said "I've starred the email from Quinn and marked it as read" after both calls failed, that reply now carries the note, and the turn where a retry did star it doesn't. The model can still write the wrong thing; the app no longer lets it stand unanswered. Before this: Testing with `gemma4` (Ollama), the model once said it had starred an email after both calls failed. Three changes make this visible and less likely: a call mxr's own guard refuses (`"blocked": true`, such as a change without `confirm`) is stored as an error, so the model reads it as a failure and the step shows as failed, including in conversations stored before the change; the system prompt says to report a change only when its result shows it succeeded; and folded steps read "N steps · N changed · N failed" right above the reply. Checking each reply against its steps would need a second model call per turn, which isn't worth it yet.

## Addresses in replies became mailto links

Resolved. Markdown autolinked addresses into `mailto:` links that opened another mail app. Addresses now stay plain text.

## Steps showed raw JSON

Resolved. An opened step showed `{}` for a call with no inputs and the result as the first 4,000 characters of minified JSON, cut mid-object. The daemon now reads each result whole into a readout (how many things it lists, the first five with a link to each, or its plain fields), and the web shows that with the inputs as words. The raw call and result sit under "Raw call". In the TUI, an open step adds one line of what it found.

## Replies named emails but didn't link them

Resolved. Replies said "Q1 Report review" and the user had to go and find it. The prompt asks for `mxr:thread/<id>` and `mxr:person/<id>` links, and the daemon adds them when the model doesn't (testing with `gemma4`, it never did): a subject or name the reply quotes exactly is linked when one tool result in the conversation gave it to one email or person. A link whose id no result returned becomes plain text. The web opens them in the app beside the assistant.

## The TUI can't open a linked email

Resolved. The latest reply with links numbers them after their words, "Q1 report [1]" (the same email keeps one number, up to nine), and with no question waiting `1`-`9` opens one: an email in the reader at its latest message, wherever it now is, and a person selected in Messages, or a note when Messages doesn't list them. The pane stays open and the keys go to the screen. Other replies keep their links as words, so a number only ever means one link; a later reply without links ("anything else?") leaves the earlier one's numbers in place. A step's readout rows still show as words in the TUI.

## A provider's error body filled the panel

Resolved in v0.6.153 (#677). A turn sent no output cap, so OpenRouter reserved Claude Sonnet 5.5's whole 65,536-token output and answered `402 Payment Required` for a key that could afford 62,047; the panel showed that JSON body as the error. Each model call now asks for at most 16,384 tokens (`Limits::default`, `crates/assistant/src/turn.rs`). `plain_error` (`crates/daemon/src/assistant/view.rs`) turns a 402 about credits into "The model's provider wants more credits for this request", and any other unknown error over 200 characters or containing JSON into "The model's provider returned an error". The raw text stays under **Details**.
