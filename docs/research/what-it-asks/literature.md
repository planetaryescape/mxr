# What the research says about detecting what an email asks of its reader

Retrieved 2026-10-22. Each source is tagged with how it was read:

- **[full]**: PDF text read.
- **[partial]**: specific sections read.
- **[abstract]**: only the abstract was reachable. ACM DL returned 403 and no author copy was found, so the abstract came from OpenAlex or the publisher page.
- **[secondary]**: known only through another paper's description.

Numbers are quoted from the paper named. Anything I inferred is marked *(inference)*.

The literature splits along the same line mxr's failures do. One body of work treats person-written mail as speech acts: requests (directives) and commitments (commissives), mostly on Enron and Avocado enterprise mail. The other treats business-to-consumer (B2C) mail as instances of sender templates, with a document type and typed fields, at Yahoo and Google scale. No paper treats both in one classifier. The strongest results in both come from deciding *what kind of mail this is* and *whose words these are* before deciding what it asks.

---

## 1. Speech acts in person-written email

### Searle's classes are the theoretical base; the email taxonomies are pragmatic variants of them

- **Searle, J. R. (1976). A classification of illocutionary acts. *Language in Society* 5(1):1–23.** https://doi.org/10.1017/S0047404500006837. [citation verified; the content is standard and not re-read]
  - Five classes: assertives, directives, commissives, expressives and declarations. A directive tries to get the hearer to act; a commissive commits the speaker to a future act.
  - **Lesson for mxr.** "Asks of you" is a directive aimed at the account owner. "Promised by you" is a commissive whose speaker is the owner. The two have opposite directions. The direction depends on who wrote the text and who reads it, not on the surface words. A bill reminder is usually an assertive ("your balance is due"), and the obligation comes from the institutional context, not from directive grammar *(inference)*. This explains why a directive-cue regex both over-fires on marketing imperatives and misses statement-form obligations.

### Cohen, Carvalho and Mitchell (2004): sender-intent acts are learnable at high precision and moderate recall; bigrams like "will you" and "I will" carry the signal

- **Cohen, W. W., Carvalho, V. R., Mitchell, T. M. (2004). Learning to classify email into "speech acts". EMNLP 2004, pp. 309–316.** https://aclanthology.org/W04-3240/. [full]
- **What they did.** They built a verb–noun ontology. The verbs are Request, Propose, Amend, Commit, Deliver, Refuse, Greet and Remind. The nouns are information (data, opinion) and activity (meeting, task, and so on).
  - A message can carry several acts. The CSpace corpora averaged about 1.3 verbs per message.
  - The data was 1,357 messages: three CSpace MBA-team corpora (351, 341 and 443 messages) plus PW CALO (222).
  - Quoted material, attachments and non-subject headers were removed by hand before classification.
- **Agreement.** κ on N03F2: Request 0.81, Commit 0.72, Propose 0.72, Deliver 0.75, Amend 0.83, Meeting 0.82. κ is higher on single-act messages (Request 0.97, Commit 0.78).
- **Results.** Binary classifier per act, 5-fold cross-validation. The table gives positive/negative message counts and the best F1 per act:

  | Act | Positive/negative | Best F1 |
  |---|---|---|
  | Request | 450/907 | 0.69 (DT) |
  | Propose | 140/1,217 | 0.44 (SVM) |
  | Commit | 208/1,149 | 0.47 (SVM) |
  | Deliver | 873/484 | 0.80 (VP) |
  | Directive | 605/752 | 0.78 |
  | Commissive | 993/364 | 0.85 |
  | Meeting | 345/1,012 | 0.72 |

  - The most discriminative features were common words, so TF-IDF hurt. Bigrams helped because "will you" and "I will" separate request from commit while the single words do not.
  - Classifiers trained on one team's mail transferred to another team without a clear loss (Table 5).
- **Open problems they named.** Context, meaning the preceding message in a thread. Thread trees, which arise from simultaneous replies. Sentence- or segment-level labelling, since disagreement rises with message length.
- **Lessons for mxr.**
  - Rare acts are the hard ones. Commit F1 was 0.47 at 15% prevalence.
  - The direction cue is pronoun plus modal ("you"+modal is a request; "I"+"will" is a commitment), which only means something once you know who wrote the text.
  - Their Figure 1 already frames the assistant as tracking *outgoing* requests, which await an answer from others, separately from *incoming* commitments. That is the distinction mxr's LLM lost when it read "could you book an engineer?" as the owner's promise.

### Carvalho and Cohen (2005, 2006): thread context helps some acts but not all; n-grams up to trigram cut error by about a quarter

- **Carvalho, V. R., Cohen, W. W. (2005). On the collective classification of email "speech acts". SIGIR 2005, pp. 345–352.** https://doi.org/10.1145/1076034.1076094. [abstract]
  - A dependency-network collective classifier with maximum-entropy local models over words and relational (thread-neighbour) features. It gave statistically significant gains over bag-of-words "for some, but not all, email-act classes."
- **Carvalho, V. R., Cohen, W. W. (2006). Improving "email speech acts" analysis via n-gram selection. HLT-NAACL ACTS Workshop, pp. 35–41.** https://aclanthology.org/W06-3406/. [full]
  - 1,716 CSpace messages.
  - Preprocessing removed quoted text, signatures and *forwarded messages*. It also normalised numbers, times, days, pronouns ("I/we/you…" became `[person]`) and file types. For Commit only, first-person pronouns were kept out of `[person]`, because "I"-n-grams carry the act.
  - 1–3-grams cut error by 26.4% on average against Cohen et al. 2004: Request −24.7%, Commit −33.3%, Propose −38.3%, Deliver −23.7%, Meeting −9.2%, dData −29.1%. Adding 4–5-grams did not help with this little data.
  - The top request n-grams were "do [person] need to", "let [me] know", "[wwhh] do [person] think ?", "could [person] please".
- **Lesson for mxr.** Thread context is a second-order gain; the first-order gains come from cleaning the text down to what the author wrote and from multi-word cues. Their normaliser also shows that pronoun identity is a feature you must keep, not collapse, for the commissive class.

### Lampert, Dale and Paris (2007–2010): careful guidelines raise commitment agreement from 0.54 to 0.74; requests sit in specific zones; zoning cuts request-classifier error by 41%

