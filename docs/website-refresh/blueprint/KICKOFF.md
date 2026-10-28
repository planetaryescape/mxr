# Start with the first complete explanation and a real synthetic result

Read `AGENTS.md`, `.agents/skills/mxr-development/SKILL.md`, this blueprint's README and all numbered documents. Read the decision log before changing direction. Then read both website research notes. This is documentation/presentation work inside an existing mail runtime, not a new application.

The user-visible completion check for the first rung is that a newcomer can explain why mxr adds value over their existing Gmail connector and reproduce a useful local query in synthetic demo mail. Tests alone do not establish it. (D-WEB-002, D-WEB-021)

1. Re-fetch `origin/main`; compare it to the audit baseline `f39ae66578e3099e464967ab2f042e24b1ad8ec4`. Use a fresh isolated worktree if the current checkout has work. Verify the latest published install and the feature branch before carrying its claims into public copy. Record S1.
2. Inspect the 80-page audit against the current tree and update it for new authored routes. Keep the source paths and URLs in this blueprint unless a new decision justifies a change.
3. Run S2 on a truly isolated synthetic profile. Do not switch BK's current demo, access real mail, authorise accounts, or send mail. Establish query scope, candidate limits and expected output; label canned model replies accurately.
4. Use the approved first-screen direction: existing-agent visitors, one working mailbox, “Put your whole mail history to work”. The concrete comp review resolved Q1–Q3. Preserve the ownership and daemon reuse emphasis in D-WEB-027.
5. Build rung 1 from the existing site. Reuse its CSS, copy/media behaviour and generators. Keep Use my agent linked to the existing For agents page until rung 2's new tutorial is available. Update opening metadata and machine entry copy with the same claim boundaries.
6. Follow the scoped verification in 10-verification.md, including rendered commands, generated documents, reader comprehension and the real deployed route when publication is authorised. Heavy commands use the shared queue with four or fewer workers; persistent previews run outside it.
7. Update HANDOFF.md with source SHA, commands/results, demo evidence, deployment URL or access blocker, unresolved questions and exact next action. Use documentation-refiner for closeout within the changed docs. Do not edit unrelated personal notes or publish private source material.

BK’s subsequent orchestrate invocation authorises normal in-scope commits, PRs, integration and the existing mxr.sh deployment path. Root owns those actions; workers do not push. Use `type: description`, BK as the sole author, and no attribution footer. Do not delete resources or change unrelated settings. (D-WEB-025)

Unresolved questions: Q1–Q3 are approved; remaining defaults and evidence gates are defined in 13-open-questions.md.
