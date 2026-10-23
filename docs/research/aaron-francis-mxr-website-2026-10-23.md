# Daemon depth should make the work easier for each client

BK supplied [Resend’s X post](https://x.com/resend/status/2113422915995869519/video/1). Its own reply links [Caring in the age of AI](https://www.youtube.com/watch?v=PMRIEL3e884), Aaron Francis at Resend Forward. Retrieved the timestamped English transcript with the youtube-transcript skill on 2026-10-23. The unchanged transcript is retained in the private orchestration ledger directory, not published with the site. This assessment comes from the transcript, not a claim to have watched the recording.

## The talk connects depth to useful breadth

At [01:03–02:13](https://www.youtube.com/watch?v=PMRIEL3e884&t=63s), Francis argues that builders should absorb customer difficulty. At [17:02–17:45](https://www.youtube.com/watch?v=PMRIEL3e884&t=1022s), he distinguishes tackling a hard customer problem from choosing a needlessly difficult implementation. At [18:28–21:53](https://www.youtube.com/watch?v=PMRIEL3e884&t=1108s), he favours product depth while retaining care as it expands. His terminal example at [22:50–23:52](https://www.youtube.com/watch?v=PMRIEL3e884&t=1370s) places state in the backend and treats interfaces as thin clients. His dependency warning example at [20:37–22:16](https://www.youtube.com/watch?v=PMRIEL3e884&t=1237s) illustrates noticing a user's problem before an error forces them to investigate.

## mxr should demonstrate the work it takes off the user

These are our application of the talk, not claims Francis made about mxr:

- Explain local ownership as practical access to synced mail and cached content on the user's computer. Remote-server sync continues; initial coverage and attachment caching affect what is available offline. Do not turn “anytime anywhere” into a promise of automatic remote access.
- Show the daemon between providers and clients. A new interface can call existing mail operations rather than implementing provider adapters and synchronization again. It still owns its interaction design, error presentation and accessibility.
- Make one persisted result visible across interfaces. Multiple screenshots alone do not demonstrate shared state; the later draft tutorial must verify the same draft identity.
- Explain the common operation a person uses before naming the protocol or crate. Architecture belongs beside the consequence it enables.
- Judge onboarding by the first useful result and the next available action. If a step requires readers to infer account scope, decode an error or guess whether mail will be sent, the docs still have work to do.
- Keep the website scope on explaining shipped capabilities. The talk does not justify speculative clients or runtime features.

Source baseline: freshly fetched origin/main f39ae66578e3099e464967ab2f042e24b1ad8ec4. SQLite storage is implemented in [store/pool.rs45–106](https://github.com/planetaryescape/mxr/blob/f39ae66578e3099e464967ab2f042e24b1ad8ec4/crates/store/src/pool.rs#L45-L106); common provider operations in [core/provider.rs9–75](https://github.com/planetaryescape/mxr/blob/f39ae66578e3099e464967ab2f042e24b1ad8ec4/crates/core/src/provider.rs#L9-L75); daemon policy before dispatch in [handler/mod.rs574–606](https://github.com/planetaryescape/mxr/blob/f39ae66578e3099e464967ab2f042e24b1ad8ec4/crates/daemon/src/handler/mod.rs#L574-L606). These establish the mechanism, not a measured reduction in client development effort.

Unresolved questions: S2 must still demonstrate an isolated local result; S3 must demonstrate the human/agent draft handoff.
