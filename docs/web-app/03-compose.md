# Phase 3 — Compose

Goal: a full-page compose flow that survives page reloads, supports drag-and-drop attachments, auto-saves drafts, and ships with **CodeMirror 6 + vim** as the default body editor and **Tiptap** as a code-split alternate.

## Deliverables

1. `/compose/new` page (centered 720 px column on canvas-elevated).
2. `/compose/$draftId` resumes an existing draft.
3. `?reply=$messageId&mode=single|all|forward` opens compose pre-filled with quoted body and recipients.
4. **Account picker** popover at top-left.
5. **Address chips** for To / Cc / Bcc with contact autocomplete.
6. **Subject** field.
7. **Body editor** — default CodeMirror 6 + `@replit/codemirror-vim` with Markdown highlighting; alt Tiptap WYSIWYG. Both lazy-loaded; only the active one in the bundle.
8. **Snippets**: `;name<space>` expansion in CodeMirror; popover/menu in Tiptap.
9. **Attachments**: drag-and-drop overlay over the body when files dragged on; paperclip button as fallback. Shows attachment chips with size + remove.
10. **Auto-save**: debounced 3 s after idle.
11. **Send confirm modal**: blocking, "Send to N recipients via $account?" with `Cmd-Enter` submitting the form (which opens the modal, not sending immediately) and Enter confirming the modal.
12. **30-second outbound undo pill** in status bar after send, click to cancel (uses `mxr unsend` semantics).
13. **Editor preference**: `compose.editor` in UI prefs (`"codemirror-vim" | "tiptap"`). Settings page lets users switch.

## Bridge endpoints used

- `POST /api/v1/mail/compose/session` — start a compose session, returns draft id.
- `POST /api/v1/mail/compose/session/refresh` — refresh from server (for race-free editing).
- `POST /api/v1/mail/compose/session/restore` — load existing draft into a session.
- `POST /api/v1/mail/compose/session/update` — save body/recipients/subject (auto-save).
- `POST /api/v1/mail/compose/session/save` — explicit save.
- `POST /api/v1/mail/compose/session/attachment` — upload a browser-dropped file to a local temp attachment path.
- `POST /api/v1/mail/compose/session/send` — actually send.
- `POST /api/v1/mail/compose/session/discard` — drop.
- `GET  /api/v1/mail/drafts` — list drafts.
- Reply prefill via `PrepareReply` / `PrepareForward` in the protocol (bridge surface — verify endpoint name in generated.ts; likely `/api/v1/mail/compose/prepare-reply`).
- Snippets: list via `/api/v1/platform/snippets` (verify) — likely an extension area in `routes_v6.rs`.

## Files

```
src/features/compose/
  ComposeRoute.tsx                # the page
  ComposeShell.tsx                # left-aligned column, account pill, breadcrumb
  AddressChips.tsx                # TanStack-style chip input with autocomplete
  AttachmentDropzone.tsx          # full-area drag overlay
  AttachmentList.tsx
  SnippetsPopover.tsx
  SendConfirm.tsx                 # the blocking modal
  OutboundUndoPill.tsx            # mounted in StatusBar via portal
  useComposeSession.ts            # session lifecycle
  useDraftAutosave.ts
  EditorSwitcher.tsx              # picks codemirror vs tiptap
  codemirror/
    CodeMirrorComposeEditor.tsx
    extensions.ts                 # markdown + vim + theme + keymaps + snippet expansion
    snippetExpand.ts              # ;name<space> trigger
    vim-config.ts                 # custom :w/:q ex commands → save/cancel
  tiptap/
    TiptapComposeEditor.tsx
    extensions.ts
    SnippetExtension.ts
src/state/
  composeStore.ts                 # current draft id + dirty state
```

## CodeMirror 6 + vim setup notes

- Use `@replit/codemirror-vim`'s `vim()` extension.
- Bind `:w` to fire the explicit save mutation; `:wq` saves and navigates away (back to drafts list); `:q!` discards (with confirm if dirty); `:send` triggers the send confirm modal.
- Use `@codemirror/lang-markdown` for syntax highlighting.
- Theme: build a small theme matching our tokens (foreground, background, selection, cursor). Don't use `oneDark` directly — it doesn't match our palette.
- Snippets: a Markdown-area `keymap.of([{ key: "Space", run: trySnippetExpand }])` that when the line ends with `;name` looks up the snippet by name and replaces the prefix with the body. Snippet definitions come from `/api/v1/platform/snippets`.
- Persist cursor position to URL state so reload preserves it.
- `Esc` is reserved for vim mode — page-level keyboard handlers must skip when the editor is focused.

## Tiptap setup notes

- Lazy import: `const Tiptap = lazy(() => import("./tiptap/TiptapComposeEditor"))`.
- Extensions: StarterKit, Link, Placeholder, custom Snippet extension that expands `;name<space>`.
- Content stored as HTML; on send, also generate plain-text alternative via Tiptap's `getText()` for multipart.

## Auto-save flow

```ts
// useDraftAutosave.ts (sketch)
useEffect(() => {
  if (!dirty) return;
  const t = setTimeout(() => updateSession.mutate(serializeDraft()), 3000);
  return () => clearTimeout(t);
}, [dirty, draft]);
```

Save status in the right-side metadata strip: "Saved 3s ago" / "Saving…" / "Unsaved changes".

## Reply / Reply-All / Forward

- Route `/compose/new` accepts search params `reply=$messageId&mode=single|all|forward`.
- On mount, call `PrepareReply` / `PrepareForward` to get pre-filled recipients, subject, quoted body.
- Cursor positioned at top of body, above the quote (CodeMirror: `cursor.setSelection(0)`).
- Subject prefixed `Re: ` / `Fwd: ` (the bridge does this; trust it).

## Send flow

1. User hits `Cmd-Enter` (or clicks Send).
2. Validate locally: at least one recipient, subject present (warn if empty).
3. Open SendConfirm modal: "Send 1 message to 4 recipients via work@gmail?" with Cancel / Send buttons.
4. On Send: POST `/compose/session/send` → bridge schedules the send → returns a draft-id + outbound undo handle.
5. Modal closes. User redirected to `/m/sent`. OutboundUndoPill mounts in status bar with 30 s countdown.
6. If user clicks "Cancel send": POST `/api/v1/mail/compose/session/discard` (or unsend endpoint — verify).

## Verification

1. `/compose/new` → empty form. Type subject + body in vim mode → press `:w` → "Saved" indicator.
2. Type `;sig` then space → snippet expands to user's signature.
3. Drag a file over the page → dropzone overlay → release → attachment chip appears.
4. Hit `Cmd-Enter` → send confirm modal → Enter → modal closes → status bar shows 30s undo pill.
5. Refresh page mid-compose → editor reopens with draft restored from server.
6. `/compose/new?reply=$messageId&mode=all` → recipients + subject + quoted body pre-filled.
7. Settings → switch editor to Tiptap → reload `/compose/new` → Tiptap loads (network tab confirms different chunk).
8. CodeMirror `Esc` → inserts vim NORMAL mode; does **not** trigger global Esc handler.

## Decisions

- 2026-05-10 — Outbound undo (30s) lives in the status bar as a pill, not a toast — toasts can stack and be dismissed; this needs to be unmissable.
- 2026-05-10 — Attachment drag overlay covers the entire compose form (not just body) so dropping anywhere works.
- 2026-05-10 — Tiptap is loaded only on user opt-in. CodeMirror is the default chunk.
