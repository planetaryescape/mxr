# How shipping products detect what an email asks of the reader

Research for the mxr unified "what does this email ask of you" classifier. Sources were fetched on 2026-10-22. Each entry gives what the product does, the mechanism where it is published, how it explains itself and takes corrections, its privacy model, and the primary source. "Not published" means I found no primary description; anything after "Inference:" is my reading, not the vendor's statement.

The most useful primary sources are engineering papers rather than product pages. Google's Juicer papers (KDD 2018, CIDR 2020), Microsoft's commitment-detection patent and WSDM 2019 paper, Yahoo's CIKM 2014 sender-classification paper, and Lampert et al.'s email-zoning work describe real mechanisms and real failure modes. Product help pages mostly describe surfaces and controls, not detection.

## 1. Google and Gmail

### Inbox categories are learned per message, corrected by dragging

- What: Primary, Social, Promotions, Updates, Forums. Updates is defined as "automated confirmations, notifications, statements, and reminders that may not need immediate attention".
- Mechanism: not published on the help page. Juicer (below) says it reuses "existing Google infrastructure for email classification used for applications like spam detection and smart folders", so categories are an ML classifier, not rules.
- Corrections: drag a message to another tab (with Undo), star it to pull it into Primary, or write a filter that sets a category. The page says corrections help Gmail "learn your preferences" over time; how is not published.
- Source: https://support.google.com/mail/answer/3055016

### Nudges predict which threads you would normally answer

- What: two settings, "Suggest emails to reply to" (received, unanswered) and "Suggest emails to follow up on" (sent, unanswered). Shown as a line on the resurfaced thread. Requires conversation view.
- Mechanism: the 2018 launch post calls it "AI-powered" and gives nothing else. Secondary sources describe a model predicting which unanswered mail you would normally reply to; the current help page does not explain the decision.
- Corrections: a per-thread "Dismiss nudge" and two global checkboxes. No feedback loop is documented.
- Sources: https://workspaceupdates.googleblog.com/2018/05/gmail-remind-respond.html, https://support.google.com/mail/answer/6585

### Summary cards, Purchases and package tracking read transactional mail

- What: cards for order tracking, events, travel, bills and promotions. The October 2024 launch post lists "Mark as paid" and "Remind me" among card actions; bills can be paid, viewed or turned into a Google Tasks reminder. "Happening soon" at the top of the inbox started with purchases, showing a card two days before estimated delivery. The Purchases view (September 2025) has an "Arriving soon" section sorted by estimated delivery date.
- Mechanism: the help page says only that cards "are generated based on the information from your email". The launch post does not name a model. Package tracking says tracking numbers are found by automated processing "similar to the system Gmail uses for spam and phishing detection", and only the tracking number is shared with participating US carriers. Inference: cards are Juicer-style template extraction plus sender markup.
- Explanation: each card has "Based on [x] email", which opens the source messages.
- Corrections: thumbs-style feedback icons under each card; "request support for another provider" via Gmail feedback. No per-field editing.
- Privacy: requires "Smart features"; off by default in the EEA, UK, Switzerland and Japan. Package tracking is a separate opt-in.
- Sources: https://blog.google/products/gmail/new-gmail-summary-cards/ (2024-10-02), https://support.google.com/mail/answer/15195630, https://support.google.com/mail/answer/16624561, https://support.google.com/mail/answer/13073650, https://blog.google/products/gmail/how-to-opt-in-to-holiday-package-tracking-in-gmail (2024-12-02). Bill card redesign with "Pay bill / View bill / Remind me" is reported in https://9to5google.com/2025/02/14/gmail-bills-summary-card/ (secondary).

### AI Inbox suggests to-dos only from Primary

- What: a Gemini view with "Suggested to-dos" (high-priority items that "need your attention", with the required action in bold, "like a due date for a utility or credit-card bill") and "Topics to catch up on". Announced 2026-01-08; rolling out to paid Workspace from 2026-10-12. US English only, beta.
- Mechanism: Gemini. VIPs are inferred from "people you email frequently, people in your contacts list, and relationships it can infer from message content". Inputs are restricted: only the Primary tab; spam and anything archived, deleted, muted or snoozed are excluded; attachments, delegated and encrypted mail are not supported. First results can take up to seven days.
- Explanation and corrections: each to-do links to its source email with View/Reply actions; "Mark done"; "Remove to-do"; positive/negative feedback; report unsafe suggestion. The help page also says some to-dos "may be generated … but not shown", with a "View all" link to the hidden ones. That is an explicit display threshold over a larger candidate set.
- Privacy: the launch post says only that analysis happens "with the privacy protections you expect". The Workspace privacy hub says content is not human-reviewed or used to train models outside the customer's domain without permission.
- Sources: https://support.google.com/mail/answer/16845247, https://blog.google/products-and-platforms/products/gmail/gmail-is-entering-the-gemini-era/, https://workspaceupdates.googleblog.com/2026/10/gmails-ai-inbox-is-rolling-out-to-paid-Workspace-users.html, https://knowledge.workspace.google.com/admin/generative-ai/generative-ai-in-google-workspace-privacy-hub

