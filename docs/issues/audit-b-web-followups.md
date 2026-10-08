# Audit B web follow-ups

**Status:** open · **Found:** 2026-10-08 at `3f038c4f` on `fix/audit-b-web-p0`

## ⌘K, `/` and `c` can suspend shortcuts during onboarding

In the no-account frame, `AppShell` mounts only `HelpDialog`; the palette,
search and compose hosts are rendered only in the full shell
(`apps/web/src/components/AppShell.tsx:153-171,242-249`). Their actions still
set modal or compose state (`apps/web/src/lib/actions/navigationActions.ts:61-107`),
and `useKeyDispatcher` treats that state as keyboard ownership
(`apps/web/src/hooks/useKeyDispatcher.ts:22-31`). The dispatcher then drops
keyboard actions (`apps/web/src/lib/keys/dispatcher.ts:92-105`). Onboarding
still works through the visible form, but those shortcuts stop working until
the host can mount. Consider guarding these actions in the no-account frame
or mounting their hosts there.

## The first account query briefly shows the full shell

Before `fetchAccounts` returns, `accounts.data` is undefined and
`noAccounts` is false, so the first render takes the full-shell branch
(`apps/web/src/components/AppShell.tsx:99-105,158`). The zero-account frame
appears after the empty result arrives. `AppShell.test.tsx:98-106` waits for
that switch. Consider keeping the frame neutral while the first account query
is pending.

## The no-account frame omits connection status

The minimal frame does not render `OfflineBanner`
(`apps/web/src/components/AppShell.tsx:153-171`); the full shell renders it
above the route (`apps/web/src/components/AppShell.tsx:222-229`). The banner
communicates a stopped daemon, a protocol mismatch, or a prolonged stream
disconnect (`apps/web/src/components/OfflineBanner.tsx:10-16,32-69`). Consider
providing that status during onboarding as well.
