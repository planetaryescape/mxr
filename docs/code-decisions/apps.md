# Web, Raycast and desktop apps

Paths are relative to the repository root.

### Desktop shell does not register the Tauri opener plugin, and grants no IPC
**Chosen**: Links open through a plain `open_url` function. The shell has no capabilities and no `invoke` handler, so neither the bundled down page nor the bridge origin can call Rust. "Try again" on the down page is a navigation that `on_navigation` intercepts.
**Rejected**: The opener plugin. It injects a script that turns every `target=_blank` click into an IPC call and cancels the click, which this shell denies, so links would silently do nothing.
**Where**: `apps/desktop/src-tauri/src/lib.rs`, `open_url` and the builder setup; `navigation.rs`

### The desktop webview may only show the bridge origin and the bundled down page
**Chosen**: Navigation to any other origin is refused and web links go to the default browser. A download is accepted only if it is a blob the web app made or a file the bridge serves, and the saved name uses only the last path component the page suggested, never overwriting an existing file.
**Rejected**: Letting the webview browse freely, or trusting the page's suggested path.
**Where**: `apps/desktop/src-tauri/src/navigation.rs`; `downloads.rs`, `is_from_bridge`, `destination`

### Desktop global shortcut is Control+Option+Command+M
**Chosen**: An unusual four-key chord, overridable with `MXR_DESKTOP_SHORTCUT`.
**Rejected**: Command+Shift+M. It is "Move to" in Mail and "switch profile" in Chrome, and a global shortcut would steal it from every app.
**Where**: `apps/desktop/src-tauri/src/lib.rs`, default shortcut

### Desktop never logs the web URL or stderr from `mxr web`
**Chosen**: stderr can carry the one-time URL, so it is dropped and never logged.
**Rejected**: Logging it for diagnosis.
**Where**: `apps/desktop/src-tauri/src/mxr_cli.rs`, the `mxr web` runner

### The desktop menu command does nothing until the page says it is ready
**Chosen**: The menu runs a script that dispatches an action id once the page has set `mxrMenuReady`. Before that, or on the down page, the command is a no-op.
**Rejected**: Navigating away to run the command.
**Where**: `apps/desktop/src-tauri/src/menu.rs`, `menu_action_script`; `apps/web/src/hooks/useDesktopMenu.ts`

### Raycast reads never mark mail seen or visited
**Chosen**: Raycast lists read the daemon's ready-made views and send no "seen" or "opened" signal. The thread pane only reads and never calls `setRead`, caches nothing, and fetches only for the selected row.
**Rejected**: Marking on read, which would clear the web app's "since you were last here" lines for mail the person only previewed.
**Where**: `apps/raycast/src/bridge/modes.ts`; `apps/raycast/src/components/pane.tsx`

### Raycast sender text is escaped and images never load
**Chosen**: Sender-controlled header text (subject, names, file names) goes on one line and is escaped so it cannot become an image, link or heading. HTML mail is converted without images; an image becomes its alt text, never the remote address. Initials for avatars are restricted to letters and digits because `getAvatarIcon` writes them into SVG unescaped.
**Rejected**: Rendering remote images or raw header text (tracking pixels, markdown injection).
**Where**: `apps/raycast/src/components/body.ts`, `markdown.ts`, `faces.ts`

### Raycast reply is handed to the daemon, not held by the extension
**Chosen**: Send-later hands the reply to the daemon at `sendAt`, and "open in web composer" stores a local draft. Leaving the form must not discard a reply the daemon already owns.
**Rejected**: A client-side timer, which would lose the undo window when Raycast closes.
**Where**: `apps/raycast/src/bridge/compose.ts`; `apps/raycast/src/components/ReplyForm.tsx`

### Compose send takes one per-session lock held in a ref
**Chosen**: A ref lock taken before the first await is held from the first Send press through save, safety check, confirm dialog and undo window until the send settles or is cancelled. Scheduling takes the same lock. Held chords run once.
**Rejected**: React state for the lock (two presses in one tick both see it unset), and relying on the editor's own send hotkey, which bypasses the surface's `busy` gate.
**Where**: `apps/web/src/features/compose/session/useComposeSend.ts`; `composeShortcuts.ts`

