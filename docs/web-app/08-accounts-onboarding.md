# Phase 8 — Accounts + Onboarding wizard

Goal: a friendly first-run experience that doesn't make the user touch a config file. Provider tiles, OAuth device-code flow for Gmail/Outlook, IMAP credentials path, initial-sync progress with live counter from WebSocket events.

## Deliverables

1. `/onboarding` — multi-step wizard:
   - Step 1: Welcome (one screen, "Connect your first account" CTA)
   - Step 2: Provider tile picker (Gmail / Outlook / IMAP)
   - Step 3a (Gmail/Outlook): device-code OAuth — show code, "open auth URL" link, poll for completion
   - Step 3b (IMAP): credentials form (host, port, username, password, SMTP details, security)
   - Step 4: Initial sync — live counter from WS events ("Synced 1,247 of ~5,000 messages")
   - Final: redirect to `/m/inbox`
2. `/accounts` — list with status dots (connected/syncing/error), default-account marker, last-sync time.
3. `/accounts/new` — same wizard at step 2 (skips welcome).
4. `/accounts/$key` — detail page:
   - Account info (provider, email, status)
   - Test connection button
   - Re-auth (OAuth)
   - Aliases / owned-addresses management
   - Sync settings (frequency? labels-to-sync?)
   - Set default
   - Disable
   - Remove (destructive; confirm modal with "purge local data" checkbox)
5. **Empty mailbox state** — when zero accounts configured, redirect from `/m/*` to `/onboarding`.
6. **Empty mailbox state** — when account exists but no mail synced yet, show "Syncing your mailbox" with live counter.

## Bridge endpoints used

- `GET /api/v1/platform/accounts` — list with status.
- `POST /api/v1/platform/accounts/test` — test connection.
- `POST /api/v1/platform/accounts/upsert` — create or update.
- `POST /api/v1/platform/accounts/default` — set default.
- `POST /api/v1/platform/auth/sessions/start` { provider } — start OAuth device flow, returns session_id, code, url.
- `GET  /api/v1/platform/auth/sessions/{session_id}` — poll status.
- `POST /api/v1/platform/auth/sessions/{session_id}/cancel`
- `POST /api/v1/platform/auth/sessions/{session_id}/complete` — when polling sees "ready"; finalizes the account.

WebSocket events:
- `OperationProgress { operation: "sync", account_id, current, total }` — drives manual sync progress when emitted.
- `SyncCompleted { account_id, messages_synced }` — clears sync progress and refreshes mail.
- `SyncError { account_id, error }` — error state.

## Files

```
src/features/onboarding/
  OnboardingRoute.tsx              # /onboarding
  OnboardingShell.tsx              # progress dots + step container
  Step1Welcome.tsx
  Step2ProviderPicker.tsx
  Step3aOAuthDevice.tsx            # Gmail / Outlook
  Step3bImapCredentials.tsx
  Step4InitialSync.tsx
  ProviderTile.tsx                 # Gmail / Outlook / IMAP card
  useAuthSession.ts                # poll loop
  useInitialSyncProgress.ts        # WS event subscription
  imap-schema.ts                   # zod schema for IMAP form
src/features/accounts/
  AccountsRoute.tsx                # /accounts list
  AccountRow.tsx
  AccountStatusDot.tsx
  AccountDetailRoute.tsx           # /accounts/$key
  AccountInfoCard.tsx
  TestConnectionButton.tsx
  AliasesEditor.tsx
  RemoveAccountDialog.tsx          # blocking confirm with "purge local data"
  AccountActionsMenu.tsx           # dropdown for re-auth / disable / remove / set default
```

## OAuth device-code flow

```ts
// useAuthSession.ts (sketch)
const start = useMutation({
  mutationFn: (provider: ProviderId) =>
    api.POST("/api/v1/platform/auth/sessions/start", { body: { provider } }),
});

const poll = useQuery({
  queryKey: ["auth-session", sessionId],
  queryFn: () => api.GET(`/api/v1/platform/auth/sessions/${sessionId}`),
  refetchInterval: (q) => (q.state.data?.status === "ready" ? false : 1500),
  enabled: !!sessionId,
});

useEffect(() => {
  if (poll.data?.status === "ready") {
    api.POST(`/api/v1/platform/auth/sessions/${sessionId}/complete`).then(() => router.navigate({ to: "/onboarding", search: { step: 4 } }));
  }
}, [poll.data]);
```

UI shows the device code in big monospace + an "Open Google sign-in" button that opens the URL in a new tab. Cancel returns to provider picker.

## IMAP form

Fields: account name, email, IMAP host, IMAP port (993 default), TLS/STARTTLS, username, password (masked), SMTP host, SMTP port (587), SMTP auth (plain/login), SMTP TLS. "Test connection" button hits `/accounts/test` before allowing save.

## Initial sync UI

- Big stat: "Syncing 1,247 of ~5,000 messages" (current of estimate).
- Subline: "This usually takes a few minutes for new accounts."
- Live progress bar.
- "Hide and continue" link → user can navigate away; sync continues in background and the status bar reflects progress.

## Verification

1. Fresh install (no accounts): visit `/m/inbox` → redirected to `/onboarding`.
2. Pick Gmail → device code displayed → click "Open sign-in" → tab opens → complete consent → wizard advances.
3. Initial sync starts → counter ticks via WS events.
4. After completion → `/m/inbox` shows synced messages.
5. `/accounts` lists the new account with green dot + "Synced 2 minutes ago".
6. Click account → detail page → "Test connection" → "OK" toast.
7. Add alias → POST → toast → list updates.
8. Pick "Remove account" → confirm modal with "Purge local data" checkbox → confirm → account gone.

## Decisions

- 2026-05-10 — Device-code flow is the canonical OAuth path. We do NOT do redirect-based OAuth in the SPA — too many cookies/CSRF concerns and the daemon already has a device-code path.
- 2026-05-10 — IMAP form does NOT store the password in the SPA — submit goes straight to the bridge which stores in the system keychain.
- 2026-05-10 — "Hide and continue" lets users start using the app while initial sync continues; status bar provides a non-blocking progress indicator.
