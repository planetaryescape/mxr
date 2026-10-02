# Teaching in place: a new model of email is learned at the moment of use, not on a tour

Research for "The app teaches itself in place, with no tour" in
[22-email-modes.md](../../blueprint/22-email-modes.md). Code references are
at `270dfc07` (`origin/main`, fetched 2026-10-02) unless marked.

BK, 2026-10-02: "since we're changing the email app to something people
aren't used to, we need to explain ourselves a lot and teach people what
the app is what things are and how to use them. I don't mean with a long
intricate onboarding tour, I don't like those."

Each section keeps evidence and inference apart. "Evidence" means a cited
source read for this note. "Inference" means this note's reasoning, which
dogfooding has to confirm. Search was limited: WebSearch's budget was
spent, Exa returned 401 and Brave's key was invalid. Pages were read with
WebFetch or curl, through the Hacker News Algolia API, the OpenAlex and
Crossref APIs, Zendesk help-centre search APIs, and `r.jina.ai` as a proxy.
Sources read through the proxy are marked "(proxy)".

## 1. Tours interrupt, get skipped and are forgotten; help that arrives with the task works better

Evidence:

- NN/g (Laubheimer, 2023) concludes that onboarding tutorials "interrupt
  users, don't necessarily improve task performance, and are quickly
  forgotten". It gives three reasons: people want to start (the paradox of
  the active user), "memory is quite limited" so help shown out of context
  is hard to recall when it's needed, and the tutorial itself is something
  to dismiss. Its example is Chase's deck-of-cards tour, which "did not
  communicate what the specific improvements were or how to use them" and
  stood between stressed users and a fraud problem.
- The same article separates **push revelations**, help shown "out of
  context, without any specific indication that the user would benefit
  from the information at that moment", from **pull revelations**, help
  "triggered by some signal that the user would benefit from that
  information at that moment", from hover tooltips to coach marks. It
  recommends pull. It cites no study by name for the claim that
  walkthroughs don't improve task performance.
- NN/g allows one exception: tutorials helped "for onboarding new users to
  a novel interaction paradigm like AR", while calling them "dramatically
  overused" elsewhere.
- NN/g (Kendrick, 2020) says deck-of-cards onboarding "tends to make the
  interface appear more complicated than it actually is, and strains
  user's memory". If one is used anyway: a visible Skip, few cards, one
  concept per card.
- NN/g (Harley, 2014) on instructional overlays and coach marks:
  "Bombarding users with frequent hint screens causes them to dismiss
  hints more quickly"; chains of tips intimidate and make the app look
  complex; overlays that look like UI confuse people about what is
  interactive. Advice: one interaction per hint, shown when the user
  reaches that part of the flow.
- Apple's Human Interface Guidelines, Onboarding (proxy, changelog June
  2024): "Ideally, people can understand your app or game simply by
  experiencing it." "Teach through interactivity." "Consider providing a
  collection of context-specific tips instead of a single onboarding
  flow." If a tutorial exists, make it optional, and if it is skipped,
  "don't present it again on subsequent launches, but make sure it's easy
  for people to find". Also: "Postpone nonessential setup flows."
- Anecdote: a founder on Hacker News (19807933, 2019) reported that about
  50% of their users skipped the onboarding altogether, while basic
  onboarding still lifted week-one retention from 2% to 10%. Another
  (48030227, 2026) planned a tour, "then remembered that in general I
  skip them".

Inference: the evidence against tours is about tours, not about teaching.
Even the Polar anecdote shows that teaching moved retention while half the
users skipped the vehicle it came in. mxr needs as much teaching as BK
says, delivered as pull revelations attached to each mode and verb.

## 2. People start working at once and stick with habits that already work, so teach where the old habit fires

Evidence:

- Carroll and Rosson named the paradox of the active user in 1987 from
  observations at IBM. Nielsen's summary (NN/g, 1998): "Users never read
  manuals but start using the software immediately. They are motivated to
  get started and to get their immediate task done." Designers "must
  design for the way users actually behave".
- Fu and Gray (Cognitive Science, 2004) studied why experienced users
  keep inefficient procedures. The procedures people prefer are
  "well-practiced, generic" ones that work across contexts, made of
  interactive steps that give "fast, incremental feedback". People are
  "biased towards the use of general procedures that start with
  interactive actions".