### Only the send itself can fail a send
**Chosen**: Reminder, archive and closing the composer after a send report their own trouble. The post-send safety check starts only once the send is committed and is never awaited.
**Rejected**: Letting a later failure reach the send failure path, which would put a sent reply back and invite a second send.
**Where**: `apps/web/src/features/compose/session/useComposeSend.ts`

### Esc and the discard prompt never destroy a draft
**Chosen**: Esc on a proof keeps the user's text and on the help line closes the line. The discard prompt is yes/no (`y` discards, `n` keeps) and Enter runs the focused option, so Enter never discards from Cancel. Leaving a conversation saves the reply onto it and shows no sheet; only a failed save keeps the sheet open.
**Rejected**: Esc discarding, or a sheet thrown over the list on Back (a phone's Back would look dead).
**Where**: `apps/web/src/features/compose/ComposeHost.tsx`; `ComposeEditorPanel.tsx`

### Compose reopen reads the saved file, and a refresh after an outage does not reload it
**Chosen**: Reopening an intent (undo send, reopened reply) reads the saved file, not the first-loaded session. Loading again resets the editor and drops typed text, so it is never part of recovery after an outage. Autosave flushes when the tab is hidden and on teardown, and stops writing once the file is gone.
**Rejected**: Reusing the first-loaded session; waiting for the 3 s debounce on hide or teardown.
**Where**: `apps/web/src/features/compose/useComposeSession.ts`; `session/useComposeAutosave.ts`

### Compose autofocus never takes focus the user already placed
**Chosen**: Focus moves to the composer only from the mail list or reader (where `c` and `r` were pressed), never from a field the user chose.
**Rejected**: Always focusing the editor.
**Where**: `apps/web/src/features/compose/session/useComposeAutofocus.ts`

### The draft text region uses the daemon's body split, and recipients stay one comma-joined string
**Chosen**: `textRegion` cuts the body exactly as `split_body` does (first `--` line or forward header), so web and daemon agree on what the user wrote. The recipient field keeps the comma-joined string as source of truth; names containing a comma fall back to the bare address.
**Rejected**: A web-only split or a structured recipient list.
**Where**: `apps/web/src/features/compose/assist/textRegion.ts`; `RecipientField.tsx`

### A draft whose body is supplied HTML opens as a preview, not in the markdown composer
**Chosen**: mxr preserves such a document byte for byte on the way to the wire, so the bridge refuses to open it. The web shows a sandboxed preview with scripts off and remote URLs blocked, and recognises the refusal by shape rather than `instanceof`.
**Rejected**: Converting the HTML to markdown, which would rewrite the message.
**Where**: `apps/web/src/features/compose/HtmlDraftNotice.tsx`

### Model-written spans stay marked until the user edits them, in one editor transaction
**Chosen**: Spans are owned by the draft and mapped through each edit. Document and marks change in one transaction. Selection is reported only by the Markdown editor so a stale selection cannot steer help after switching to the rich editor.
**Rejected**: Installing marks before the text (they would be checked against the old document).
**Where**: `apps/web/src/features/compose/assist/modelSpans.ts`; `codemirror/CodeMirrorComposeEditor.tsx`; `ComposeEditorPanel.tsx`

### Lists find the lit row by id, never by position
**Chosen**: The lamp row, the cursor and the done-advance are keyed by row id. A row that left falls to the first row until `useAdvanceOnRemoval` moves to the neighbour. A pointer-folded row still draws but no longer counts. A list row's identity is its thread, not the message that represents it.
**Rejected**: Index-based cursors, which jump on refetch, search re-ranking and sync.
**Where**: `apps/web/src/features/now/nowItems.ts`; `mailbox/rowKey.ts`; `hooks/useAdvanceOnRemoval.ts`; `todo/TodoRoute.tsx`

### Long lists window their rows, keep the lit row mounted, and use a stable item key
**Chosen**: A band mounts a screenful (spike S4: with 5,000 people the browser's style and layout, not React, cost a `j`). The lit row stays mounted wherever it is, because a phone hides the list while a person page is open. `getItemKey` is stable per list; an inline function made the virtualizer recompute every position on every scroll frame.
**Rejected**: Rendering all rows; an inline key function.
**Where**: `apps/web/src/features/messages/MessagesList.tsx`; `mailbox/MailboxList.tsx`

### Messages scroller is a callback ref held in state
**Chosen**: A callback ref into state. React attaches a parent's ref after its children's layout effects, so with a ref object the bands mount without their scroller and, with nothing else re-rendering (daemon stopped), stay empty.
**Rejected**: A ref object.
**Where**: `apps/web/src/features/messages/MessagesRoute.tsx`; `hooks/useScrollFade.ts`

### Messages: opening restores an unsent reply only on an explicit open, and view changes keep the pane
**Chosen**: Opening a row with an unsent reply opens the conversation and brings the reply back inline, but moving the lamp with `j`/`k` never takes the keys. Changing a view is not a sync removing the selected conversation, so the pane tree and open conversation are kept across views, never presenting history rows as attention or one account's mail as another's. A default topic page is reused as that topic's page.
**Rejected**: Resuming on selection; resetting the pane on view change.
**Where**: `apps/web/src/features/messages/MessagesRoute.tsx`; `messages/api.ts`

### The focus queue treats a failed source as an error, not a finish
**Chosen**: The queue starts with whichever source answers first and adds the other later. A failed source cannot prove nobody is waiting, so it is a retryable error. The last conversation left is set aside rather than shown again.
**Rejected**: Waiting for both sources, or calling an empty queue finished.
**Where**: `apps/web/src/features/focus/useFocusSession.ts`; `focusQueue.ts`; `FocusRoute.tsx`

### Desk and place sweeps follow the daemon's removal rules
**Chosen**: Archive, snooze, trash and moves remove owed and new-from-people rows as they leave the inbox. A promise stays until resolved whatever happens to its thread, so due rows leave only for trash and spam. Sweep dialogs open on Cancel because `A` sits next to `S` and a slip plus Enter must not archive a whole place.
**Rejected**: Treating every row alike; opening sweeps on the action button.
**Where**: `apps/web/src/features/desk/deskRows.ts`; `places/SweepDialog.tsx`

### Undo is a single slot that is claimed, retired once, and never reaches past an irreversible change
**Chosen**: `u` undoes the newest reversible action (a send in its window or the last mail change). Running an undo retires it, from key or toast, and clears only itself. A change that cannot be undone retires the previous undo. Each verb is either reversible or marked irreversible with a reason and always confirms first.
**Rejected**: A history stack, which lets `u` undo an older action than the user expects.
**Where**: `apps/web/src/features/mail-actions/mailUndo.ts`; `verbFeedback.ts`; `state/undoStore.ts`

### Mutation toasts and refreshes go through one serial queue; large batches run as daemon jobs
**Chosen**: Every mail mutation records a pending operation (instant UI), goes through the serial queue and refreshes every mail query family. Batches over a threshold run as a daemon job (TUI parity) so the daemon chunks provider calls and reports progress. Archive does not project optimistically; the row stays until the daemon confirms, so a second `e` on the same row cannot double-send.
**Rejected**: Parallel requests per surface; optimistic archive.
**Where**: `apps/web/src/features/mail-actions/mailMutations.ts`; `mailMutationJobs.ts`; `mailQueryInvalidation.ts`; `pendingMailOps.ts`

### While the daemon is down, mutations fail at once instead of queueing
**Chosen**: React Query counts as offline when the daemon is down; mutations fail immediately with a toast saying what cannot run and what is safe. "Online" means the daemon answers, replacing the browser's online/offline events.
**Rejected**: React Query's default, which parks mutations and replays them on reconnect (a hidden queue).
**Where**: `apps/web/src/lib/queryClient.ts`; `lib/daemonAvailability.ts`

### One keydown dispatcher, with held-key, text-field and overlay rules
**Chosen**: Only moving and scrolling repeat on a held key (a held `e` must not empty a list). A repeat never starts or extends a chord. Modifier chords work from text fields and closing dialogs, editing chords stay with the field, and Radix dialogs animating closed (still mounted, focus inside) no longer own keys. A palette or dialog owns the keyboard from the moment it opens, and type-ahead after the palette key is inserted as input so Enter picks from the filtered list. Single-character shortcuts turn off with `single_keys = false` (WCAG 2.1.4). Off macOS Ctrl is `Mod`, and browser essentials are never claimed for a Ctrl chord.
**Rejected**: Per-page key listeners (kept only in an allowlisted test list), and bare-letter shortcuts that cannot be disabled.
**Where**: `apps/web/src/lib/keys/dispatcher.ts`; `typeAhead.ts`; `chord.ts`; `hooks/useKeyDispatcher.ts`

### Keymap: a cached served keymap is rebuilt field by field, and a rejected one falls back to defaults
**Chosen**: The cached record is untrusted, so each entry is shape-checked and dropped if wrong. A keymap this build's registry rejects (a daemon from another release) must not take the keyboard away. Cached keys are applied before first render; the daemon's list arrives after first paint without blocking.
**Rejected**: Trusting browser storage; waiting for the request before painting.
**Where**: `apps/web/src/lib/actions/servedKeymap.ts`; `catalog.ts`

### Group and scope tables are Records so the compiler enforces them
**Chosen**: Records rather than arrays: adding a group or scope to the union without updating the table fails compilation. The Rust side checks scopes but not groups, so a misspelt group would otherwise appear in help as a new section.
**Rejected**: Arrays of known values.
**Where**: `apps/web/src/lib/actions/definition.ts`

### Web mirrors of daemon rules are kept in step by tests
**Chosen**: Some rules exist in both the daemon and the web: freshness words, the text region split, the sidebar lens ids, and the invite card (mirrors the TUI variant). Tests restate the daemon's cases. See issue `web-mirrors-of-daemon-rules`.
**Rejected**: Asking the daemon on every render; a shared crate for the TS side.
**Where**: `apps/web/src/features/freshness/copy.ts`; `compose/assist/textRegion.ts`; `mailbox/lenses.ts`; `thread/InviteCard.tsx`

### The mailbox sanitizer treats untrusted HTML and untrusted storage as hostile
**Chosen**: Markup placed in the app's own DOM drops style, class, id and name attributes (they could restyle or shadow the app), allows only web and mail links, and shows blocked images as a small note instead of an empty box (the sender learns nothing and text does not reflow twice). Message HTML runs in a sandboxed frame without scripts; keyboard focus is handed back by the page (`messageFrameFocus`). Assistant markdown is sanitised first and walked into elements. Only web addresses open from reading verbs.
**Rejected**: Trusting sender markup, or a script inside the frame.
**Where**: `apps/web/src/lib/sanitizeHtml.ts`; `features/thread/MessageBody.tsx`; `thread/messageFrameFocus.ts`; `assistant/markdown.ts`; `reading/readingVerbs.ts`

### Links never fetch on their own
**Chosen**: A Reading link opens the same issue or article from the URL but never fetches the article by itself (that tells its site you came). A To do link is highlighted in its email and never opened. The link preview loads nothing until the card is open.
**Rejected**: Prefetching or auto-opening links.
**Where**: `apps/web/src/features/reading/ReadingReader.tsx`; `thread/linkHighlight.ts`; `assistant/useLinkPreview.ts`

### The hover preview is one state machine we own, as a Popover
**Chosen**: `usePreviewOpen` decides when the card opens and closes; Esc anywhere cancels it. The link is the Popover anchor, so Radix adds no click handling.
**Rejected**: A hover library, whose timers run separately for hover and focus, cannot be cancelled together and skip closing during a press.
**Where**: `apps/web/src/features/assistant/usePreviewOpen.ts`; `LinkHoverCard.tsx`

### Secrets never travel over the bridge
**Chosen**: LLM settings store the name of an environment variable, never the key; the panel says which CLI command stores one. The Marginalia token is checked then kept in the daemon's secrets file and never returns to the page. Opening in Marginalia sends the email only on `M`, and opens the tab on the keypress before the send finishes so the browser does not block it.
**Rejected**: Entering or echoing secrets in the page; sending mail text to Marginalia on view.
**Where**: `apps/web/src/features/settings/LlmRoutesPanel.tsx`, `LlmSettingsSection.tsx`, `MarginaliaSection.tsx`; `reading/readingVerbs.ts`

### Cached model output is keyed on the model policy
**Chosen**: The thread gist cache is keyed on what decides which model writes and what it may read, so a settings change never shows an answer written under the old policy. A reply records where it ran so it still says so after the model is switched.
**Rejected**: Keying on thread id alone.
**Where**: `apps/web/src/features/llm/useLlmStatus.ts`; `assistant/Copy.tsx`

### Sound is cached, rare and never audible outside an action
**Chosen**: One AudioContext created on first need and resumed inside a gesture; the player reads a cache and never subscribes; the chime setting is re-read when the page returns to view because the TUI and CLI share it. Sound changes are patches applied by the daemon, and a stale reply cannot overwrite a newer change. No voice outlasts the action; ramps never use a linear or zero set (they click).
**Rejected**: Subscribing the player to settings; a hard gain set.
**Where**: `apps/web/src/features/sound/player.ts`, `voices.ts`, `feedback.ts`, `api.ts`; `settings/SoundSection.tsx`

### Hints show once, one at a time, and never on arrival
**Chosen**: At most one hint at a time; a hint waits until the user has pressed a key or clicked on this page; dismissing one never reveals another. Dismissal writes to the daemon so every client agrees; the cache flips first and flips back on a failed write. Private windows show the hint again next visit.
**Rejected**: Per-browser dismissal only; stacking.
**Where**: `apps/web/src/features/hints/hintSlot.ts`, `api.ts`; `arrivals/useTrustHint.ts`

### The service worker caches reads only, never credentials
**Chosen**: Cache only the read URL, never bearer-token headers, only reads needed to reopen a conversation (never auth, drafts or actions). A token change deletes cached reads directly because the page may have no worker yet. A read finishing after sign-out cannot recreate the private cache. Old HTML is not kept per URL because hashed assets may be gone; offline navigations return the current precached shell.
**Rejected**: Caching per-URL HTML; keeping reads across accounts.
**Where**: `apps/web/src/pwa/service-worker.ts`; `pwa/offlineReads.ts`

### Remote bridge links need explicit consent and never carry the local token
**Chosen**: A `#remote=` fragment on any page may only point the app at another bridge after the user consents; a remote link brings its own token or none.
**Rejected**: Honouring the fragment silently.
**Where**: `apps/web/src/lib/tokenStorage.ts`

### Bundle splitting: few shared chunks, no named chunks for the compose editors
**Chosen**: Small shared modules are merged (on Fast 4G at 150 ms RTT the Messages route pulled about thirty 1-7 kB files before first paint). Named vendor chunks are only for libraries every route loads. The compose editors stay unnamed, because naming @tiptap and @codemirror also pulled their whole dependency trees into the shared chunk.
**Rejected**: Default chunking; vendor groups for the editors.
**Where**: `apps/web/vite.config.ts`, `manualChunks`

### The speed gate runs against a production build with one budget
**Chosen**: Keydown-to-paint p95 budget 50 ms (rubric B1), measured on `vite preview` because React's development build adds owner-stack work users never pay. A slower CI runner raises the bar through `MXR_KEY_TO_PAINT_BUDGET_MS`, so the change shows in the workflow. A key must change the screen (cursor, row leaving, etc.) or the press fails.
**Rejected**: Timing the next paint alone, which passes a key the app ignored; loosening the assertion.
**Where**: `apps/web/e2e/helpers/speed.ts`; `playwright.perf.config.ts`

### E2E processes: separate process groups and a shared demo account address
**Chosen**: `e2e-server` runs the daemon and Vite in their own process groups and stops them on SIGTERM because Playwright's default group SIGKILL never reaches them. The account uses the demo's own address so sent mail syncs as outbound. Hints are pre-dismissed because every spec shares one daemon.
**Rejected**: Default process handling; a separate address per spec.
**Where**: `apps/web/scripts/e2e-server.mjs`; `playwright.config.ts`

### `/api/v1/health` is exempt from the bearer requirement in the OpenAPI doc
**Chosen**: It is the one tokenless route, so it does not inherit the document-level bearer requirement.
**Rejected**: Marking all routes secured.
**Where**: `site/scripts/validate-docs.mjs`

### Unresolved merge markers fail the docs check
**Chosen**: Every markdown tree the repo publishes is scanned, because the decision log carried one to main twice.
**Rejected**: Scanning only the docs site.
**Where**: `site/scripts/validate-docs.mjs`
