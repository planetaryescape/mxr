# Assistant follow-ups

Open work on the assistant (blueprint [25](../blueprint/25-assistant.md), D122). Status: two open items: a model limit that the app makes visible but can't remove, and opening a linked email from the TUI.

## The TUI history list can't delete

Resolved. In the list `L` opens, `D` asks the daemon for a dry run of deleting the highlighted conversation, shows what goes, and deletes it on `y`, through the same `DeleteAssistantConversation` request the CLI and web use. Deleting the open conversation leaves a fresh one. A refusal, such as a conversation that is still running, shows under the list.

## TUI approvals don't show the exact call

Resolved. `x` on a waiting approval unfolds the tool and its arguments as indented JSON, and the raw preview when the card can't put it in words, as the web's "Exact call" does. The next approval starts folded.

## Toasts cover the TUI pane's hint line

Resolved. While the pane is open, toasts stack bottom-right of the area left of it, so the pane's key hints stay visible.

## Small models sometimes say they did what failed

Open, mitigated. Testing with `gemma4` (Ollama), the model once said it had starred an email after both calls failed. Three changes make this visible and less likely: a call mxr's own guard refuses (`"blocked": true`, such as a change without `confirm`) is stored as an error, so the model reads it as a failure and the step shows as failed, including in conversations stored before the change; the system prompt says to report a change only when its result shows it succeeded; and folded steps read "N steps · N changed · N failed" right above the reply. Checking each reply against its steps would need a second model call per turn, which isn't worth it yet.

## Addresses in replies became mailto links

Resolved. Markdown autolinked addresses into `mailto:` links that opened another mail app. Addresses now stay plain text.

## Steps showed raw JSON

Resolved. An opened step showed `{}` for a call with no inputs and the result as the first 4,000 characters of minified JSON, cut mid-object. The daemon now reads each result whole into a readout (how many things it lists, the first five with a link to each, or its plain fields), and the web shows that with the inputs as words. The raw call and result sit under "Raw call". In the TUI, an open step adds one line of what it found.

## Replies named emails but didn't link them

Resolved. Replies said "Q1 Report review" and the user had to go and find it. The prompt asks for `mxr:thread/<id>` and `mxr:person/<id>` links, and the daemon adds them when the model doesn't (testing with `gemma4`, it never did): a subject or name the reply quotes exactly is linked when one tool result in the conversation gave it to one email or person. A link whose id no result returned becomes plain text. The web opens them in the app beside the assistant.

## The TUI can't open a linked email

Open. The TUI shows a reply's links and a step's rows as words. Opening one would need a way to pick a link in the pane (a link cursor, or numbered links and a key), which the pane doesn't have yet. Until then, the email is one search away.