### Gemini in Gmail extracts action items on request

- What: on-demand prompts such as "Create a list of action items for me based on this email", plus task creation in Google Tasks.
- Corrections: Good/Bad suggestion buttons with an issue picker. Destructive actions (delete, archive, label, mark read) need confirmation and have a 60-second undo.
- Source: https://support.google.com/mail/answer/14355636

### Email markup gives authenticated senders a structured channel

- What: schema.org JSON-LD or Microdata in the email powers "highlights" (Order, ParcelDelivery, Invoice, FlightReservation, HotelReservation, EventReservation, RestaurantReservation, RentalCar, Bus, Train), actions (one-click ConfirmAction/SaveAction, go-to ViewAction/TrackAction, RSVP) and Calendar "Events from Gmail".
- Invoice fields: `accountId`, `billingPeriod`, `confirmationNumber`, `customer`, `minimumPaymentDue`, `paymentDue` (DateTime), `paymentMethod`, `paymentMethodId`, `paymentStatus`, `provider`, `referencesOrder`, `scheduledPaymentDate`, `totalPaymentDue`. schema.org `PaymentStatusType` has five members: PaymentAutomaticallyApplied, PaymentComplete, PaymentDeclined, PaymentDue, PaymentPastDue. Paid, autopay, declined, due and overdue are separate states. The same split is missing in mxr's "Direct Debit" failure.
- Sender requirements for registration (actions): DKIM or SPF, with the signing or SPF domain's TLD matching the `From:` TLD; a static from address; "hundreds of emails a day minimum to Gmail" for "at least a few weeks"; very low spam complaints; send a real production sample to `schema.whitelisting+sample@gmail.com`. Actions are for "transactional mail with high expected interaction, not promotional bulk mail". Gmail strips markup when a message is forwarded. Date-bound actions expire after the event. A ConfirmAction can be used once.
- Bulk-sender rules (from 2024-02-01, 5,000+ messages/day to Gmail): SPF, DKIM and DMARC (policy may be `none`). Marketing and subscribed mail must carry RFC 8058 `List-Unsubscribe-Post` and RFC 2369 `List-Unsubscribe` headers plus a visible unsubscribe link.
- Inference: a `List-Unsubscribe` header from a large sender is strong evidence the message is marketing or subscribed mail. It is not proof: transactional mail is not forbidden from carrying it.
- Sources: https://developers.google.com/workspace/gmail/markup/overview, https://developers.google.com/workspace/gmail/markup/registering-with-google, https://developers.google.com/workspace/gmail/markup/reference/invoice, https://developers.google.com/workspace/gmail/markup/actions/declaring-actions, https://schema.org/PaymentStatusType, https://support.google.com/a/answer/81126

### Google research: Juicer extracts by template, under k-anonymity

Sheng, Tata, Wendt, Xie, Zhao, Najork, "Anatomy of a Privacy-Safe Large-Scale Information Extraction System Over Email", KDD 2018. PDF: https://storage.googleapis.com/gweb-research2023-media/pubtools/4462.pdf (page: https://research.google/pubs/anatomy-of-a-privacy-safe-large-scale-information-extraction-system-over-email/). Follow-up: Sheng et al., "Migrating a Privacy-Safe Information Extraction System to a Software 2.0 Design", CIDR 2020, https://www.vldb.org/cidrdb/papers/2020/p31-sheng-cidr20.pdf.

Mechanism, as published:

- Scope: business-to-consumer mail only. "Upwards of 90% of messages are machine-generated" and are filled-in templates. Personal mail not generated from a template is excluded from training by construction.
- Template induction, offline, from a 0.5% sample of 90 days of Gmail, by two methods:
  - Sender plus subject regex: fixed subject terms seen in at least k documents and n unique users become literals, and the rest become `(.+)`.
  - MinHash over the XPaths of DOM leaf nodes.
  - Clusters below the anonymity thresholds are discarded.
  - Templates are keyed by sender, and online lookup takes the first hash hit.
- Classification: one binary "vertical" classifier per vertical (bills, offers, hotels…), not a multi-class model, so each vertical can set its own precision/recall trade-off and be developed independently.
  - Scores are produced per email and aggregated per template by majority, which de-noises the label.
  - Features include subject, sender tokens, top 150 body words, bold/header text, alt text, footer, tag counts, date-time count and salient entities.
  - Tokens that fail k-anonymity are replaced with `REDACTED`.
- Field extraction: a high-recall candidate generator (dates, currency amounts, addresses), then a binary field classifier over features such as 5/10/20 words before and after the span, span text and document position. "Precision is provided by the classifier."
  - Scores are averaged per XPath across a template's emails, and an XPath becomes a rule above minimum support and precision.
  - The CIDR paper adds validation conditions, such as check-out after check-in.
