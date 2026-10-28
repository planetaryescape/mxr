# Rung 5 media provenance and accessible descriptions

## Source boundary

The implementation started from `36db26ab80ccc68e6cb7a4a4bc82787b409c2c81` on branch `t3/video-provenance-descriptions`. HTTPS fetch confirmed current `main` at `cd08cfe5eb72b0959bc623ad15439bbf09cbe2c3`; that is the source SHA for current behavior below. The configured SSH remote rejected its key, so the HTTPS URL was fetched directly without changing the remote.

## What visitors can learn from each recording

`site/src/components/VideoGallery.astro` now pairs each listed clip with a plain-language description, recording provenance, and expandable ordered notes describing visible steps. The notes are authored descriptions, not fabricated verbatim transcripts. Native controls, posters, `playsinline`, `preload="none"`, and the no-video fallback remain. There is no autoplay or JavaScript added.

`site/src/content/docs/videos.mdx` says the recordings are silent and synthetic, identifies the recorded-agent excerpts and older TUI/composite boundaries, and explains that the notes make terminal steps readable without relying on the video. No clip is presented as a current model-quality test.

Every `marketing/video-library/contact-sheets/{01..15}-*.jpg` contact sheet was inspected. The original tapes for clips 12–15 were also read. The corrections follow those frames and tapes:

- Clips 01–04 are excerpts from a recorded synthetic-inbox agent session. GPT-5.5 is visible; the recording date and mxr version are not identified. The notes describe the search, selected header checks, and recorded answer without calling it live or current model evidence.
- Clip 05 shows one search page with 50 displayed results from 97 matches in a synthetic 50,000-message dataset. It makes no timing claim.
- Clips 06–07 include the terminal's “open-rate” label. Current analytics documentation says the value is the count of messages with local `READ` set, and the command ranks only the bounded returned set: `site/src/content/docs/guides/analytics.md` at `cd08cfe5eb72b0959bc623ad15439bbf09cbe2c3`, lines 119–147. The storage query computes `opened_count` from the READ bit and applies a result limit (`crates/store/src/message.rs`, lines 1084–1153); CLI ranking happens after the bounded result is returned (`crates/daemon/src/commands/subscriptions.rs`, lines 95–117). The copy calls this a read-state proxy, not distinct opens.
- Clips 08–09 show older TUI panels. Their copy identifies the historical navigation and does not present it as the current default layout. Clip 09's visible overview says no action was recorded.
- Clips 10–11 are edited composites of separate TUI, CLI, and agent recordings. Their notes do not claim a continuous handoff or shared-state proof. Clip 11 is no longer presented as evidence of Rust implementation because no source code is shown.
- Clip 12's tape runs `mxr archive ... --dry-run`; the frame lists two selected threads and two messages. The gallery title and copy identify it as a CLI preview, not an agent run.
- Clip 13's tape first lists two synthetic accounts, then searches one sender address without an account selector. Copy does not claim combined cross-account scope or real-provider behavior.
- Clip 14's query is visible but its JSON result is not legible in the contact sheet. It is excluded from the featured gallery and catalogued as archive-only. The original MP4, poster, and post copy remain.
- Clip 15's tape searches one synthetic sender with `has:attachment` and the contact sheet shows two result rows. It does not show downloading, opening, or filing a file.

The catalog and post copy were aligned with those descriptions. The stored MP4s were checked with `ffprobe -v error -select_streams a -show_entries stream=codec_type -of csv=p=0`; all 15 have no audio stream. No captions with invented timings, new recordings, stills, or rendered assets were added.

## Validation

The final site build passed with `agent-work run -- env PATH=/opt/homebrew/opt/node/bin:/usr/bin:/bin npm run build` from `site/`: 10 existing script tests passed, docs validation accepted 265 OpenAPI paths, Astro built 209 pages, and the docs exporter wrote 205 Markdown siblings. The build used the committed `site/public/openapi.json` fallback because Cargo was unavailable; it does not establish live API conformance.

The generated `site/dist/videos/index.html` was parsed: 14 listed recordings each have `controls`, `playsinline`, `preload="none"`, a rendered poster and MP4 source, a descriptive `aria-label`, a browser fallback link, and a “Read the recording notes” disclosure. Both referenced assets exist in `dist`; clip 14 is absent. A DOM read of the local built page at `http://127.0.0.1:4325/videos/` independently confirmed 14 cards, controls, inline playback, notes, and omission of clip 14 at a 1280-pixel viewport. T3's single snapshot attempt timed out with `Preview snapshot failed: Preview automation snapshot timed out after 15000ms.` The DOM read succeeded. The real page was opened in Dia; no playback or keyboard-activation interaction was tested, so those remain unverified. No real mail, current-model, or live runtime check was performed for these recordings.

The local preview ran with `npm run preview -- --host 127.0.0.1 --port 4325` as PID `70696`. Its exact listener on `127.0.0.1:4325` was stopped after inspection. T3 tab `tab_n_b484908c-2400-42a7-8b40-55a8835e83a1` opened the route; its snapshot timed out, while one DOM evaluation succeeded. The installed `site/node_modules` (`460736 KiB`) and built `site/dist` (`48440 KiB`) remain in this worktree. No other preview listener or queue job is owned by this task.

## Root review correction

Root independently viewed contact sheets 01, 07 and 12. Clip 07 shows a search with `--limit 2000` followed by a separate `mxr subscriptions --rank` command with no explicit limit or account selector. Its notes now distinguish the two commands; the search limit is not the subscription limit, and the search does not supply the ranked set. The recorded version is unidentified, so a current default is not presented as an observed historical setting. Clip 12 visibly previews two threads/two messages; no mutation is shown. The new descriptions preserve existing media assets.