Inference: every mxr user arrives with Gmail habits that work: scan
subject lines, chase the unread count, archive to clear. Fu and Gray
predict those habits will keep firing in the modes. So the useful moment
to teach is when an old habit meets a new rule: the first `e` in Updates
("Let go: this source leaves today's digest"), the first look for an
unread count in Reading, the first time Archive the mode is confused with
archive the action. Teaching before that moment is a push revelation.

## 3. Minimal instruction attached to real tasks teaches faster than complete instruction up front

Evidence:

- Carroll's minimalism (The Nurnberg Funnel, MIT Press, 1990; publisher
  description read through the proxy) aims at "rapid achievement of
  realistic projects right from the start of training" and treats "error
  recognition and recovery as basic instructional events, instead of
  seeing error as failure". The four principles as summarised by
  instructionaldesign.org (secondary): start immediately on meaningful
  tasks; minimise reading and let users fill gaps; include error
  recognition and recovery; make each learning activity self-contained and
  independent of sequence. The same page reports a study where 25 task
  cards beat a 94-page manual and "users learned the task in about half
  the time"; this note did not read the primary study.
- Lazonder and van der Meij (International Journal of Man-Machine Studies,
  1993) replicated the minimal manual with 64 students learning a word
  processor: "minimalist users learned faster and better", with no
  interaction between manual type and prior computer experience
  (University of Twente repository abstract).
- Counter-evidence: Kirschner, Sweller and Clark (Educational
  Psychologist, 2006) argue that minimally guided instruction "is less
  effective and less efficient" than strongly guided instruction for
  novices, and that "the advantage of guidance begins to recede only when
  learners have sufficiently high prior knowledge".

Inference: the two lines agree more than they seem to. Minimalism is
strong guidance, cut small and attached to the task; what fails novices is
discovery with no guidance. For mxr this means each mode needs a short,
explicit statement of its job and its verb, placed on the mode, plus
recovery (`u` undo, `X` not here) taught as part of the lesson. "Self-
contained and independent of sequence" fits modes: a user can meet To do
before Messages, so no card may assume another card was read.

## 4. Empty states and inline explanations are where an unfamiliar model gets explained

Evidence:

- NN/g (Kaplan, 2021) gives empty states three jobs: say the system status,
  give learning cues ("in-context help can often be applied right away and
  is thus more memorable"; Datadog's "Star your favorites to list them
  here"), and offer a direct path to fill the space (Loggly offers "adding
  external log sources or populating demo data").
- NN/g (Nielsen, 2006) on progressive disclosure: show "only a few of the
  most important options" first, more on request; more than two levels
  usually hurts.
- NN/g (Kendrick, 2019) on tooltips: "Users shouldn't need to find a
  tooltip in order to complete their task", and hover-only tooltips are
  inaccessible to keyboard users.
- Apple HIG, Offering help (proxy, tips guidance added September 2023):
  tips are "one or two sentences", "direct, action-oriented", for features
  that take no more than three actions, and never promotional. "People
  who've already used a feature won't appreciate viewing a tip that
  describes it", so tips use eligibility rules and a display frequency
  (their example: at most once every 24 hours). Tooltips should "begin
  the description with a verb", stay within 60 to 75 characters, and not
  repeat the control's name.

Inference: an empty state in mxr is shown most to exactly the people who
need teaching: new users, users on the demo, and users whose mode is
clear. It is the cheapest place to state the job ("what lands here") and
the rhythm ("next digest at 16:30"). Retiring a tip once the user performs
the verb, not only when they dismiss it, follows Apple's eligibility rule.

## 5. Item-level "why" explanations build an accurate model of a classifier, if they are complete and true

Evidence:

- Kulesza, Burnett, Wong and Stumpf (IUI 2015, OpenAlex abstract):
  Explanatory Debugging, where the system explains each prediction and the
  user explains corrections back, "increased participants' understanding
  of the learning system by 52% and allowed participants to correct its
  mistakes up to twice as efficiently". The prototype classified text
  messages into topics.
- Kulesza et al. (VL/HCC 2013, OpenAlex abstract): completeness of an
  explanation mattered more than soundness for building a mental model,
  but "when soundness was very low, participants experienced more mental
  demand and lost trust in the explanations".
- mxr already ships the pattern. `whyHere` in
  `apps/web/src/features/places/placeCopy.ts` renders "Here because:
  automated sender, has List-Unsubscribe." on Reading and Paper trail
  rows, `e2e/places.spec.ts` asserts it, and `crates/daemon/src/commands/places.rs`
  prints "here because:" in the CLI. 22 already requires a reason on
  every item and one-key corrections (`X`, `K`).

Inference: the "Here because" line is the strongest teaching surface mxr
has, because it teaches the model through the user's own mail, one item
at a time, and is backed by the best study in this note. It must stay
complete (name the evidence and the source: rule, you, or the model by
name) and true, because Kulesza 2013 found a low-soundness explanation
costs trust. A cute or vague reason ("Looks important") is worse than
none. Adding "what next" to the same line ("Goes in the 16:30 digest")
teaches the rhythm on the item.

## 6. HEY taught four new boxes mostly with plain names, analogies to familiar habits and a public walkthrough

Evidence:

- hey.com/how-it-works headings each state a box's job: "The Imbox is for
  your important email", "The Feed is for your casual, whenever reads",
  "The Paper Trail is where your transactions go". The page offers a
  walkthrough video only to those who ask: "Got 37 minutes? Really
  curious? Here's a full product walkthrough".
- HEY's help pages lean on analogies to behaviour people already have.
  The Screener: "You screen your calls, so why can't you screen your
  emails?" The Feed: "like a Twitter or Instagram feed, but for email",
  with "already open" newsletters, "Just scroll to read."
- HEY doesn't import old mail: "a fresh start is a blessing, not a curse",
  and "99% of it is dead weight" (help.hey.com article 754). New users
  therefore meet each box empty and fill it with mail as it arrives.
- The coined name needed explaining. Hacker News 39022851 (2024): "When
  Hey first launched the explained why its called imbox". A prospective
  user in 23594138 (2020) mixed up the Imbox with the Screener and asked
  whether it would fill with spam, before using the product.
- Users who adopted it describe the categories as a fit for kinds of
  mail (23630955, 2020; 27219428, 2021, from someone who "never used
  folders or labels").
- Not verified: the brief asks about HEY's in-app "first time here"
  panels. Neither the marketing pages nor the 11 Getting Started help
  articles describe them, and no review that mentions them could be read.
  How HEY's in-app teaching looks, and whether it worked, is unknown here.

Inference: three things transfer. Headlines that say "X is for Y" teach
the job in one line. Analogies work when the habit is exact ("screen your
calls"). And a box that starts empty and fills with real mail teaches
itself as it fills, which mxr gets on the first run without HEY's
fresh-start cost. mxr's names are plain words, which avoids the Imbox
problem, except for one collision: Archive the mode and archive the
Gmail action. That collision must be named in Archive's first card and in
every toast that archives in the provider.

## 7. Things and Linear state each list's job in one sentence and let a demo carry the rest

Evidence:

- Things' guide (proxy) defines each list by its job in one sentence:
  "Today is the list for to-dos that you want to start before the day
  ends. They're your priorities." "Someday is the place for to-dos that
  you might like to get to, but you're not sure when."
- Linear's conceptual model page gives one sentence per concept, then
  context: issues are "the fundamental unit of work in Linear", cycles "a
  team's repeating planning period".
- Linear's start guide (proxy) offers a demo workspace, "Changes are local
  to your browser and reset on refresh", beside optional live sessions and
  videos. Its docs state the key with each task: "Use the keyboard
  shortcut C to open up an issue creation modal"; Inbox is "G then I"
  (proxy).
- Not verified: Things 3's empty-state art and copy and any built-in
  tutorial project; the MacStories review read through the Wayback Machine
  says nothing about either. Linear's command menu showing a shortcut
  beside each command could not be confirmed from a first-party page this
  session (`linear.app/docs/keyboard-shortcuts` returns 404).

Inference: a mode needs one sentence that names its job before anything
else, and a safe place to try it. mxr already has the safe place:
`mxr demo` seeds a FakeProvider mailbox (`crates/daemon/src/commands/demo.rs`,
`alex@demo.mxr.local`, 50,000 messages by default) with the canonical
examples 22's checks use (Camden council tax, the landlord). Like
Linear's demo, it should be offered, not imposed.

## 8. Keyboard apps teach keys by showing them beside the action, not by drilling them

Evidence:

- Superhuman's help centre (Keyboard Shortcuts, updated 2026-08-20): the
  first shortcut is Cmd+K, Superhuman Command; "You'll see each action's
  shortcut on the right, making it easy to learn as you go." "You can
  also learn shortcuts by hovering over any button in Mail." "No need to
  memorize every shortcut at once. Start with the actions you do most."
- Raycast's manual (proxy): "Make it a habit to press ⌘K / Ctrl K
  whenever you're unsure what actions are available. Each action shows its
  keyboard shortcut on the right, so you can learn to trigger them
  directly over time."
- mxr today: `?` opens Help in every TUI scope
  (`docs/reference/tui-keymap.json`), and the web `HelpDialog` is
  "generated from the action registry: the current view's keys first"
  (`apps/web/src/components/HelpDialog.tsx`). 22's key table keeps `?` as
  help and rules out a separate "why" key.

Inference: mxr already has both halves of this pattern (a palette and a
generated help dialog). What's missing is the mode's own words at the
top of `?`, and the verb next to each key everywhere the key appears
("e done here", never a bare "e").

## 9. Superhuman's onboarding call taught by doing with a human, and mxr can't copy it

Evidence:

- Superhuman required a 30-minute onboarding call with a team member
  before use (Hacker News 22193775, 2020). Its CEO describes onboarding as
  a game's "tutorial" level (same comment, second-hand).
- A teardown (Flowjam, read through the proxy, a secondary and weak
  source) describes the call as: learn the user's workflow, configure the
  app live, then teach one "magic moment", Cmd+K. It attributes "one
  onboarding specialist adds $650k ARR per year" to a First Round
  interview; the First Round article is paywalled and was not read.
- Some resented the gate: a mandatory "onboarding/sales call" (41389926,
  2024); "VimCal has the same business model as Superhuman but doesn't
  require an onboarding call. So I'm only using Vimcal" (31616236, 2022).
- Superhuman's help centre now offers "this 2-minute Mail Tour", on-demand
  videos, weekly webinars and self-paced guides that "each take less than
  15 minutes" (Learn Superhuman Mail, updated 2026-10-02). When the call
  stopped being required was not confirmed.

Inference: the call worked because a person taught by doing on the user's
own mail, one habit at a time. mxr can't copy it: it is local-first with
no accounts or sales team, users install it from a release, and a human
would need to see the user's mail, which the privacy rules in 22 forbid.
What transfers is the shape: real mail, one magic moment per mode, done
while working. The first-run screen (section 12) is that moment.

## 10. Duolingo and Arc teach by doing first and explain only what the doing raises

Evidence:

- Duolingo (Freeman, 2023): "Learn by doing", with "optional hints and
  bite-sized explanations". Duolingo (Blanco, 2023): implicit learning
  "happens through experience, interaction, and practice", and "Implicit
  and explicit learning can work together effectively"; the explicit part
  lives in "section overviews, unit guidebooks" and the blog.
- Luke Wroblewski's gradual engagement (2010): let people "accomplish
  something relevant to the core of your product within their first one
  or two interactions". Twitter added a step that showed topics to follow
  before a blank feed, and sign-up completion rose 29%.
- Arc's help centre explains an unfamiliar rule by naming its benefit and
  its undo together: "we Auto Archive idle Unpinned Tabs at the cadence of
  your choice. This gives you a fresh start and keeps your sidebar tidy",
  with View Archive and Restore one command away (Auto Archive: Clean as
  you go).
- Not verified: Arc's first-day "learn by doing" onboarding. No
  first-party page describing it was found, and the help centre's search
  returns none.

Inference: show the user's own sorted mail before explaining anything,
then explain the one thing they are looking at. An unfamiliar automatic
rule (items fading, digests, expiry) is taught by stating the rule, why
it helps, and the undo in the same line, as Arc does.

## 11. Apple's Tips app is the out-of-context catalogue mxr should not build

Evidence:

- Tips arrived in iOS 8 and adds "tips and guides" with each major release
  (Wikipedia, List of built-in iOS apps).
- Apple's own guidance moved to context: TipKit and the 2023 HIG tips
  section put tips beside the feature, with eligibility rules and display
  frequency, and the 2024 onboarding guidance prefers "context-specific
  tips instead of a single onboarding flow".
- Criticism found was anecdotal. Hacker News 46133818 (2025): "Occasionally
  the official Apple 'Tips' app has useful stuff, but not much", in a
  comment about features being found by "luck or having a friend tell
  you".

Inference: a "what's new" or "tips" feed, or notifications advertising
features, is a push revelation by design. mxr should have none. Its one
reference page is the glossary, reached on request from `?`, never pushed.

## What works for a new mental model, surface by surface

| Surface | Verdict | Evidence behind it | Strength |
|---|---|---|---|
| One-line mode header | Ship on every mode | Things, Linear, HEY headings; Carroll's minimalism | Moderate (practice, plus minimalism studies) |
| Empty states that teach the job and show what lands there | Ship two per mode: never-had-any and clear-for-now | NN/g Kaplan 2021; HEY boxes start empty | Moderate |
| First-encounter card, one per mode, dismissible, never returns | Ship, retire on dismiss or first use of the verb | Apple HIG tips and eligibility; NN/g Harley 2014 (one interaction, in context) | Moderate |
| Item-level why and what next | Ship on every row; already started | Kulesza 2015 (52% better understanding), 2013 (soundness) | Strong |
| `?` on every mode | Ship by leading the existing help with the mode's explainer | Superhuman, Raycast; Apple HIG "easy to find" | Moderate |
| Handoff toasts that name the destination | Ship as 22 specifies | Inference from Fu and Gray (feedback on interactive steps); NN/g undo over confirmation, cited in 22 | Weak to moderate |
| Key hints at the point of use | Ship: verb beside key in footer, tooltip, palette | Superhuman, Raycast help pages | Moderate |
| Glossary page linked from the app | Ship as the one reference, pulled not pushed | Apple HIG (optional, findable); Duolingo guidebooks | Weak |
| Demo mailbox (`mxr demo`) | Offer on first run and in docs, never required | Linear demo; NN/g Kaplan (Loggly demo data) | Weak to moderate |
| First-run catch-up as the teaching moment | Ship: the one place the whole model is shown, with the user's own counts | Gradual engagement (+29%); Apple HIG "teach through interactivity"; Carroll | Moderate |

## What to avoid, and why

1. **A multi-step tour or deck of cards.** It interrupts, is forgotten and
   makes the app look harder (NN/g 2023, 2020). Half of users skip one
   (Hacker News anecdote). BK has ruled it out.
2. **Chained coach marks and overlays that look like controls.** People
   dismiss them faster the more there are, and confuse them with UI (NN/g
   Harley 2014).
3. **A tips feed, "what's new" notifications or feature announcements.**
   Push revelations by design (NN/g 2023); Apple itself moved tips into
   context.
4. **Anything essential only on hover.** The meaning of the runway bar, the
   act-by date or a fading item must be readable without a tooltip (NN/g
   Kendrick 2019), and hover-only help shuts out keyboard users.
5. **Re-showing a tip after the user has used the feature** (Apple HIG).
6. **A vague or cute reason line.** Low-soundness explanations cost trust
   (Kulesza 2013). Every reason names its evidence and its source.
7. **Coined names that need a gloss.** HEY had to explain "Imbox". mxr
   uses plain words; Archive's collision with archive the action is the
   one name to explain on sight.
8. **Copying Superhuman's call.** It needs a human and the user's mail;
   mxr has neither, and some users resented the gate.
9. **Front-loaded setup questions.** Postpone what isn't essential (Apple
   HIG). The first run asks one question (the catch-up window, D117) with a
   default.
10. **Discovery with no guidance.** Minimal is not absent (Kirschner et al.
    2006). Each mode states its job and verb explicitly.
11. **Nudges about using the keyboard.** No source was found on toasts
    that scold mouse use; this is inference from the push-revelation
    finding. Show the key in the tooltip and stop there.

## Copy rules

Evidence-led rules, with their source:

1. **Say what the mode is for in under 12 words, and include its verb.**
   HEY's "The Imbox is for your important email"; Things' one-sentence
   lists. Twelve words is this plan's limit, not a finding.
2. **Teach the verb, not the feature.** "Let go of this digest", not
   "Digest dismissal". Apple HIG: start with a verb; Carroll: tasks, not
   systems.
3. **Plain words, no coined names, no hype.** No "AI", "smart", "magic",
   "powerful", no exclamation marks, in user-facing copy (BK's voice
   rules). A model is named by name where the trust rules in 22 require it.
4. **One or two sentences per card, 60 to 75 characters per tooltip**
   (Apple HIG).
5. **Every reason names its evidence and its source** ("rule", "you", or
   the model's name), and is true (Kulesza 2013).
6. **Every handoff names the destination and the undo** ("Filed in
   Archive. u undo").
7. **State the rhythm with a time**, not an adjective: "Next digest at
   16:30", not "soon".
8. **Facts, not nudges or guilt.** "Nobody is waiting on you", not "Inbox
   zero!" (rubric C3).
9. **Use an analogy only when the habit is exact** (HEY's "screen your
   calls"). Inference: most mode jobs are plain enough not to need one.
10. **Keys always appear with their verb**: "e done here", "A let go of
    digest".

## Evidence strength

- Strong: tours interrupt and are forgotten while in-context help works
  (NN/g, several articles, consistent with Carroll and Rosson); minimal,
  task-attached instruction teaches faster (Carroll; Lazonder and van der
  Meij, n = 64); explanations of a classifier's decisions improve users'
  understanding and correction (Kulesza 2015, 2013). Experienced users keep
  generic habits (Fu and Gray 2004).
- Moderate: Apple's HIG and Superhuman's and Raycast's help pages are
  first-party practice, not studies. Five-second tests measure first
  impressions, not use (Lyssna guide), so the rubric pairs one with
  dogfooding.
- Weak: everything about HEY's in-app teaching, Things' empty states,
  Arc's first day and Linear's command menu, which this note could not
  read first-hand, plus all Hacker News anecdotes.
- Against: Kirschner et al. 2006 on minimal guidance for novices, and
  NN/g's own exception for novel paradigms. mxr's modes are novel for
  email, which argues for explicit statements of each job, on demand and
  in place, not for a tour.

## Open questions for BK

- Should the first-run screen offer `mxr demo` to people who want to try
  the modes before their own mail is sorted, or only the docs page?
- Is one first-encounter card per mode per profile right, or per client
  (web and TUI separately)? This plan says per profile.
- Who besides BK takes the five-second check? It needs people who haven't
  seen mxr.

## Sources

Research and guidance:

- Laubheimer, P. Onboarding Tutorials vs. Contextual Help. NN/g, 12 Feb 2023. https://www.nngroup.com/articles/onboarding-tutorials/
- Kendrick, A. Mobile-App Onboarding: An Analysis of Components and Techniques. NN/g, 21 Jun 2020. https://www.nngroup.com/articles/mobile-app-onboarding/
- Harley, A. Instructional Overlays and Coach Marks for Mobile Apps. NN/g, 16 Feb 2014. https://www.nngroup.com/articles/mobile-instructional-overlay/
- Kaplan, K. Designing Empty States in Complex Applications. NN/g, 19 Sep 2021. https://www.nngroup.com/articles/empty-state-interface-design/
- Nielsen, J. Progressive Disclosure. NN/g, 3 Dec 2006. https://www.nngroup.com/articles/progressive-disclosure/
- Nielsen, J. Paradox of the Active User. NN/g, 4 Oct 1998. https://www.nngroup.com/articles/paradox-of-the-active-user/
- Kendrick, A. Tooltip Guidelines. NN/g, 27 Jan 2019. https://www.nngroup.com/articles/tooltip-guidelines/
- Carroll, J. M. and Rosson, M. B. (1987). Paradox of the active user. In Interfacing Thought, MIT Press. (Metadata via OpenAlex; not read.)
- Fu, W.-T. and Gray, W. D. (2004). Resolving the paradox of the active user. Cognitive Science. https://doi.org/10.1207/s15516709cog2806_2 (abstract via OpenAlex)
- Carroll, J. M. (1990). The Nurnberg Funnel. MIT Press. https://mitpress.mit.edu/9780262031639/the-nurnberg-funnel/ (proxy)
- Minimalism (J. Carroll). instructionaldesign.org. https://www.instructionaldesign.org/theories/minimalism/ (secondary)
- Lazonder, A. W. and van der Meij, H. (1993). The minimal manual: is less really more? International Journal of Man-Machine Studies. https://doi.org/10.1006/imms.1993.1081 (abstract via https://research.utwente.nl/en/publications/f811580e-953a-4312-a1ee-9916b3f98e05)
- Kirschner, P. A., Sweller, J. and Clark, R. E. (2006). Why minimal guidance during instruction does not work. Educational Psychologist 41(2). https://doi.org/10.1207/s15326985ep4102_1 (abstract via OpenAlex)
- Kulesza, T., Burnett, M., Wong, W.-K. and Stumpf, S. (2015). Principles of Explanatory Debugging to Personalize Interactive Machine Learning. IUI 2015. https://doi.org/10.1145/2678025.2701399 (abstract via OpenAlex)
- Kulesza, T. et al. (2013). Too much, too little, or just right? Ways explanations impact end users' mental models. VL/HCC 2013. https://doi.org/10.1109/vlhcc.2013.6645235 (abstract via OpenAlex)
- Apple. Human Interface Guidelines: Onboarding. https://developer.apple.com/design/human-interface-guidelines/onboarding (proxy)
- Apple. Human Interface Guidelines: Offering help. https://developer.apple.com/design/human-interface-guidelines/offering-help (proxy)
- Wroblewski, L. Gradual engagement and Twitter's sign-up redesign. LukeW, 2010. https://www.lukew.com/ff/entry.asp?1128
- Lyssna. Five-second testing guide. https://www.lyssna.com/guides/five-second-testing/

Products:

- HEY. How it works. https://www.hey.com/how-it-works/
- HEY. The Screener; The Feed; Paper Trail. https://www.hey.com/features/the-screener/ , https://www.hey.com/features/the-feed/ , https://www.hey.com/features/paper-trail/
- HEY Help. Imbox; The Feed; The Screener; Can we import our old emails? https://help.hey.com/article/759-imbox , https://help.hey.com/article/761-the-feed , https://help.hey.com/article/722-the-screener , https://help.hey.com/article/754-can-we-import-our-old-emails
- Things. Getting Productive guide. https://culturedcode.com/things/guide/ (proxy)
- MacStories. Things 3 review (Wayback snapshot 2022-10-07). https://www.macstories.net/reviews/things-3-beauty-and-delight-in-a-task-manager/
- Linear. Start guide. https://linear.app/docs/start-guide (proxy)
- Linear. Conceptual model. https://linear.app/docs/conceptual-model
- Linear. Inbox; Search; Creating issues. https://linear.app/docs/inbox , https://linear.app/docs/search , https://linear.app/docs/creating-issues (proxy)
- Raycast Manual. Action Panel. https://manual.raycast.com/action-panel (proxy)
- Superhuman Help. Keyboard Shortcuts in Superhuman Mail; Learn Superhuman Mail; Get Started with Superhuman Mail. https://help.superhuman.com/hc/en-us/articles/46005701270541 , https://help.superhuman.com/hc/en-us/articles/46005744375437 , https://help.superhuman.com/hc/en-us/articles/46005793127181 (via the help centre search API)
- Flowjam. Superhuman onboarding teardown. https://www.flowjam.com/blog/superhuman-onboarding-teardown-30-minute-wow-session (proxy, secondary)
- Duolingo. Freeman, C. The Duolingo teaching method, 2 Feb 2023. https://blog.duolingo.com/duolingo-teaching-method/
- Duolingo. Blanco, C. What is implicit learning?, 3 Aug 2023. https://blog.duolingo.com/what-is-implicit-learning/
- Arc Help. Auto Archive: Clean as you go. https://resources.arc.net/hc/en-us/articles/19228855311127 (via the help centre search API)
- Wikipedia. List of built-in iOS apps (Tips). https://en.wikipedia.org/wiki/List_of_built-in_iOS_apps

Anecdotes (Hacker News, via the Algolia API):

- 19807933 (onboarding skipped by half, 2019); 48030227 (skipping tours, 2026)
- 22193775 (Superhuman's 30-minute call, 2020); 41389926 (mandatory call, 2024); 31616236 (Vimcal without a call, 2022)
- 39022851 (Imbox explained at launch, 2024); 23594138 (Imbox and Screener confused, 2020); 23630955 and 27219428 (HEY's boxes fit, 2020, 2021)
- 46133818 (Tips app, 2025)