- All-or-nothing extraction: "If any of the required fields for a vertical fail to extract, we discard the other field values and generate nothing."
- Bills specifically:
  - Two classifiers: is-bill, and a bill subtype classifier with classes "payment confirmation, autopay notification, reminder, and overdue". Only reminder and overdue are surfaced.
  - Training data was ~1M positives against ~200M negatives.
  - Precision on unseen templates was 66.8% (structural) and 76.2% (sender-subject), against 84.6% and 94.1% on templates with existing parsers.
  - Named false positive: "a statement of an account balance" read as a bill reminder, because the field types (a date and an amount) and the language match.
  - The amount-due field was the bottleneck: rules existed for 81.8% of bill templates, against 93.5% for due date.
- Offers: emails already tagged Promotions were excluded from the negative set, because many unlabelled promotions really are offers. Holdout precision was 0.993; on new templates it fell to 0.85.
- Hotels: the named hard case is promotional hotel mail with a hotel name, an address and check-in/check-out-looking dates ("Special promotional rates at the Grand Hotel … Check in on Friday the 12th"). The authors say distinguishing it "require[s] semantic understanding". This is the same failure as mxr's marketing "Pay now" and event announcements read as RSVPs.
- Training data came from sender microdata (high precision, very low recall), hand-written per-sender parsers from donated mail, and generic regex parsers (lower precision, higher recall). The CIDR migration used the old rules' output as training labels, then deleted over 45k lines of rule code. Precision and recall exceeded the rules, and the ML version extended to non-English languages.
- Evaluation without reading mail had three routes:
  - Donated emails, which are small and unrepresentative.
  - An opt-in rater programme where users rate extractions from their own mail (a few thousand people).
  - Synthetic emails built from each template's fixed text, with variable text replaced by placeholder values. Reviewers answer "Is this a hotel reservation confirmation?" and "Is this highlight the check-in date?"
- Developer workflow: bottleneck analysis by template coverage, then vertical coverage, then rule coverage, then extraction coverage, to show where recall is lost.

### Google takeaways

The bill problem is decomposed into is-bill, status subtype and required fields, with a separate precision bar per vertical. Promotional look-alikes are a named, persistent false-positive class even at Google scale.

## 2. Microsoft

### Cortana commitments and the extraction patent

- What: from 2016–2017, Cortana reminded Windows users of promises made in their own sent email ("I'll send you the report…"). Built with Microsoft Research.
- Mechanism (patent US10361981B2, "Automatic extraction of commitments and requests from communications and content", Microsoft, priority 2015-05-15):
  - Statistical classifiers label sentences or messages as containing a request or commitment and extract the best descriptive span.
  - Extraction is attributed to a specific author in multi-party threads.
  - Four directional cases are distinguished: the user's outgoing commitments, the user's outgoing requests, the other party's incoming commitments and the other party's incoming requests. Third-person commitments ("the user commits someone else") are also covered.
  - Features: text, sender and recipients, recipient count, length, subject, send time and day, sentence position, and thread history (earlier requests or commitments between the same people).
  - Low-confidence inferences may prompt the user to confirm. High-confidence ones need not.
  - Explicit and implicit correct/incorrect feedback is recorded to improve models.
  - Calendars and other personal data are consulted only after opt-in.
- Source: https://patents.google.com/patent/US10361981. Related: US10999230B2 (surfacing flagged request/commitment mail sorted by due date), https://patents.google.com/patent/US10999230.

### Microsoft research on commitments is precise about whose action it is

- Azarbonyad, Sim, White, "Domain Adaptation for Commitment Detection in Email", WSDM 2019, https://www.microsoft.com/en-us/research/uploads/prod/2019/01/AzarbonyadWSDM2019.pdf.
  - Definition: "any sentence in an email where the sender is promising to do an action which can potentially be added to his/her TODO list or be worthy of a reminder".
  - Annotator question: "Does the highlighted sentence contain a specific action that the sender must complete or is obliged to do? (The action must be on the sender and must not already be complete)."
  - Data: 65,398 Enron sentences (3,337 positive) and 13,021 Avocado sentences (4,484 positive). Krippendorff's α 0.73.
  - Results: an in-domain logistic regression on word n-grams reaches precision 0.82 / recall 0.81 / F1 0.81 on Avocado and 0.80 / 0.77 / 0.78 on Enron. Cross-domain precision drops, for example 0.82 to 0.74 going Avocado to Enron (recall 0.81 to 0.78). Adversarial domain adaptation recovers part of the loss.
  - Lesson: models trained on one corpus degrade on another user's mail.
- Mukherjee et al., "Smart To-Do: Automatic Generation of To-Do Items from Emails", ACL 2020, https://arxiv.org/abs/2005.06282. Two stages: detect sentences where the sender commits, then generate a to-do with seq2seq (BLEU 0.23, ROUGE 0.63).
- MSR blog summary: https://www.microsoft.com/en-us/research/blog/email-overload-using-machine-learning-to-manage-messages-commitments/