- **Lampert, A., Paris, C., Dale, R. (2007). Can requests-for-action and commitments-to-act be reliably identified in email messages? ADCS 2007, pp. 48–55.** https://researchers.mq.edu.au/en/publications/can-requests-for-action-and-commitments-to-act-be-reliably-identi/. [secondary, via ALTA 2008 Table 1]
- **Lampert, A., Dale, R., Paris, C. (2008a). The nature of requests and commitments in email messages. AAAI EMAIL-08 Workshop, pp. 42–47 (AAAI Tech. Report WS-08-04).** [secondary, definitions restated in 2008b and 2010]
- **Lampert, A., Dale, R., Paris, C. (2008b). Requests and commitments in email are more complex than you think: Eight reasons to be cautious. ALTA 2008, pp. 55–63.** https://aclanthology.org/U08-1009/. [full]

  **Definitions.**
  - A *request* is an utterance from the sender that obligates a recipient to schedule an action, perform an action, or respond with a speech act. Questions count.
  - A *commitment* is an offer or promise of future action by a specified agent. The agent is often, but not always, the sender, so third-party commitments count.
  - Both may be *conditional*. The diagnostic: would the action appear on that agent's task list or calendar?

  **Agreement (Cohen's κ, three annotators, random Enron samples).**

  | Experiment | Unit | Request κ | Commit κ |
  |---|---|---|---|
  | Sentences 1: 54 messages, 310 sentences, ADCS 2007 guidelines | sentence | 0.78 | 0.54 |
  | Sentences 2: 350 messages, 750 sentences, explicit guidance on conditional and indirect cases | sentence | 0.80 | 0.74 |
  | Messages: 100 messages | message | 0.84 | 0.78 |

  **The eight hard categories and their annotation rulings.**
  1. *Locus ambiguity.* The same request is stated indirectly and then directly. Mark every realisation. This is why message-level agreement beats sentence-level.
  2. *Meetings.* Announcements are implicit requests to attend, and iCalendar messages count. Whether the sender commits to attend depends on context.
  3. *Pleasantries.* "Let me know if you have any questions" is not a request unless context makes it one. Some disagreement stays unresolvable, so "tools need to adapt to the preferences of a specific user."
  4. *Requests for inaction* ("please don't…") are requests. "Don't forget to send…" is a request for action.
  5. *Process instructions* ("file for later" text, "in the event of a fire…") are left to context. The deciding properties are how far off the action is and how likely its trigger is.
  6. *Attachment review.* An attachment counts as a request only when text asks for something.
  7. *Reported requests and commitments* ("Paul asked if you could…" versus "Paul requested your prize be sent late December") depend on context.
  8. *Third-party commitments as requests.* "My assistant Shirley will send you…" is a request to Shirley if Shirley is a recipient.

  **Evaluation stance.** There is no objective gold standard. Evaluating "in the context of real users' mailboxes and task contexts" is more meaningful than an abstract gold set. The authors chose to favour recall for in-message highlighting.

- **Lampert, A., Dale, R., Paris, C. (2009). Segmenting email message text into zones. EMNLP 2009, pp. 919–928.** https://aclanthology.org/D09-1096/. [partial]
  - Zebra, an SVM line and fragment classifier, sorts text into nine zones: author, greeting, signoff, quoted reply, forwarded, signature, advertising, disclaimer and attachment reference. Accuracy is 87.01% over nine zones, 93.60% over two and 91.53% over three.
  - Simple quote heuristics that "used to yield 95% accuracy now find less than 10%" of reply and forward content in Enron.
  - Data: about 400 Enron messages (11,881 lines), single annotator.
- **Lampert, A., Dale, R., Paris, C. (2010). Detecting emails containing requests for action. NAACL-HLT 2010, pp. 984–992.** https://aclanthology.org/N10-1142/. [full]
  - **Data.** 664 random Enron messages, three annotators, κ = 0.681. Only the 505 unanimous messages were kept, so results exclude hard cases. 52.08% contain a request.
  - **Features.** Length, capitalisation, non-alphanumeric counts, Re:/Fw: markers, sender and recipient names, modal-initial and wh-initial sentences, "?" sentences, normalised uni- and bigrams.
  - **Results.** Accuracy 72.28% without zoning. With Zebra, keeping only author, greeting and signoff lines, accuracy rose to 83.76% (weighted F 0.723 to 0.838, p = 0.01), an error reduction of 41%.
    - Without zoning, the top n-grams were Enron automated-report tokens ("hourahead", "westdesk", "iso").
    - With zoning, they became "please", "?", "let me know", "thanks".
  - **Error analysis.**
    - About 20% of errors are implicit requests.
    - About 10% are request-like directives in **marketing and spam**, which annotators agreed were not useful requests. The authors propose filtering those genres out before the request classifier runs.
    - About 5% come from ignoring forwarded zones, because a forward can delegate a task, yet including forwarded text hurt overall.
    - About 5% are zone errors, about 5% pleasantries and about 5% attachment cues.
  - **Lessons for mxr.**
    - Classify only what the account's correspondent wrote. Quoted and forwarded third-party text is context, not the sender's speech act. That is the "forwarded text read as the user's words" failure.
    - Gate on genre first, because marketing imperatives look exactly like requests (the "Pay now" failure).
    - Keep forwards as a separate case: delegation is possible but rare.

### Bennett and Carbonell (2005, 2007): sentence-level labels train better message-level detectors than message-level labels

- **Bennett, P. N., Carbonell, J. G. (2005). Detecting action-items in e-mail. SIGIR 2005 (poster).** https://www.microsoft.com/en-us/research/publication/detecting-action-items-e-mail/. [poster not read; the companion paper below was read in full]
- **Bennett, P. N., Carbonell, J. G. (2005). Feature representation for effective action-item detection. SIGIR 2005 Workshop on Beyond Bag-of-Words.** https://www.cs.cmu.edu/~jgc/publication/Feature_Representation_Effective_ACM_2005.pdf. [full]
  - **Data and definition.** 744 messages from academic volunteers, with quoted material and signatures hand-stripped. Quoted material could also leak test text into training folds. An action item is "an explicit request for information that requires the recipient's attention or a required action."
  - **Agreement.** At message level, 93% raw agreement, κ 0.85. Over 6,301 sentences, κ 0.82. After reconciliation, 328 of 744 messages had action items.
  - **Why annotators disagreed.** Mostly oversight or **conditionals**: "If you would like to keep your job, come to tomorrow's meeting" is an action item; "If you would like to join the football betting pool, come…" is not.
  - **Results.** Message-detection F1 for SVM went from 0.690 with document unigrams to 0.743 with document n-grams and 0.768 with sentence n-grams. kNN with sentence n-grams reached 0.779. Accuracy peaked at 0.817.
    - Features: n-grams up to 4 with χ² selection, sentence-start and sentence-end tokens, and a sentence-position octile.
    - Aggregation: a message is positive if any sentence is, and its ranking score is length-normalised. Longer messages raise false positives.
- **Bennett, P. N., Carbonell, J. G. (2007). Combining probability-based rankers for action-item detection. NAACL-HLT 2007, pp. 324–331.** https://aclanthology.org/N07-1041/. [partial]
  - Stacking and "striving" meta-classifiers over document-view and sentence-view rankers give rankings less sensitive to training-set variation. The paper restates that sentence-level classifiers typically beat document-level ones on this problem.
- **Lesson for mxr.** Label at sentence or span level, decide at message level. The span is also what the UI shows as evidence. Normalise for length, because long newsletters accumulate spurious cues.

### Corston-Oliver et al. (2004): sender-disjoint evaluation, word n-grams suffice, and "fail soft" presentation

- **Corston-Oliver, S., Ringger, E., Gamon, M., Campbell, R. (2004). Task-focused summarization of email. ACL-04 Workshop: Text Summarization Branches Out, pp. 43–50.** https://aclanthology.org/W04-1008/. [full]
- **What they did.** SmartMail, from Microsoft Research.
  - 15,741 messages; training used 106,700 sentences from 14,535 messages.
  - At most 50 messages per sender, "to avoid overtraining to individual writing styles."
  - 3,098 senders, each assigned wholly to train or test, "to ensure that our evaluations are indicative of performance on messages from previously unencountered senders."
  - Sentence tags: Salutation, Chit-chat, Task, Meeting, Promise, Farewell, signature parts, None. Pairwise κ ranged from 0.823 to 0.858.
  - A Task is something that belongs on a to-do list. Simple factual questions were excluded. Formulaic endings were judged in the context of the whole message.
- **Results.** Reported only as PR curves.
  - Deep linguistic features were "negligible" over word n-grams.
  - Message-level detection is more accurate than sentence-level.
  - Task sentences are rewritten into imperatives with deictic resolution: "me" becomes the sender's name and "Thursday" becomes an absolute date.
- **Deployment finding.** Automatically adding tasks to the to-do list "received a fairly negative reception" because it is "fail hard". A checkbox list in the reading pane is "fail soft". A slider lets the user set the probability threshold.
- **Lessons for mxr.**
  - Evaluate on held-out senders.
  - Cap examples per sender.
  - Resolve "I", "you" and relative dates against the owner and the sent date before storing an item.
  - Note for the decision record: BK's standing rule is that detected to-dos are created automatically ([32](../../blueprint/32-classifier.md)). The 2004 user reaction is evidence that automatic creation needs a higher precision bar than suggestion does, not a reason to reverse that decision.

### Other speech-act work in email and forums

- **Jeong, M., Lin, C.-Y., Lee, G. G. (2009). Semi-supervised speech act recognition in emails and forums. EMNLP 2009, pp. 1250–1259.** https://aclanthology.org/D09-1130/. [partial]
  - The data was 40 BC3 email threads with 12 speech-act tags, κ 0.79.
  - The transfer baseline scored **micro-F1 78.87 but macro-F1 37.44**. Supervised training on the email data scored 90.95 micro and 75.71 macro.
  - **Lesson for mxr.** A metric pooled over all sentences or classes hides rare-class failure; mxr's 99.8% accuracy against 7/98 recall is the same effect.
- **Scerri, S., Gossen, G., Davis, B., Handschuh, S. (2010). Classifying action items for semantic email. LREC 2010.** http://www.lrec-conf.org/proceedings/lrec2010/pdf/39_Paper.pdf. [abstract]
  - Rule-based speech-act features. "Full automation is not feasible," but the output is good enough as suggestions to review, and the rules were to bootstrap a learned model.
- **Qadir, A., Riloff, E. (2011). Classifying sentences as speech acts in message board posts. EMNLP 2011.** https://aclanthology.org/D11-1069/. [title and authors only; downloaded, not analysed]

---

## 2. Microsoft and later work

### Lin et al. (2018): annotate the recipient's likely action, not the sender's intent

- **Lin, C.-C., Kang, D., Gamon, M., Khabsa, M., Awadallah, A. H., Pantel, P. (2018). Actionable email intent modeling with reparametrized RNNs. AAAI 2018.** https://doi.org/10.1609/aaai.v32i1.11931 · https://arxiv.org/abs/1712.09185. [partial]
- **What they did.** Annotators imagine being the addressee, read the previous messages in the thread, and pick the most likely action. The options are reply-yes/no, reply-ack, reply-other, investigate, send-new-email, setup-appointment, approve-request, share-content and other.
  - 399 Avocado emails in 110 complete, linear threads. κ 0.75.
  - They argue the scheme is theory-agnostic and less prone to annotator bias than sender-intent taxonomies.
- **Result.** Transfer from IRC and Reddit threads helps, but "all the F1 scores are low": about 0.26–0.31 in the transfer breakdown. The labelled set is far too small for end-to-end training.
- **Lesson for mxr.** "What does this email ask of you" is literally a recipient-action label. Defining labels as the owner's next action makes the owner's perspective explicit by construction, and maps directly to UI affordances such as reply, pay or sign.

### Azarbonyad, Sim and White (2019): commitment detectors work in-domain and lose AUC from 0.86–0.88 to 0.58–0.67 across corpora

- **Azarbonyad, H., Sim, R., White, R. W. (2019). Domain adaptation for commitment detection in email. WSDM 2019, pp. 672–680.** https://doi.org/10.1145/3289600.3290984 · https://www.microsoft.com/en-us/research/uploads/prod/2019/01/AzarbonyadWSDM2019.pdf. [full]
- **Annotation question.** "Does the highlighted sentence contain a specific action that the sender must complete or is obliged to do? (The action must be on the sender and must not already be complete)." The sentence was shown highlighted inside its email. Two annotators, a third on disagreement, Krippendorff's α 0.73.
- **Data.**
  - Enron: 65,398 sentences, 3,337 positive (5.1%). Mostly a random sample, plus 4,000 sentences sampled by a classifier.
  - Avocado: 13,021 sentences, 4,484 positive. Selected by the classifier, so "the distribution … does not reflect the true base rate."
- **Results (logistic regression on word n-grams).**
  - In-domain F1/AUC: Avocado 0.81/0.86, Enron 0.78/0.88.
  - Cross-domain: Avocado→Enron 0.73/0.67, Enron→Avocado 0.76/0.58.
  - A domain classifier separates the corpora at F1 0.85 using words like "enron", "gas" and "houston".
  - Importance sampling, linear mapping and an adversarial autoencoder recover F1 to 0.77–0.81.
  - The top Enron positive features were "i will", "i'll", "let you know", "call you" and "we will". The top-10 feature sets of the two corpora overlap at only 43% (Jaccard).
  - The paper does not say whether P, R and F1 are for the positive class or averaged.
- **Lessons for mxr.**
  - The annotation question encodes two constraints mxr's extractor lacks: the actor is the sender, and the action is not already complete.
  - Any model or prompt tuned on one population (Enron-like work mail, or BK's inbox) should be expected to degrade on another. Evaluation must include held-out senders or mailboxes.

### Wang et al. (2019): context lifts human recall on request sentences from 52.9% to 76.6% and model F1 by 3–6 points

- **Wang, W., Hosseini, S., Awadallah, A. H., Bennett, P. N., Quirk, C. (2019). Context-aware intent identification in email conversations. SIGIR 2019, pp. 585–594.** https://doi.org/10.1145/3331184.3331260 · https://www.microsoft.com/en-us/research/uploads/prod/2019/05/Wang_SIGIR19.pdf. [full]
- **Intent distribution.** 1,300 random Avocado threads; first message annotated by three annotators, κ 0.694.
  - Categories: information exchange (share, request), task management (request action, promise action), scheduling (schedule meeting, reminder), social.
  - 55.2% of messages carry one intent, 35.8% two and 9.0% three or more. Share-information and request-information frequently co-occur.
- **Human study on request-information.** 540 sentences, half positive, two disjoint crowd groups.
  - With the email body: precision 93.45, recall 76.61.
  - Sentence only: precision 91.51, recall 52.86.
  - α was 0.58 and 0.56, so the groups annotated equally consistently. Missing context mostly costs recall.
- **Model data.** Sentence-level datasets for schedule meeting (47,914 sentences, 15.5% positive), promise action (9,076, 28.5%) and request information (7,080, 19.6%), each with κ > 0.6.
  - **Negatives were hard negatives**: only unlabelled sentences that contain one of the top-100 TF-IDF words of the positive class.
  - Quoted text, greetings and signatures were excluded.
  - Splits were made **by user**.
- **Results (F1).** The sentence–context attention model (DCRNN) scored 73.48 / 80.42 / 78.37 on schedule meeting / promise action / request information. The best sentence-only model scored 67.12 / 75.21 / 75.45.
  - Concatenating context features into LR or SVM did not help.
  - Context alone did badly: 43.29 / 61.33 / 67.12.
- **Lessons for mxr.**
  - Hard-negative construction is the right tool for "marketing with 'Pay now'": negatives should share the positive vocabulary.
  - Context must be read *in relation to* the target sentence, not as a bag of extra words. A prompt that hands an LLM 12 messages per contact is the concatenation design, which did not help the linear models here *(inference about the analogy)*.

### Rameshkumar et al. (2018): who must act is a separate, noisy task, and a model scoring 0.9/0.9 in-domain falls below a naive baseline on a new corpus

- **Rameshkumar, R., Bailey, P., Jha, A., Quirk, C. (2018). Assigning people to tasks identified in email: The EPA dataset for addressee tagging for detected task intent. W-NUT @ EMNLP 2018, pp. 28–32.** https://aclanthology.org/W18-6104/. [full]
- **Task.** Given a detected task sentence and the Sender, To and Cc list, decide for each person whether they are responsible. "No-one" is allowed: in "Brad will complete the draft", Brad may not be on the email.
  - Implicit "you" in imperatives can be singular or plural.
  - The responsible person may be identifiable only from client-generated quoted headers.
- **Data.** Enron: 6,300 tasks and 15,649 (email, task, recipient) tuples. Perfect agreement 71.07%, α 0.6123. An Avocado version reached 84.51% and α 0.7854, attributed to cleaner formatting.
- **Results.** The production model, trained on Avocado, scored P/R 0.9/0.9 on an Avocado blind set. On Enron it scored F1 0.78 for single-recipient and 0.66 for multi-recipient emails. The naive "every recipient is responsible" baseline scored 0.80 and 0.61.
- **Lessons for mxr.**
  - Responsibility assignment is its own step, with its own evaluation, and it needs the owner's identity as an input.
  - The owner can be the sender, a To recipient, a Cc recipient or nobody.
  - In-domain scores say little about a new mailbox.

### Mukherjee et al. (2020, Smart To-Do): a commitment classifier at P 0.86, R 0.84, then generation; the replied-to message held useful context in 70% of cases

- **Mukherjee, S., Mukherjee, S., Hasegawa, M., Awadallah, A. H., White, R. (2020). Smart To-Do: Automatic generation of to-do items from emails. ACL 2020, pp. 8680–8689.** https://aclanthology.org/2020.acl-main.767/. [partial]
- **Commitment classifier.** 9,076 Avocado sentences labelled by three judges (2,586 positive), κ 0.694. It is an RNN with self-attention over the whole email, from Wang et al. 2019. Precision 0.86, recall 0.84.
- **To-do generation data.** Candidates were thresholded at 0.9 confidence, giving 29k of 500k sentences, of which 12k were sampled.
  - Annotators saw the current email, the previous email in the thread, From, SentTo and Subject.
  - 9,349 instances with two to-dos each. A median to-do is 9 tokens. The subject line helped in 60.42% of cases.
  - Of 100 instances, 44 had a replied-to email, and 31 of those (70.4%) contained a helpful sentence.
  - The two-stage generation model reached BLEU 0.23 and ROUGE 0.63.
- **Lessons for mxr.**
  - Detect first, then extract with the immediately preceding message as context.
  - The output is rewritten from the owner's perspective: "Send the product launch sales report to Alice."
  - The 0.9 threshold shows the deployment pattern: run generation only on high-confidence detections.

### Sappelli et al. (2016): about half of corporate messages contain a task, and a classifier trained on one company's mail works on another's

- **Sappelli, M., Pasi, G., Verberne, S., de Boer, M., Kraaij, W. (2016). Assessing e-mail intent and tasks in e-mail messages. *Information Sciences* 358–359:1–17.** https://doi.org/10.1016/j.ins.2016.03.002. [abstract; the author manuscript at boa.unimib.it returned 403]
  - A task taxonomy, annotated on Enron and Avocado. About half of corporate messages contain at least one task, "mostly informational or procedural."
  - The number of tasks per message is predicted with 71% accuracy, and cross-company training works.
  - Dimensions include whether a reply is expected and the task's spatial and time sensitivity. The full taxonomy was not reachable, so it is unverified here.

### Behaviour-labelled work: reply, deferral, completion and actions

- **Kannan, A. et al. (2016). Smart Reply: Automated response suggestion for email. KDD 2016.** https://arxiv.org/abs/1606.04870. [partial, triggering section]
  - **Labels and data.** A feed-forward "triggering" model decides whether a message needs a reply. Positives are messages replied to from mobile; negatives are downsampled to balance training.
  - **Features.** Content n-grams plus social signals: whether the sender is in the address book or social network, and whether the recipient has replied to this sender before.
  - **Evaluation.** The split is temporal, with test messages delivered after train messages. AUC 0.854, and the model triggers on about 11% of messages.
  - The canonical negative is "a newsletter containing a sentence 'Where do you want to go today?'"
  - **Lesson for mxr.** Gate on whether the mail expects a response before analysing its sentences, and use relationship history, which mxr has locally.
- **Yang, L., Dumais, S., Bennett, P. N., Awadallah, A. H. (2017). Characterizing and predicting enterprise email reply behavior. SIGIR 2017.** https://doi.org/10.1145/3077136.3080782. [abstract]
  - Content, metadata, historical-interaction and temporal features predict whether and when a recipient replies, on Avocado.
- **Sarrafzadeh, B. et al. (2019). Characterizing and predicting email deferral behavior. WSDM 2019.** https://doi.org/10.1145/3289600.3291028 · https://arxiv.org/abs/1901.04375. [not read beyond MSR blog: 15 interviews, logs from 40,000 users]
- **White, R. W., Awadallah, A. H., Sim, R. (2019). Task completion detection. SIGIR 2019.** https://doi.org/10.1145/3331184.3331187. [abstract]
  - From digital-assistant logs, completion of tasks such as commitments is detected with accuracy above 80%. Features: time since creation, task content, email, notifications and user history. The motivation is to stop reminding users about done tasks.
  - **Lesson for mxr.** Lifecycle matters. A detected obligation needs a "resolved" signal, such as a later payment confirmation.
- **Di Castro, D., Karnin, Z., Lewin-Eytan, L., Maarek, Y. (2016). You've got mail, and here is what you could do with it! Analyzing and predicting actions on email messages. WSDM 2016, pp. 307–316.** https://doi.org/10.1145/2835776.2835811. [abstract]
  - On Yahoo mail, the most frequent actions are read, reply, delete and delete-without-read.
  - At 90% recall, read and reply are predicted at precision up to 40% for active users.
  - "Horizontal" regularisation across users raises AUC by close to 50% for less active users.

### LLM-era work (2022–2026) on email task and obligation extraction is thin

I searched arXiv, OpenAlex and the web for LLM evaluations of request, commitment and action-item detection in email and found no peer-reviewed benchmark for it. What exists:

- **Srivastava, S. et al. (2023). MAILEX: Email event and argument extraction. EMNLP 2023.** https://arxiv.org/abs/2305.13469. [partial]
  - About 1.5K threads, about 4K emails and about 8K events. Ten event types built from the verbs Request, Deliver and Amend and the nouns Data, Meeting data, Action data and Action, with 76 argument roles. Deliver events carry a "Confirmation" role (positive, negative, tentative).
  - Agreement: κ 0.791 on triggers and event types, 0.810 on arguments.
  - Few-shot GPT-3.5 "behaved substantially worse" than fine-tuned BART and sequence-labelling models: about 0.05 argument-classification F1 end to end. Even with gold triggers it trailed by more than 0.3 F1. 42% of its sampled errors were semantically incorrect or fabricated arguments.
  - The authors name history modelling as an open problem.
- **Singh, M. et al. (2026). EmailBench: A benchmark for evaluating LLM agents on enterprise email and productivity tasks. arXiv:2609.31906.** [abstract]
  - Agentic tasks, not obligation detection. The best configuration passes 33.5% of 206 scenarios while 99.7% of its tool calls succeed: "valid tool execution is not equivalent to task completion."
- **Diwanji, P., Guo, H., Kalia, A. K., Singh, M. P. (2020). Liṅ: Unsupervised extraction of tasks from textual communication. COLING 2020.** https://aclanthology.org/2020.coling-main.164/. [abstract and introduction]
  - Uses dependency parses plus VerbNet to find task main verbs, and reports parity with supervised models plus robustness on unseen domains.
- **Indirect speech acts in LLMs.** [abstract]
  - A Korean benchmark: arXiv:2502.10995.
  - The Italian INDIR-IT benchmark (CEUR-WS Vol-4112).
  - Both report LLMs doing worse on indirect than direct speech acts, and worse still on non-conventional ones.
  - Not email-specific; unverified beyond abstracts.
- **Lesson for mxr.** No published result tells you how well a local LLM detects owner-directed obligations in personal mail. The only email-specific LLM evaluation found (MAILEX) shows few-shot prompting far below supervised extraction. mxr will have to measure its own.

---

## 3. Machine-generated mail

### Machine-generated mail is 60–90% of consumer traffic; classify it apart from human mail, by sender or template, with a message-level fallback

- **Ailon, N., Karnin, Z. S., Liberty, E., Maarek, Y. (2013). Threading machine generated email. WSDM 2013.** https://doi.org/10.1145/2433396.2433447. [abstract]
  - Machine-generated mail is above 60% of Yahoo traffic.
  - "Causal threading" (receipt, then shipment notice) works in four stages: identify templates, build a template-level causal graph, learn a causal-relation predictor, then thread the stream.
  - On 12.5M opted-in users: above 70% recall at 90% precision by editorial judgment, and about 80% precision at 90% recall in a search scenario.
  - **Lesson for mxr.** Obligation lifecycle (bill, then payment confirmation, then overdue) can be linked at template level.
- **Grbovic, M., Halawi, G., Karnin, Z., Maarek, Y. (2014). How many folders do you really need? Classifying email into a handful of categories. CIKM 2014, pp. 869–878.** https://doi.org/10.1145/2661829.2662018 · https://arxiv.org/abs/1606.09296. [partial]
  - Over six months and more than 500B messages, non-spam machine-generated mail was 90% of traffic.
  - LDA over 100k user folders found six latent categories: one human and five machine. The paper names human, career, shopping, travel and financial as the chosen set, and its LDA table also shows social.
  - They argue that "one should first distinguish between machine and human-generated messages before attempting any finer classification."
  - **Aggregation levels.** 1.4T messages, 2.6B raw senders, 339M filtered senders and 75.8M domains.
    - Sender-level classification is a table lookup.
    - **When the sender's label is low-confidence, fall back to per-message classification.**
    - Named losses: the *machine–human mix* (a recruiter or broker sends both templated and personal mail) and *multiple-businesses loss* (one address serves several lines of business, common at domain level). Their examples include payroll@, careers@ and orders@ on one domain carrying different categories.
  - Manual editorial labels were inconsistent. Labels came from folder votes, heuristics and co-training.
  - Precision and recall were about 0.9 for most categories.
- **Wendt, J. B., Bendersky, M., Garcia-Pueyo, L., Josifovski, V., Miklos, B., Krka, I., Saikia, A., Yang, J., Cartright, M.-A., Ravi, S. (2016). Hierarchical label propagation and discovery for machine generated email. WSDM 2016, pp. 317–326.** https://doi.org/10.1145/2835776.2835780. [abstract]
  - A document is a template layer plus a document-specific layer. Labels propagate over a template graph built from term or topic similarity, then down to matching documents.
  - Above 91% precision and 93% recall, with label coverage up more than 11% over the document classifier.
- **Avigdor-Elgrabli, N., Cwalinski, M., Di Castro, D., Gamzu, I., Grabovitch-Zuyev, I., Lewin-Eytan, L., Maarek, Y. (2016). Structural clustering of machine-generated mail. CIKM 2016, pp. 217–226.** https://doi.org/10.1145/2983323.2983350. [abstract]
  - Clusters on the HTML structure of bodies, beyond header-based clustering, at strict or flexible granularity. The authors call this "especially critical for mail extraction."

### Juicer: bills are a document type with subtypes and required fields, its bill classifier reaches only 66.8% precision on unseen templates, and a balance statement was a typical false positive

- **Sheng, Y., Tata, S., Wendt, J. B., Xie, J., Zhao, Q., Najork, M. (2018). Anatomy of a privacy-safe large-scale information extraction system over email. KDD 2018, pp. 734–743.** https://doi.org/10.1145/3219819.3219901 · https://research.google/pubs/anatomy-of-a-privacy-safe-large-scale-information-extraction-system-over-email/. [full]
- **Architecture.**
  - **Templates.** Templates are induced from a 0.5% sample of 90 days of Gmail. Two methods:
    - sender plus subject regex, where fixed terms are seen in at least k documents and n users;
    - MinHash over the XPaths of DOM leaf nodes.
  - Failure modes: *conflation* (two templates in one cluster) and *fragmentation*.
  - **Classification.** An email-level "vertical" classifier (bill, offer, hotel…) is aggregated to a template label. Per-field classifiers score type-generated candidates (dates, currency amounts), and scores averaged per XPath become extraction rules.
  - **Online extraction.** Look up the template and apply its rules. "**If any of the required fields for a vertical fail to extract, we discard the other field values and generate nothing.**"
- **Bills.**
  - Two classifiers: is it a bill, and which subtype? The subtypes are **payment confirmation, autopay notification, reminder, overdue**, and only reminder and overdue are extracted.
  - Training data was about 1M positives and 200M negatives. Negatives are downsampled for training, but "final precision and recall metrics [are computed] on the full template-level data."
  - Holdout AUC-ROC 0.9727 but **AUC-PR 0.8010**: "While the AUC-ROC value exceeding 0.97 … might suggest that this is an easy problem; the extreme imbalance … makes this a challenging task."
  - Precision at a threshold set for 90% holdout precision:

    | Template set | Structural templates | Sender-subject templates |
    |---|---|---|
    | Existing (known to parsers) | 84.6% | 94.1% |
    | New (unseen) | **66.8%** | 76.2% |

  - The cited false positive is "a statement of an account balance" read as a bill reminder, because its field types and language are similar.
  - **Recall loss.** Only 73.3% of known bill emails match a positive template, and only 58.7% match templates with rules.
- **Generalisation.** The offers classifier had holdout precision 0.993 but 0.85 on new templates.
  - Training data from popular, sophisticated senders does not represent the long tail.
  - Mitigations: stratified sampling **per template**, so large senders do not dominate; regularisation against sender-specific features; active learning.
  - Legacy parsers "may erroneously parse and extract a bill reminder from an email that doesn't actually contain one" when a new template resembles an old one.
- **Design choices.**
  - Separate binary classifiers per vertical, with their own precision/recall trade-offs, instead of one multi-label model.
  - Email-level classification aggregated to the template, so conflated templates are visible.
  - Candidate generators must be high recall, because they cap the system's recall; precision comes from the classifier.
- **Evaluation under privacy.** Donated mail, a rater programme, and synthetic emails that keep only k-anonymous fixed template text, rated by humans.
- **Lessons for mxr.**
  - Several of mxr's failures are subtype errors inside one vertical: "final reminder" read as a payment, "Why not set up a Direct Debit?" read as paid, a credit note or £0.00 due read as a bill. Juicer's answer is an explicit subtype classifier and required fields.
  - A bill obligation should exist only when the subtype is reminder or overdue *and* an amount-due and due-date candidate both extract. The amount > 0 check is mxr's own validation rule, not Juicer's *(inference)*.
  - Expect holdout precision to overstate real precision on senders not seen in development.
- **Sheng, Y., Vo, N., Wendt, J. B., Tata, S., Najork, M. (2020). Migrating a privacy-safe information extraction system to a Software 2.0 design. CIDR 2020.** https://www.vldb.org/cidrdb/papers/2020/p31-sheng-cidr20.pdf. [partial]
  - The rule system ran in production from 2013. "Oftentimes, fixing error cases results in breaking correct cases."
  - The migration used the legacy extractions as training labels. The first models replaced only 6% of rule extractions; closing the gap meant building high-coverage *CandidateGenerators*, high-precision *CandidateLabelers* and ground-truth cleanup, because legacy labels contained bad rules.
  - The result beat the rules on precision and recall, and over 45K lines of code were deleted. Templates are released in weekly batches only when precision on the batch is above 90%.
  - Machine translation enabled non-English extraction.
  - **Lesson for mxr.** The current regex set is a usable source of weak labels, not ground truth.
- **Kocayusufoglu, F., Sheng, Y., Vo, N., Wendt, J., Zhao, Q., Tata, S., Najork, M. (2019). RiSER: Learning better representations for richly structured emails. WWW 2019.** https://doi.org/10.1145/3308558.3313720. [abstract]
  - Jointly encoding the HTML structure and the words of B2C mail beats text-only attention LSTMs for classification.
- **Majumder, B. P., Potti, N., Tata, S., Wendt, J. B., Zhao, Q., Najork, M. (2020). Representation learning for information extraction from form-like documents. ACL 2020, pp. 6495–6504.** https://aclanthology.org/2020.acl-main.580/. [partial]
  - Candidates are generated by field *type*, then scored by a learned representation of their neighbouring words and relative positions.
  - Evaluation is on **unseen templates**: a 60-40 template split, plus a second invoice corpus that shares no templates.
  - On invoices, `amount_due` (4.8% of candidates positive) scores ROC-AUC 0.973 but end-to-end max F1 0.801. `due_date` scores 0.984 and 0.861.
  - About 93% of invoice dates had "date" among neighbouring words, and about 7% needed other cues.
  - The authors note domain constraints such as a due date not preceding the invoice date.
  - **Lesson for mxr.** The amount of a bill is a typed field chosen among candidates by context ("amount due", "total", "credit"). The candidate generator has to recognise the currency formats in use, for example "R953.63" (South African rand), or recall is capped at zero for them.

---

## 4. Datasets and annotation

| Dataset | Source | Unit and labels | Size | Agreement | Availability |
|---|---|---|---|---|---|
| CSpace + PW CALO (Cohen 2004; Carvalho 2006) | MBA team simulation; SRI exercise | message, multi-label verb–noun acts | 1,357, later 1,716 messages | κ 0.72–0.83 | via Ciranda / CMU at the time; current status unverified |
| Bennett & Carbonell 2005 | academic volunteers | message plus highlighted spans, action item | 744 messages, 6,301 sentences | κ 0.85 (message), 0.82 (sentence) | "available for some outside experimentation" on request |
| SmartMail (Corston-Oliver 2004) | Microsoft internal | sentence tags incl. Task, Promise, Meeting | 15,741 messages | pairwise κ 0.82–0.86 | not public |
| Lampert 2008–2010 | Enron | sentence and message, request/commit; 664-message request set | κ tables above | κ 0.54–0.84 | five-annotator set at annotate.thoughtlets.org (link not checked) |
| BC3 + Jeong 2009 | W3C threads | sentence, 12 acts | 40 threads | κ 0.79 | BC3 from UBC |
| Azarbonyad 2019 | Enron, Avocado | sentence in email, sender commitment | 65,398 + 13,021 sentences | α 0.73 | not stated |
| Wang 2019 | Avocado | sentence: schedule meeting, promise action, request info | 47,914 / 9,076 / 7,080 | κ > 0.6 | Avocado licence (LDC2015T03) limits release |
| Lin 2018 | Avocado | message: recipient's next action | 399 emails, 110 threads | κ 0.75 | not stated |
| EPA (Rameshkumar 2018) | Enron (Avocado version not released) | (email, task, recipient) responsible? | 15,649 tuples | α 0.61 (Enron), 0.79 (Avocado) | http://aka.ms/epadataset |
| Smart To-Do (Mukherjee 2020) | Avocado | commitment sentence to to-do text | 9,349 | κ 0.694 (classifier labels) | https://aka.ms/SmartToDo, subject to Avocado terms |
| MAILEX (2023) | Enron threads (Minkov et al. version) | event triggers and 76 argument roles | about 1.5K threads | κ 0.79 / 0.81 | released with the paper |
| Machine mail (Yahoo, Google) | production, opt-in or donated | template or vertical labels | 10⁸–10¹² messages | editors inconsistent (Grbovic) | none public |

The base corpora are Enron (Klimt and Yang, ECML 2004, pp. 217–226), about 250K deduplicated messages in the Fiore and Heer dump, and Avocado (Oard et al. 2015, LDC2015T03), with 279 accounts and 938,035 emails. Wang 2019 and Mukherjee 2020 note Avocado entered research use with the owner's consent, unlike Enron. **No public labelled set covers consumer or B2C mail for obligations** such as bills, renewals, verification or RSVPs.

**What annotators found hard, with sources:**

- **Commitments versus requests.** Commitment agreement was lower everywhere: 0.54 against 0.78 for requests at first (Lampert); Commit F1 0.47 against Request 0.69 (Cohen).
- **Conditional requests and commitments.** These were the main source of disagreement for Bennett and Carbonell, and needed explicit rules to reach moderate agreement (Lampert 2008b).
- **Indirect requests and pleasantries.** "Let me know if…" (Lampert 2008b, 2010; Corston-Oliver 2004). Implicit requests were 20% of request-classifier errors (Lampert 2010).
- **Locus.** The same obligation is stated twice, so sentence-level agreement is below message-level (Lampert 2008b; Cohen 2004).
- **Reported requests, and third parties.** Includes third-party commitments that become requests when that person is a recipient (Lampert 2008b).
- **Group addressees, implicit "you", and nobody on the email being responsible** (EPA 2018). Mailing lists hide who receives the mail, so Lampert considered only individual addresses.
- **Formatting.** Enron's noisy formatting lowered EPA agreement from α 0.79 to 0.61, and simple quote detection finds less than 10% of quoted content (Lampert 2009).
- **Marketing and spam.** Their imperatives are not requests, and annotators agreed on that, but the classifier did not (Lampert 2010).

---

## 5. How these papers evaluate

- **Unit of evaluation.**
  - Message-level: Cohen 2004, Lampert 2010, Lin 2018, Grbovic 2014.
  - Sentence-level: Wang 2019, Azarbonyad 2019, Corston-Oliver 2004, Jeong 2009.
  - Span or argument-level: MAILEX, with partial credit for overlapping spans.
  - Field-level, end-to-end: Juicer, Majumder.
  - Sentence labels give better message-level detectors (Bennett 2005). Message-level agreement and accuracy are higher than sentence-level (Lampert 2008b; Corston-Oliver 2004).
- **Metrics.**
  - Per-class precision, recall and F1 for the positive class is the norm, with one binary task per act or vertical (Cohen, Wang, Azarbonyad, Juicer).
  - Accuracy appears only where classes are near-balanced (Lampert 2010: 52% positive; Bennett: 44%).
  - When classes are imbalanced, the papers that report both show the gap:
    - micro-F1 78.87 against macro-F1 37.44 (Jeong 2009);
    - AUC-ROC 0.97 against AUC-PR 0.80 for bills (Juicer);
    - ROC-AUC 0.973 against max F1 0.80 for `amount_due` (Majumder).
  - Saito and Rehmsmeier (2015, *PLOS ONE*, https://doi.org/10.1371/journal.pone.0118432) give the general argument that PR beats ROC on imbalanced data. Cited by title; not re-read.
- **Class imbalance.**
  - Train on downsampled negatives and report on the natural distribution (Juicer; Smart Reply trigger).
  - Enrich positives by classifier-weighted sampling, then warn that the base rate is no longer real (Azarbonyad).
  - Build hard negatives that share positive vocabulary (Wang).
  - Report positive and negative counts per class (Cohen).
- **Held-out units.**
  - Senders: sender-disjoint splits with a cap of 50 messages per sender (Corston-Oliver).
  - Users: user-disjoint splits (Wang).
  - Teams and corpora: cross-team (Cohen) and cross-corpus (Azarbonyad, EPA).
  - Templates: unseen templates (Juicer: holdout against new-template precision; Majumder: template-disjoint).
  - Time: test messages after training messages (Smart Reply).
  - Every paper that measured a shift reported a drop:
    - AUC 0.86 to 0.67 and 0.88 to 0.58 (Azarbonyad);
    - P/R 0.9/0.9 to F1 0.78/0.66, below a naive baseline for single recipients (EPA);
    - precision 0.993 to 0.85 for offers and 84.6% to 66.8% for bills (Juicer).
  - Cohen 2004 is the exception: no clear cross-team drop.
- **Effect of context.**
  - Full body against sentence: human recall rises 52.86 to 76.61 at similar precision, and model F1 rises 3–6 points when context is attended in relation to the sentence (Wang 2019).
  - Preceding message: helpful in 70.4% of replies (Mukherjee 2020).
  - Thread neighbours: significant for some acts only (Carvalho 2005).
  - Concatenated context: no help for LR or SVM (Wang 2019).
- **Gold standard and agreement.** Results are bounded by κ of about 0.6–0.85, and Lampert argues there is no objective gold standard. Lampert 2010 trained and tested on unanimous items only, which inflates scores against messages people disagree on. Grbovic found editors inconsistent. Juicer evaluates with human raters on synthetic, anonymised renderings.

---

## Design lessons

1. **Split by mail genre before looking for obligations.** First decide person-written versus machine-generated, and within machine mail, marketing versus transactional or account notices.
   - Support: Grbovic 2014; Lampert 2010 (10% of errors were marketing and spam directives); Smart Reply triggering (the newsletter-with-a-question example); Juicer (vertical classification precedes fields).
   - mxr failures addressed: "Pay now" in marketing read as a bill; event-platform announcements read as RSVPs.

2. **Treat organisational obligations as document type, subtype and required typed fields, not as speech acts.** For bills: subtype in {confirmation, autopay, reminder, overdue, …}. An obligation exists only for actionable subtypes, and only when the amount-due and due-date fields extract. If a required field fails, emit nothing.
   - Support: Juicer 2018 (subtype classifier; the "discard … and generate nothing" rule; the balance statement as false positive); Majumder 2020 (typed candidates, domain constraints).
   - mxr failures addressed: "final reminder" read as a payment; a credit note or "amount due £0.00" read as a bill; "Why not set up a Direct Debit?" read as paid. The last two need subtype and polarity, not a phrase veto.

3. **Make the candidate generators high recall and locale-aware; let a classifier choose among candidates.** The generator caps recall: a currency generator that does not know "R953.63" can never extract it.
   - Support: Juicer 2018; Sheng 2020 (CandidateGenerators and CandidateLabelers as the main migration work); Majumder 2020.

4. **Aggregate evidence per sender template, not per sender or domain, and fall back to message-level classification when the aggregate is unsure.** One sender address can carry several businesses or templates, so per-sender merging conflates them.
   - Support: Grbovic 2014 (machine–human mix, multiple-businesses loss, low-confidence fallback); Juicer (template conflation and fragmentation; label the email first, then aggregate); Wendt 2016.
   - mxr failure: two accounts billed by one sender were merged. The literature covers label conflation, not account identity; see the open questions below.

5. **Zone the body; attribute each span to an author.** Classify the correspondent's own text. Use quoted text and the immediately preceding message as context for interpretation. Treat forwarded third-party text as someone else's words unless the forward itself delegates.
   - Support: Lampert 2009 and 2010 (41% error reduction; forwards as 5% of errors but net harmful to include); Carvalho and Cohen 2006; Wang 2019; Bennett 2005 (quote leakage across folds).
   - mxr failure: forwarded third-party text read as the user's words.

6. **Make owner identity and direction first-class inputs.** Every candidate obligation needs an actor (who must act: owner, correspondent, third party, nobody) and a direction:
   - incoming request to the owner;
   - outgoing request from the owner, which is "awaiting others";
   - owner commitment;
   - others' commitment to the owner.
   - Support: Cohen 2004 (Fig. 1 separates outgoing requests from incoming commitments); Lampert 2008b (third-party commitments, recipient-dependent requests); EPA 2018 (addressee tagging, the "no-one" label, implicit "you"); Azarbonyad 2019 ("the action must be on the sender"); Corston-Oliver 2004 and Mukherjee 2020 (deictic rewriting to names and dates).
   - mxr failures addressed: "could you book an engineer?" from the owner read as the owner's promise; the LLM not being told who the owner is.

7. **Add temporal status and modality to the label: future and not yet done, versus past, hypothetical, suggested or conditional.**
   - Support: Azarbonyad 2019 ("must not already be complete"); Bennett 2005 and Lampert 2008b (conditionals); Lampert 2008b (process instructions decided by the likelihood of the trigger and how far off the action is; reported requests); White 2019 (completion detection).
   - mxr failures addressed: a support agent narrating a past card decline; "Why not set up a Direct Debit?" as a suggestion.

8. **Label per sentence or span, decide per message, and keep multiple labels.** A message often carries several acts (Cohen: about 1.3 per message; Wang: 44.8% have two or more intents). First-match-wins with whole-email negative vetoes cannot represent that. Use one binary detector per obligation kind with its own threshold rather than an ordered cascade.
   - Support: Bennett 2005 and 2007; Wang 2019; Juicer's per-vertical binary classifiers.

9. **Give the model context in relation to the target sentence; do not pad it with unrelated history.** The body and the preceding message help. Twelve messages per contact concatenated is the design that did not help linear models.
   - Support: Wang 2019; Mukherjee 2020 (previous email useful in 70% of replies); Carvalho 2005 (gains for some acts only); MAILEX (history modelling unsolved).

10. **Write annotation guidelines that rule on the known edge cases, and add an explicit "pleasantry or unsure" class.** Expect κ of about 0.7–0.85, and expect commitments to be harder than requests.
    - Support: Lampert 2008b (eight categories; commitment κ 0.54 to 0.74 after guidance); Bennett 2005; Wang 2019; Lin 2018.
    - A guideline phrased as "what would the recipient do next" (Lin 2018, κ 0.75) matches mxr's product question directly.

11. **Evaluate per obligation kind on the positives.**
    - Report precision, recall and F1 per class and macro-averaged, plus PR-AUC. Never accuracy or ROC-AUC alone on a 99.7%-negative set.
    - Build the evaluation set by enriching positives and adding hard negatives that share positive vocabulary. Then also report precision on a natural-distribution sample.
    - Support: Jeong 2009 (micro 78.9 against macro 37.4); Juicer (ROC 0.97 against PR 0.80); Wang 2019 (hard negatives); Azarbonyad 2019 (an enriched sample distorts the base rate); Saito and Rehmsmeier 2015.
    - mxr failure: 99.8% accuracy alongside 7/98 recall.

12. **Hold out senders, templates and time.** Cap examples per sender. Expect a drop on unseen senders: 84.6% to 66.8% for bills in Juicer, AUC 0.86 to 0.67 in Azarbonyad. Treat the drop as the expected cost and size thresholds against the held-out number.
    - Support: Corston-Oliver 2004; Wang 2019; Juicer 2018; Majumder 2020; Azarbonyad 2019; EPA 2018; Smart Reply.

13. **Use the existing regexes as weak labels for a learned or LLM-checked system, not as the system.** Rules accumulate regressions ("fixing error cases results in breaking correct cases"). Their outputs bootstrap training data but contain wrong labels that need cleanup.
    - Support: Sheng 2020; Juicer 2018 (legacy parsers' false positives); Scerri 2010.

14. **Use local behavioural signals as labels and features.** Examples: whether the owner replied to this sender before, whether the sender is in contacts, whether a later message resolved the item (payment confirmation, reply sent).
    - Support: Smart Reply (reply-labelled training; address-book and reply-history features); Yang 2017; Di Castro 2016; White 2019; Ailon 2013 (template-level causal links).

15. **Because creation is automatic, the precision bar is higher.** Show the source span and keep removal cheap.
    - Support: Corston-Oliver 2004 ("fail hard" auto-add was poorly received; "fail soft" checkboxes and a threshold slider); Scerri 2010 (good enough as reviewed suggestions); Mukherjee 2020 (0.9 confidence gate before generation).
    - BK has chosen automatic creation, so this sets the threshold rather than reopening the decision.

16. **Do not assume an LLM solves pragmatics; measure it on mxr's labelled set.**
    - Support: MAILEX (few-shot GPT-3.5 at about 0.05 argument F1, 42% of sampled errors fabricated or incorrect); indirect-speech-act benchmarks (abstract-level); EmailBench (tool success is not task success).
    - If an LLM is used, give it the owner identity, the zoned author text, the preceding message and a structured schema with actor, direction, status and type. Score it with the same per-class protocol as any other model *(inference from lessons 5–11)*.

---

## What the literature does not answer

- **Consumer-inbox obligations as a labelled task.** Requests and obligations from organisations ("send your meter readings", "upload your CV", "sign the attached consent form", "verify your email", renewals, RSVPs to personal invitations) fall between the two bodies of work. They are not Enron-style person-to-person requests, and only bills, offers, hotels, events and purchases were studied as B2C verticals. There is no public dataset and no reported precision or recall.
- **Single-mailbox scale.** Template induction and aggregation assume many users per template (k-anonymity thresholds, millions of instances). How well template-level aggregation works when one mailbox has a handful of instances per template is not studied.
- **Account and entity identity within a sender.** Distinguishing two accounts or customer numbers billed by the same sender and template is not addressed. Template work is about labels and fields, not about which obligation a field belongs to.
- **Polarity and modality inside machine mail.** "Why not set up a Direct Debit?", "you're all set", "no action needed" and "amount due £0.00" are not studied, beyond Juicer's note that balance statements are confused with reminders.
- **Owner-relative direction in a mixed corpus.** Cohen 2004 sketches incoming versus outgoing tracking, and EPA tags addressees for a given task. No evaluated system detects whether an obligation is on the owner, on others, or is the owner's outgoing request, across sent and received mail together.
- **Currency, locale and language.** Only CIDR 2020 mentions cross-language extraction, through machine translation. Rand ("R" prefix) and other non-£/$/€ formats are not discussed anywhere.
- **Obligation lifecycle across messages** (reminder, then payment confirmation, then resolved). Ailon 2013 links templates causally and White 2019 detects task completion from assistant logs; neither evaluates bill or request resolution in email.
- **LLM performance on any of the above, especially local models.** No calibrated, per-class evaluation of LLM request or commitment detection in personal email was found for 2022–2026.
- **Per-user adaptation of the request and pleasantry boundary.** Lampert says it is needed; Di Castro shows personalisation for behaviour prediction only. No method was evaluated for obligation detection.
