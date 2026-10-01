# Experience rubric: what stands between v2 and its pass bar

**Status:** open · **Found:** 2026-09-28 (builder re-score), regraded
2026-09-30 by an independent reviewer (see "Independent grade" in
`docs/web-app-experience-rubric.md`).

Under v2 the web app fails its pass bar: 11 of 31 criteria are below 2, and
section A is capped at 2 until a dogfooding log exists. In order of impact
on the user:

1. **Draft provenance (C1, C2).** Put model locality, whether history was
   used, and inspectable sources on generated drafts, with a browser
   journey.
2. **Timed deferral (A5).** Reply later and waiting-on as timed, one-key
   flows, including the "no reply" return.
3. **Owed under 1 s (D2).** Measure `mxr owed` on the real mailbox and
   bring the warm run reliably under the budget.
4. **Real-mailbox scroll (D4).** Measure scrolling and key-to-paint against
   the real bridge, with gist-filled rows.
5. **Speed gate coverage (B1).** Add `s` and cached-thread open, and assert
   the intended change before accepting a sample. Built in `feat/polish`;
   awaiting a grade.
6. **Real-model gists (A11).** Time to useful coverage and ask accuracy on
   representative conversations; browser journeys stub the model today.
7. **Daemon stopped (B8).** Stop the daemon mid-session and assert cached
   reading and keys still work, then recover. Built in `feat/offline`
   (`daemon-stopped.spec`); awaiting a grade.
8. **Undo matrix (B3).** Cover send and unsubscribe, and document what is
   truly irreversible. Built in `feat/polish`; awaiting a grade. Open: `u`
   after a star (`docs/issues/web-star-undo-gaps.md`).
9. **Motion and typography audit (B2, B7).** Remove `transition-colors` in
   `Sidebar.tsx` and the literal 1500 ms duration in `app.css`; tighten
   `reading.spec` to 60–80 characters; record a 1440 and 390 px review.
   Built in `feat/polish` (`docs/web-visual-review-2026-10-01.md`);
   awaiting a grade. Open: Formatted HTML's measure.
10. **Dogfooding (E2).** BK's own five working days, logged in `docs/`,
    before any section-A score can reach 3.

Still at 2 from the builder re-score, needing a person rather than code:
sound on real speakers (B4), swipes on a real phone (B9), and an independent
copy review (C3).