### Viva Insights / Briefing email separated four item types, then retired them

- What: the Briefing email listed:
  - "Commitment — Something you promised or committed to do for someone else in an email"
  - "Request — Something another person asked you to do in an email"
  - "Follow-up — Something you asked for from someone else in an email"
  - To Do tasks
- Corrections: "Done", or "Remind me", which adds the item to To Do and re-surfaces it in the next working day's briefing, with options for a custom date, next week or delete.
- Status: Briefing emails were paused from 2023-01-15. Message Center MC713042 (2024-02-06) said suggested tasks "will no longer include commitments and requests based on email and chat messages", retiring "View outstanding tasks" and "Follow up on your requests" in the Outlook add-in from late February 2024. Microsoft pointed users at Copilot. A Tech Community thread reports users who relied on it saying Copilot was not a substitute. No public reason beyond the notice.
- Privacy: not re-verified, because the Learn pages now redirect. Inference: as personal insights, the data was visible only to the user.
- Sources: archived https://learn.microsoft.com/en-us/viva/insights/personal/briefing/be-tasks (2023-03-16 snapshot via web.archive.org), https://mc.merill.net/message/MC713042

### Outlook flags feed To Do; Copilot prioritises by user-written rules

- To Do "Flagged email": a manual flag becomes a task. Only messages flagged in the last 30 days sync, with a maximum of 100. No detection involved. https://support.microsoft.com/en-us/todo/using-microsoft-to-do-with-flagged-email-from-outlook
- Copilot "Prioritize my inbox": assigns high, normal or low using "people on the thread, their job titles, email content and more".
  - The user must write at least one natural-language criterion ("It's from my manager"). Microsoft advises "it's from…" or "contains…" phrasing.
  - High-priority mail gets an up arrow, the preview line is replaced by a summary, and the reading pane shows a few lines on why it is important.
  - Correction is only by editing criteria. It applies to future mail only, with no re-evaluation of existing messages and no per-message override.
  - Explicitly not evaluated: mail the sender marked low importance, out-of-office replies, meeting mail, mail to yourself, very short emails, NDRs, quarantine and unreadable encrypted mail.
  - Source: https://support.microsoft.com/en-US/Outlook/copilot-outlook/prioritize-my-inbox

## 3. Apple

### Mail categories run on-device

- What (iOS 18.2+, macOS, iCloud.com): Primary ("personal messages, direct communications, and time-sensitive information"), Transactions ("confirmations, receipts, and shipping notices"), Updates ("news, newsletters, and social updates") and Promotions. Time-sensitive messages from the other three also appear in Primary. Non-Primary categories open as a per-sender digest.
- Mechanism: not published. The privacy page says in the iOS Mail app "categorization processing occurs entirely on-device". On iCloud.com Apple processes "certain metadata, such as senders and email header information … This metadata does not include the body content of your emails."
- Corrections: Categorize Sender, which applies to all current and future mail from that sender; drag individual messages on a computer; "Don't Show In Primary" for a time-sensitive item; "Reset Manual Categorization"; turn categorisation off.
- Inference: the per-sender correction and the digest view suggest classification is largely sender-level, like Yahoo's design below.
- Sources: https://www.apple.com/legal/privacy/data/en/mail-categorization/, https://support.apple.com/guide/icloud/use-categories-mmafbebd3108/icloud

### Priority Messages and summaries do not publish their decision rule

- What: Priority Messages, a section at the top of the inbox for "your most urgent emails, like a same-day dinner invitation or boarding pass". One-line summaries replace previews on unread mail. On-demand summaries cover whole threads.
- Mechanism: not published for Mail. Apple states that Priority Notifications are decided "by an on-device model". It does not say the same for Mail's Priority Messages.
- Private Cloud Compute: Apple Intelligence runs on device where possible. Larger requests go to PCC, which sends only task-relevant data. Data "is not stored or made accessible to Apple". Users can export an "Apple Intelligence & PCC Report" from Settings > Privacy & Security.
- Corrections: none documented beyond turning summaries off. The help page says to check important information for accuracy.
- Wallet order tracking (iOS 26) uses Apple Intelligence to "identify and summarize order tracking details from emails sent from merchants or delivery carriers".
- iOS 27 adds suggested actions for flights, reservations and shipments. This is reported by 9to5Mac and MacRumors; I did not find an Apple primary source.
- Sources: https://support.apple.com/guide/mac-help/use-apple-intelligence-in-mail-mchlb2dbea8f/mac, https://www.apple.com/legal/privacy/data/en/intelligence-engine/, https://www.apple.com/newsroom/2025/06/apple-intelligence-gets-even-more-powerful-with-new-capabilities-across-apple-devices/

## 4. Yahoo Mail

### Yahoo research classifies mostly by sender

