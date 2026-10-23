# The task-owned navigation and mode journey are live

Verified 23 October 2026. [PR #695](https://github.com/planetaryescape/mxr/pull/695) merged at 20:16:30 UTC as `16b1adaed540bd0fdd9e2d447ecd6f6711abcf60`. The fetched main tree `c3c8aa09275eeefe78d9965ddd9b50b3b5b2d7e9` exactly matches reviewed head `848bbde9363c7078633119c43c63ad0225dfeee8` and the computed merge result. BK is the sole author; the existing admin-merge authority was used with an explicit footer-free commit body.

The existing Vercel project `mxr-mail` reports READY production deployment `dpl_HSYJei85yL7Hn8e1ueTs1B9yrPLS`, [deployment](https://mxr-mail-otp298dmd-planetaryescape.vercel.app), source `16b1adaed540bd0fdd9e2d447ecd6f6711abcf60`, with aliases `mxr.sh` and `www.mxr.sh`. A bounded 45-second wait first ended while building; a later deployment lookup confirmed READY. No redeploy or hosting settings change was needed.

## The public route keeps the five jobs together

Root opened [mxr.sh](https://mxr.sh/) through T3 and clicked `See how people use mxr`. It reached [Your day](https://mxr.sh/guides/your-day/), with links to Now and all five modes. The Now, Messages, To do, Updates, Reading and Archive guides each returned 200 with the expected heading and further guide links. Root followed the Now link in the actual page. The public human guide was also opened in Dia.

On Your day, Start here and the active Work with your mail group were expanded, with the six inactive top-level groups collapsed. Root focused Search, understand and automate and pressed Enter; its disclosure opened. Focusing Search and pressing Enter opened the search dialog with its input focused; Escape closed it. Recursive loaded, and the measured document and viewport widths both equalled 1280px. This is desktop browser evidence, not a phone interaction pass.

Your day, email modes and glossary Markdown exports and curated `llms.txt` returned 200. The [implementation record](rung3-modes-navigation-implementation.md) contains the 81-authored-page inventory, 80-route navigation coverage, 1,844 internal route/anchor checks, final build and static 390px dark/light layout evidence. Static mobile captures do not establish interactive phone navigation.

The ready-event Docs Build, Secret Scan and Release Gate Scripts passed. Earlier draft jobs were cancelled by CI concurrency and superseded. The six protected Rust checks were skipped for docs-only scope. The Vercel comment reported READY; CodeRabbit's existing usage-limit comment contained no findings. No second-opinion review was run after BK waived it.

Root verified and stopped only its owned Astro PID 1737 on port 4323; the listener closed. All worktrees, generated output, fixtures, private QA captures and backups remain retained without deletion. The website changes do not establish a new binary release or runtime replay. At the following source check, GitHub's latest published release remained v0.6.158; the guide boundaries after that version remain accurate.

Rung 4 may now implement workflow, architecture, authority, activity and model documentation using the [accepted source and proof map](rung4-source-and-proof-map.md). Prior S2 and S3 observations remain separate v0.6.156 synthetic runs, with no invoice proof or live agent/model run claimed.
