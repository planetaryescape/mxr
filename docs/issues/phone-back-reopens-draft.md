# On a phone, Back from a person with a reply open shows the draft, not the people

Status: open. Found 2026-10-28 on fix/ci-health while fixing `resizable-panes.spec.ts` "a reply drafted on a phone survives going back to the people and reopening", which had failed on main since b2ea14e8b.

## What happens

At 390 px, open a person in Messages, start a reply inline and type some text. Then tap "Back to the people". The people list renders, but the reply's sheet covers the whole screen: Radix dialog `Reply · <subject>`, classes `fixed inset-0` below the `sm` breakpoint. Taps on the list land on the sheet's editor. To reach the people, the user has to close the sheet ("Close composer (saves draft)"), then tap the person and Reply again to resume the draft inline.

## Since

b2ea14e8b ("feat: keep replies inline and resume conversation drafts") changed what `ComposeHost` does when the reply's inline slot disappears. Before it, an inline draft whose thread was gone docked as a non-modal window. On a phone that window was only its title bar (`dockedBar`), so the list stayed usable and reopening the person brought the draft back inline. Now `visibleSurface` falls back to `"overlay"`, the sheet: "If navigation removes the conversation slot, keep the session visible in its sheet" (`apps/web/src/features/compose/ComposeHost.tsx`).

## Evidence

- Playwright, Chromium at 390×844 against main d85fdeb68: after Back, `row.click()` timed out for 30 s. Each attempt reported that the `Message body` textbox from `<div role="dialog" aria-label="Reply · Contract renewal" class="fixed z-40 … inset-0 sm:inset-x-auto …">` intercepts pointer events. This is from the error context of the 2026-10-28 local full run. CI run 40672677511 timed out on the same test at 0cfa64bad, both attempts.
- The spec now follows the current behaviour: the draft is in the sheet after Back, closing it saves the draft, and Reply resumes it inline.

## Why it reads as "Back did nothing"

On a phone, the sheet and the inline reply look much the same, and both fill the screen around the editor. Tapping Back changes what sits under the sheet, which the user can't see. The draft is safe, but the control the user pressed seems to have no effect, and the way to the list ("close the composer") isn't the control they reached for.

## Options

- Restore the phone docked bar: a draft whose slot is gone shows as a title bar above the tab bar, and tapping it reopens the sheet. This is close to the behaviour before b2ea14e8b.
- Treat Back as leaving the draft: close the sheet (saving, as the close button does) and say so in a toast with "Resume", or rely on the resume-on-Reply that b2ea14e8b added.
- Keep the sheet, but have Back close the reply first and leave the person page; a second Back reaches the list.