- Grbovic, Halawi, Karnin, Maarek, "How Many Folders Do You Really Need? Classifying Email into a Handful of Categories", CIKM 2014, https://arxiv.org/abs/1606.09296.
  - Categories: Human, plus machine-generated Shopping, Financial, Travel, Career and Social, with Other as a fallback.
  - Online cascade: (1) light message rules, where replies and forwards are labelled human and top senders are hard-coded (~32% of traffic); (2) a lookup table of senders classified offline, keeping high-confidence senders only; (3) a heavier message-level classifier for unknown senders (~8%).
  - Offline sender classifiers are one-vs-all logistic regression over content, address features (subdomain, "no-reply", commercial keywords) and behavioural features (reply, forward, read and delete rates, send bursts).
  - Labels came from editors, heuristics, folder majority votes and LDA over users' folders.
  - Precision and recall are "close to 90%", with average AUC 0.973.
  - Quote: "The majority of the email traffic should be classified by sender for performance optimization."
- Di Castro, Lewin-Eytan, Maarek, Wolff, Zohar, "Enforcing k-anonymity in Web Mail Auditing", WSDM 2016, https://labs.yahoo.com/mobstor/publication_attachments/WSDM16_final___Mail_monitoring_while_preserving_k_anonymity.pdf. "Mail-Hash" groups structurally identical machine-generated mail so auditors see only k-anonymous masked samples. This is the Yahoo counterpart to Juicer's synthetic review. Juicer notes Yahoo's extraction system made tens of thousands of clusters per sender domain, against Juicer's tens, and attributes the difference to template fragmentation.

### Yahoo products surface bills and codes; mechanism not published

- What:
  - The current app shows one-line summaries and quick actions: "open bills, grab verification codes, and track packages".
  - iOS summaries include "billing details" and offer "set reminders for bill payments".
  - Planner (announced 2026-03-23, US) pulls "school events, appointments, bill payment deadlines" into a calendar and to-do list. It also carries brand-sponsored events (H&R Block at launch).
  - The older Reminders feature suggested emails and reminder times "based on email content".
- Mechanism: not published ("Yahoo Scout Intelligence Platform"). No correction or privacy detail on the pages read. Sponsored items in the same to-do surface is a trust cost mxr should not copy.
- Sources: https://www.yahooinc.com/press/yahoo-mail-introduces-planner-an-ai-powered-personal-productivity-hub, https://www.yahooinc.com/press/yahoo-mail-launches-a-new-app-experience-with-mobile-first-ai-features, https://help.yahoo.com/kb/SLN36900.html

## 5. Indie and newer clients

| Product | Needs-action detection | LLM? Where | How users correct | Accuracy claims | Source |
|---|---|---|---|---|---|
| Superhuman | Built-in Auto Labels (Marketing, News, Pitch, Social go to "Other"). The Auto Label Library includes "Messages that need your response", "Invoices, bills, and payment notices" and meeting scheduling. Custom labels combine deterministic criteria (From/To/Subject) with an AI prompt using AND/OR plus exclusions. Labels apply to new mail and the last 14 days. Auto Reminders: "Superhuman AI detects when a follow-up is needed based on your latest outgoing message if no reply has been received." | Cloud LLM; model not named. Owned by Grammarly; AWS us-east. Users can opt out of training (Product Improvement and Training control, on by default for individuals). | Live preview while writing a label, with ✔/✖ on each sample. Move to Important/Other scoped to this conversation, this address or this domain. Important/Other on Gmail depends on Gmail smart features. | None published | https://help.superhuman.com/hc/en-us/articles/46005657758861-Auto-Labels, .../46005680129933-Auto-Label-Library, .../46005807905421-Reminders-on-Autopilot, .../46005707869965-Moving-Conversations-Between-Important-and-Other, .../46242180015501-Product-Improvement-and-Training-Control |
| Shortwave | AI filters: natural language, search query or both. Actions: label, archive, mark important, star, update todos, delete. Pre-built filters include "Needs Action", "FYI", "Cold Outreach", "Finance" and "Purchases". Importance comes from Gmail's Importance model. Todos are manual groupings of threads. | Cloud. "The vast majority of our AI workloads use open source models that run on hardware we control." Sub-processors: Google Cloud, OpenAI, Anthropic, Pinecone. No third-party training. | "Reapply filters" on selected emails shows "exactly what actions the AI takes and why". Per-sender auto-apply/always-remove rules with "Apply changes to future messages". | None | https://www.shortwave.com/docs/guides/customize-your-shortwave-settings/, https://www.shortwave.com/docs/how-tos/manage-auto-apply-labels/, https://www.shortwave.com/docs/guides/security/ |
| HEY | No detection. The user screens each new sender once (Screener) and routes it to Imbox, The Feed (newsletters) or Paper Trail (receipts, transactional). Reply Later is a manual pile. | No | Per-sender routing decision | n/a | https://www.hey.com/features/the-screener/, /paper-trail/, /the-feed/, /reply-later/ |
| SaneBox | Moves unimportant mail to @SaneLater using headers only ("sender, subject, etc."). Uses sent-folder relationships and open/reply behaviour. Server-side IMAP folders. | Not LLM. Privacy policy says it processes headers. | Moving a message between folders is the training signal. Daily digest of filtered mail. | None | https://www.sanebox.com/help/155-how-does-sanebox-work, https://www.sanebox.com/privacy |
| Spark (Readdle) | Smart Inbox splits People, Notifications and Newsletters. Priority is user-marked senders. Gatekeeper screens new senders. +AI Auto-Drafts and Inbox Automation. | Mixed. AI uses on-device models "and/or" Azure OpenAI, OpenAI, Anthropic and Vertex. "We do not use your data to train any AI model." Providers keep inputs up to 30 days for abuse monitoring. | Per-sender accept/block; mark Priority | None | https://sparkmailapp.com/features/smart_inbox, https://sparkmailapp.com/help/manage-your-inbox/customize-your-inbox, https://sparkmailapp.com/privacy (2026-09-10) |
| Notion Mail | AI auto-labels from a prompt; you pick relevant samples; new mail only. Shut down on 2026-09-22. | Cloud | Sample selection at setup | None | https://www.notion.com/help/notion-mail-inbox-is-going-away-what-to-do-next |
| Fyxer | One fixed category per email: To respond/To do ("need your reply, plus urgent emails that don't need your reply, but may require another action"), FYI, To follow up/Awaiting reply, Notification, Comment, Meeting update, Marketing, plus optional topic sub-labels (orders, newsletters…). Categories change with state: once you reply, To respond becomes To follow up after a configurable wait. Claims to be built on "500,000+ hours of real executive assistant data". | Cloud; never trains third-party models | Categories cannot be renamed or added ("intentional"). Users toggle which categories stay in the inbox. Changes apply going forward. No per-email correction documented. | None | https://support.fyxer.com/article/fyxer-email-categorization-handbook, https://www.fyxer.com/blog/how-fyxer-organizes-your-inbox-by-priority, https://fyxer.com/security |
| Missive | Rules with an AI Prompt condition. Actions: AI label, AI note, AI draft, "Add tasks with AI" (extracts action items). Advice: "use standard conditions to filter first, then apply AI only to relevant messages". | User's own OpenAI, Anthropic or Gemini key, or Missive credits. "Content is sent to your provider only during rule processing." Fast (synchronous) models for conditions, slower background models for actions. | "Log prompt result in conversations" writes the model's raw answer into the thread for debugging | None | https://missiveapp.com/docs/advanced-features/rules/ai-rules |
| Front | Legacy AI Tagging read only subject and body ("cannot look at the sender's email address"), required 10 approved examples per tag, and allowed up to 50 tags per inbox. Replaced by Topics (auto-discovered, best above 1,000 conversations/month) and "Branch by Autopilot" (tag by the AI's answer to a question). | Cloud | "Report inaccurate tag" removes the tag and records feedback. No negative examples could be given. | None | https://help.front.com/en/articles/759488 |

No indie product publishes accuracy. The common design is a fixed small taxonomy with "needs reply" at the top, LLM judgement per message, per-sender overrides, and a preview or debug view of why a rule fired. Missive is the only one that runs on the user's own model key, and the only one that shows the model's raw output in context.

## 6. Bills and fintech

### Bill trackers mostly read bank data, not email

- Rocket Money "automatically detects subscriptions by analyzing transactions from your connected bank accounts and credit cards". Items with no payment in the past month move to Inactive. Users add missed bills manually and attach matching transactions with "Find Transactions". https://help.rocketmoney.com/en/articles/2185531-managing-your-bills-and-subscriptions, https://help.rocketmoney.com/en/articles/934383-missing-subscriptions
- Emma detects "direct debits, standing orders, and frequent card purchases" from linked accounts. Manual add links all transactions from that merchant. Duplicates appear when the payee name or account changes. https://help.emma-app.com/en/article/how-can-i-check-my-recurring-subscriptions-hxlh8
- Monzo lists Direct Debits, standing orders and recurring card payments with amount, frequency and predicted next date. Users toggle "Repeating payment" on a card transaction to teach it. Failed morning Direct Debits are retried at 14:00. These are 2018 sources and may be stale. https://monzo.com/blog/2018/08/15/manage-your-bills-with-monzo
- Mint (historical) grew bill tracking from PageOnce/Check, which logged into biller accounts with user credentials. Bill pay ended 2018-06-30 and Mint closed in March 2024. https://en.wikipedia.org/wiki/Intuit_Mint (secondary)
- doxo connects to over 120,000 billers for bill pay and a due-date calendar. Due dates come from biller connections or the user. No email scanning documented. https://www.doxo.com/about/support/

Inference: these products avoid parsing email because bank and biller data say what was actually paid. Email says what a sender claims or proposes. The mxr equivalent is to treat "paid" as a claim that needs a payment-confirmation subtype, never something inferred from a sentence that suggests setting up payment.

### Document parsers make document type a first-class field

- Google Document AI Invoice Parser fields include `invoice_id`, `invoice_date`, `due_date`, `payment_terms`, `currency`, `net_amount`, `total_amount`, `total_tax_amount`, `amount_paid_since_last_invoice`, supplier and receiver blocks, `line_item/*` and `vat/*`. It has no credit-note field. The Expense Parser covers receipts (`receipt_date`, `purchase_time`, `payment_type`, `credit_card_last_four_digits`, `tip_amount`). A separate Custom Classifier and Splitter assign document types. https://docs.cloud.google.com/document-ai/docs/processors-list
- Mindee (V2) `document_type` takes one value from `invoice`, `payslip`, `quote`, `purchase_order`, `statement`, `receipt`, `credit_note` or `other_financial`. Other fields include `due_date`, `payment_date` ("the date on which the payment is due / was full-filled"), `total_amount`, `total_net` and `total_tax`. https://docs.mindee.com/use-cases/extraction-models/financial-documents.md, https://docs.mindee.com/use-cases/extraction-models/invoice.md
- Veryfi `document_type` takes one value from `other`, `receipt`, `invoice`, `statement`, `purchase_order`, `check`, `w9`, `packing_slip`, `contract`, `w8` or `remittance_advice` (nullable). Related fields:
  - `is_money_in`: "true if the document has a refund or credit note".
  - `balance`: "If the invoice is paid, the final balance is 0, but if the invoice is not paid, the value is equal to the total."
  - `is_document`, which asks whether this is a supported document at all.
  - `is_duplicate`.
  - Per-field confidence `score`.
  - A separate Classification API.
  - https://docs.veryfi.com/api/receipts-invoices/process-a-document/

### Receipt, invoice, statement, credit note and reminder carry different obligations

| Document | Money direction | Creates an obligation? | Key discriminating fields |
|---|---|---|---|
| Receipt / payment confirmation | User has already paid | No | payment date in the past, payment method, `PaymentComplete` |
| Invoice / bill | User owes | Yes, if balance > 0 and not autopay | due date, amount due/balance, `PaymentDue` |
| Autopay notice | Will be collected automatically | Usually no action; maybe check funds | `PaymentAutomaticallyApplied`, scheduled payment date |
| Statement (account balance) | Informational, may show minimum due | Only if a minimum payment due and due date are present | statement period, balance; Juicer's named false positive |
| Credit note / refund | Money to user | No | `credit_note`, `is_money_in`, negative or zero total |
| Overdue / failed payment | User owes now | Yes, urgent | `PaymentPastDue`, `PaymentDeclined` |
| Quote / estimate | Proposal | No (or a decision) | `quote` type, validity date |

## 7. How the research maps to mxr's observed failures

| mxr failure | What the industry does |
|---|---|
| Marketing "Pay now" read as a bill | Gate on mail class before extraction. Juicer bills run on B2C templates, and offers training excluded Promotions-tagged mail from negatives. AI Inbox reads Primary only. Juicer names promotional look-alikes as a persistent false-positive class. Gmail's bulk-sender rules make `List-Unsubscribe` a strong (not conclusive) marketing signal. |
| "Why not set up a Direct Debit?" read as paid | Status is a separate classifier (Juicer: payment confirmation / autopay / reminder / overdue), with schema.org's five-value `PaymentStatusType`. A suggestion or question is none of these states. |
| Credit notes and £0.00 read as bills | Document type is an explicit field with `credit_note` (Mindee) or `is_money_in` (Veryfi). Veryfi's `balance` is 0 when paid. Juicer emits nothing when a required field such as amount due fails. Validation rules belong beside extraction (CIDR: check-out after check-in). |
| Event-platform announcements read as RSVPs | Juicer's hotel case: promotional mail with dates and venue needs semantic separation. Gmail highlights an `EventReservation` (a booking the user holds) only from registered, authenticated senders. Invitation, reservation and announcement are distinct. |
| User's requests to others read as user's promises | Microsoft separates Commitment (you promised), Request (asked of you) and Follow-up (you asked someone). The patent tracks four directions plus third-person commitments. The WSDM annotation rule is that the action must be on the sender and not already complete. |
| Forwarded text read as the user's | Lampert et al. zone mail into author text, greeting, sign-off, quoted reply, forwarded content, signature, advertising, disclaimer and attachment references. Detecting requests only in author, greeting and sign-off zones raised accuracy from 72.28% to 83.76%, a 41% error reduction. The patent attributes each extraction to its author. Gmail strips markup on forward. |
| Requests from people and organisations missed | Recall is measured as a funnel (Juicer bottleneck analysis). AI Inbox weights VIPs by contact frequency, contacts and inferred relationships. Fyxer's "To do" covers urgent mail that needs action but no reply. |

Lampert, Dale, Paris, "Detecting Emails Containing Requests for Action", NAACL 2010, https://aclanthology.org/N10-1142.pdf. Zone classifier: "Segmenting Email Message Text into Zones", EMNLP 2009, https://aclanthology.org/D09-1096/ (Zebra, nine zones, 87.01% line accuracy). Request annotation agreement was κ 0.681, so requests are hard for humans too.

## Patterns worth copying

1. Gate, then classify, then extract. Decide the mail class first: person-to-person versus machine-generated, and marketing versus transactional. Then run one binary classifier per obligation kind, each with its own threshold, then field extraction with required fields. If a required field fails, emit nothing (Juicer). This replaces mxr's "first regex wins, negatives veto the whole email".
2. Model status separately from kind. Bills need paid, autopay, due, overdue and declined (schema.org `PaymentStatusType`; Juicer subtypes). Financial documents need a document type that includes credit note, quote and statement (Mindee, Veryfi).
3. Model direction and author explicitly. Track who owes the action: me to them, them to me, me asking them, or a third party. Take text only from the current author's zone. Strip quoted and forwarded zones before detection and keep them as context only (Microsoft patent, Viva's three item types, Lampert zoning).
4. Classify mostly by sender, with message-level fallbacks. Yahoo routes most traffic through a sender table and uses a message classifier only for unknown senders. Apple and Superhuman corrections are per sender. Template or sender aggregation de-noises labels (Juicer majority vote).
5. Use sender-supplied structure when it is authenticated. Read schema.org markup only when DKIM or SPF aligns with `From:`, and treat markup as high precision, low recall (Juicer training source; Gmail registration rules). Treat `List-Unsubscribe` as a marketing prior.
6. Keep a candidate set larger than the displayed set. Gmail AI Inbox generates more to-dos than it shows and exposes "View all". The display threshold is a product choice, separate from detection.
7. Show the source and the reason, every time. Use "Based on N emails" (Gmail cards), a reason line (Copilot), "Reapply filters" showing what the AI did and why (Shortwave), and the raw model answer logged in the thread (Missive).
8. Make corrections cheap and scoped. Offer this message, this sender or this domain (Superhuman); Done, Remind me or Delete (Viva); Remove to-do plus feedback (AI Inbox). Let low-confidence items ask for confirmation while high-confidence ones do not (Microsoft patent).
9. Evaluate without reading private mail, and measure recall as a funnel. Use synthetic emails built from template boilerplate (Juicer) and opt-in self-rating of one's own extractions. Report coverage stage by stage: template, vertical, rule, extraction. For mxr, the local equivalent is a user-owned labelled set plus per-stage counts.
10. Run cheap filters before the LLM. Missive's advice is to use deterministic conditions first and call the AI only on what remains. Shortwave runs most AI on self-hosted open models. Both fit a local-first default.

## Patterns to avoid

1. Treating any date-plus-amount email as a bill. Google's own bill classifier reached only 66.8–76.2% precision on unseen templates, with account statements as the named false positive.
2. Inferring that something is paid from language about payment. Bank-data products infer recurrence from actual transactions. Email gives claims, so only a payment-confirmation subtype should set "paid".
3. One global classifier trained on someone else's mail. Commitment precision fell from 0.82 to 0.74 across corpora (WSDM 2019), and Juicer's offer precision fell from 0.993 to 0.85 on new templates. Local per-user signals and corrections matter.
4. Corrections that only affect the future and cannot be applied to one message. Copilot Prioritize offers no per-message override. Fyxer and Notion Mail apply changes going forward only.
5. Fixed taxonomies with no per-item correction, or prompt-only labels with no preview. Fyxer cannot rename categories, and Front's legacy AI Tagging could not take negative examples.
6. Mixing sponsored content into obligation lists. Yahoo Planner shows sponsored events alongside the user's to-dos.
7. Shipping commitment and request detection without a reliability story. Microsoft retired email-based commitments and requests in 2024 without public data. Users who relied on it complained. Hidden quality problems or cost are possible reasons, but none is published.
8. Silent background processing with no audit trail. Contrast Apple's PCC report and Missive's prompt-result log.

## Unanswered

1. Gmail summary cards and AI Inbox: how much is Juicer-style template extraction and how much is Gemini? Neither the launch post nor the help pages say.
2. Apple Mail categories and Priority Messages: is classification sender-level or message-level, and is Priority Messages decided on device? Apple states "on-device" only for categorisation in the iOS app and for Priority Notifications.
3. Why Microsoft retired email commitments and requests in Viva Insights. MC713042 gives no reason, and the old Learn privacy pages now redirect.
4. Precision figures for any LLM-based "needs reply" or "to-do" feature (Gmail AI Inbox, Superhuman, Shortwave, Fyxer). None is published.
5. Whether Gmail Nudges use content or only behavioural signals. Google says only "AI-powered".
6. Yahoo Mail's current bill and Planner extraction mechanism and its correction flow. Press and help pages are silent.
7. Gmail markup: which highlight types (as opposed to actions) are shown for unregistered senders. The overview page does not say.
8. How Veryfi and Mindee decide `credit_note` versus `invoice` when a document is ambiguous. They expose only the output field and a confidence score.
