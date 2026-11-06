# Messages should make the conversation and its audience obvious

Design Messages around opening a conversation, catching up and replying.
BK's requirement is a messaging app: people, what they said, and who receives
the next message. Addresses and email roles help establish those facts.
They should be easy to inspect without becoming the main reading surface.

Research date: 23 October 2026. Code evidence is freshly fetched
`origin/main`, commit `f39ae66578e3099e464967ab2f042e24b1ad8ec4`.
The working checkout remains at `3b61579a3f34203bdacb9954612b05401fa2e25c`;
all implementation findings below use the fetched ref. Product code is unchanged.
The screenshot is evidence of the reported interaction, not proof of which
daemon conditions produced its particular results.

## Reading and replying should need less interpretation

The screenshot gives the topic index, relationship statistics and repeated
“quiet” labels substantial space. The selected message shows a first name,
time and “Good afternoon”, with the rest behind “Read all 4 paragraphs”.
BK cannot identify the addresses or distinguish the audience of each message.

Prioritize these tasks:

1. Recognize the person or group and resolve an ambiguous identity.
2. Read the recent exchange and distinguish new messages from older history.
3. See when a message involved a different audience.
4. Reply with confidence about who receives the message and which account sends it.
5. Return to a different conversation with the same person without losing a draft.

The proposed direction keeps the person as the entry point, places the selected
conversation in the main reading area, and makes the other conversations a
compact switcher. Tapping a name reveals its address. Message info answers who
sent that particular message and its recorded To/Cc/Bcc. Beside the writing area,
“Replying to Maya and Jo” exposes the actual next audience and opens its controls.
These are design proposals, not shipped behavior or usability-study results.

Move relationship counts and inferred closeness into person information.
Use the reading surface for the exchange itself. The same discipline applies
to “Nobody is waiting on you”: absence of a qualifying turn is weaker evidence
than knowledge of another person's intentions. Any revised copy must respect
that distinction.

## “Quiet” is a fallback state, not a fact about somebody's intentions

The daemon chooses topic state in this order: a shared desk turn produces
`your_turn`; a covering dismissal produces `done`; otherwise an outbound
latest message produces `waiting`; everything else becomes `quiet`.
The web app appends age from `last_at`, the selected latest message's date.
Thus “quiet · 1h” combines a fallback classification with message age.
It does not establish that the message was read, answered or needs no reply.
See [state construction](https://github.com/planetaryescape/mxr/blob/f39ae66578e3099e464967ab2f042e24b1ad8ec4/crates/daemon/src/handler/messages.rs#L331-L412)
and [label formatting](https://github.com/planetaryescape/mxr/blob/f39ae66578e3099e464967ab2f042e24b1ad8ec4/apps/web/src/features/messages/messagesView.ts#L99-L108).

A person's Quiet band is a separate concept. All topics done, or no recent
non-done topic and no waiting topic, can put a row there. A recent quiet topic
can sit under a Recent person. Unread is computed independently.
See [band selection](https://github.com/planetaryescape/mxr/blob/f39ae66578e3099e464967ab2f042e24b1ad8ec4/crates/daemon/src/handler/messages_view.rs#L194-L217)
and [unread computation](https://github.com/planetaryescape/mxr/blob/f39ae66578e3099e464967ab2f042e24b1ad8ec4/crates/daemon/src/handler/messages.rs#L389-L408).

Remove the ordinary topic's “quiet” label from the proposed reading experience.
Activity time can stand alone. Preserve useful explicit states with a clear
basis; do not translate the fallback into “No reply needed”, “Muted” or “Done”.
Keep the row-band redesign separate until its grouping behavior is assessed.

## Person identity, message audience and reply audience are different facts

`PersonRefData` already includes a primary address, a name and explicitly linked
addresses. Each conversation message includes the original sender's full
`Address`. The UI renders “You” or the sender's first whitespace-delimited name.
It does not provide a contact disclosure.
See [person identity](https://github.com/planetaryescape/mxr/blob/f39ae66578e3099e464967ab2f042e24b1ad8ec4/crates/protocol/src/types/messages.rs#L176-L207),
[message identity rendering](https://github.com/planetaryescape/mxr/blob/f39ae66578e3099e464967ab2f042e24b1ad8ec4/apps/web/src/features/messages/Conversation.tsx#L51-L81)
and [person header](https://github.com/planetaryescape/mxr/blob/f39ae66578e3099e464967ab2f042e24b1ad8ec4/apps/web/src/features/messages/PersonPane.tsx#L57-L89).

The conversation's existing member list is unsuitable as a complete audience.
Shape computation counts human inbound authors and the owner's outbound
recipients. An inbound message copying someone else can remain one-to-one;
there is an explicit test for this.
See [shape rules](https://github.com/planetaryescape/mxr/blob/f39ae66578e3099e464967ab2f042e24b1ad8ec4/crates/daemon/src/handler/conversation_shape.rs#L98-L155)
and [copied-recipient test](https://github.com/planetaryescape/mxr/blob/f39ae66578e3099e464967ab2f042e24b1ad8ec4/crates/daemon/src/handler/conversation_shape.rs#L253-L267).

Recorded To/Cc/Bcc are present upstream, but the Messages projection omits them.
Its constructor copies sender identity, date and message content into a DTO
without recipient fields.
See [Envelope](https://github.com/planetaryescape/mxr/blob/f39ae66578e3099e464967ab2f042e24b1ad8ec4/crates/core/src/types.rs#L883-L898),
[projection constructor](https://github.com/planetaryescape/mxr/blob/f39ae66578e3099e464967ab2f042e24b1ad8ec4/crates/daemon/src/handler/messages.rs#L1059-L1120)
and [Messages DTO](https://github.com/planetaryescape/mxr/blob/f39ae66578e3099e464967ab2f042e24b1ad8ec4/crates/protocol/src/types/messages.rs#L454-L488).

Reply preparation already resolves recipients from the target message,
Reply-To, the account's owned addresses and the reply-all choice. Original
Bcc recipients are not an input to these reply rules. Messages' display label
and group-derived reply-all default are not a resolved recipient list.
Use the prepared draft to display the next audience.
See [reply recipient rules](https://github.com/planetaryescape/mxr/blob/f39ae66578e3099e464967ab2f042e24b1ad8ec4/crates/daemon/src/handler/reply_recipients.rs#L17-L78),
[Messages composer defaults](https://github.com/planetaryescape/mxr/blob/f39ae66578e3099e464967ab2f042e24b1ad8ec4/crates/daemon/src/handler/messages.rs#L1123-L1142)
and [opening the reply](https://github.com/planetaryescape/mxr/blob/f39ae66578e3099e464967ab2f042e24b1ad8ec4/apps/web/src/features/messages/MessagesRoute.tsx#L421-L428).

The general reader already exposes addresses and an expandable recipient list.
Messages' “as sent” component displays the body rather than that reader header.
Reuse the underlying facts without importing the reader's entire layout.
See [reader audience disclosure](https://github.com/planetaryescape/mxr/blob/f39ae66578e3099e464967ab2f042e24b1ad8ec4/apps/web/src/features/thread/MessageCard.tsx#L447-L498)
and [Messages as-sent rendering](https://github.com/planetaryescape/mxr/blob/f39ae66578e3099e464967ab2f042e24b1ad8ec4/apps/web/src/features/messages/Conversation.tsx#L164-L204).

## Messaging precedents support identity on demand and explicit routing

Beeper combines a person's direct chats into one timeline, retains network
context and lets the user select the underlying sending chat in the composer.
It documents compromises in merged read state. Adopt the explicit routing
principle; defer a merged timeline across email topics because audience and
reply context need a separate design.
[Beeper merged chats](https://help.beeper.com/beeper-plus/merge-chats-getting-started-guide).

Apple Messages exposes conversation details through the contact identity.
It distinguishes an inline reply visible to the group from messaging one
member privately. Signal exposes contextual identity information and flags
duplicate names in groups. Adopt tappable identities and disambiguate where
names alone are insufficient.
[Apple Messages](https://support.apple.com/en-gb/guide/messages/icht35827/mac),
[Signal identities](https://support.signal.org/hc/en-us/articles/6829998083994-Phone-Number-Privacy-and-Usernames-Deeper-Dive).

Slack creates a new group DM when people are added, with explicit control
over included history and notifications in the relevant conversations.
Its thread design history also describes how showing every linked reply in
the channel increased noise. Preserve conversation boundaries and communicate
audience changes. In email, say “Maya copied Jo on this message” when supported;
do not infer permanent membership by saying “Jo joined”.
[Slack DM audience changes](https://slack.com/help/articles/1500002969782-Add-people-to-a-direct-message),
[Slack thread design](https://slack.engineering/weaving-threads/).

Apple Mail provides direct access to the address behind a name. The useful
pattern is the direct disclosure, rather than permanently exposing every
transport field.
[Apple Mail Smart Addresses](https://support.apple.com/en-mide/guide/mail/use-smart-addresses-mlhl0257dcd6/16.0/mac/26).

The general usability basis is recognition, clear system state and prevention
of consequential mistakes. These principles guide the proposal; vendor
documentation does not prove that this particular design is usable.
[NN/g usability heuristics](https://www.nngroup.com/articles/ten-usability-heuristics/).

## The default view can be simple while message information stays precise

Keep names, readable messages, activity times, a new-message boundary where
supported, and the next reply's audience in the main flow. Make the person
header, author names and conversation people directly inspectable. Ambiguous
or unfamiliar identities should reveal a distinguishing address immediately.
The contact card should identify the address used on the selected message
separately from other linked addresses.

Give message information one direct entry point. It can expose exact sender,
recorded recipients and roles, the sending account and exact time. A list of
people seen across the conversation is historical context, not a claim that
each person received every message. A reply audience change must remain
visible beside the draft.

The protocol permits delivery copies with missing or different Bcc information.
An empty Bcc list does not establish that there were no hidden recipients.
Avoid asserting “You were Bcc'd” merely because the owner's address is absent
from To/Cc. Show available saved outgoing Bcc in message info.
[RFC 5322 §3.6.3](https://www.rfc-editor.org/rfc/rfc5322.html#section-3.6.3).

“Got it” currently sends a reply rather than adding a reaction or marking Done.
Any proposed quick acknowledgement must show that consequence and its
recipients before sending. Do not draw reaction-style affordances that imply
semantics the transport cannot provide.
[Acknowledgement send path](https://github.com/planetaryescape/mxr/blob/f39ae66578e3099e464967ab2f042e24b1ad8ec4/crates/daemon/src/handler/messages_ack.rs#L225-L285).

## Verify the interaction through communication tasks

The user-visible completion check for implementation is that BK can identify a
person, inspect an individual message's recorded audience, read the exchange
and name the next reply's recipients without leaving the conversation.

- Two people share a display name: identify and copy the intended address.
- A person uses two linked addresses: see the address used on this message.
- Someone is copied midway: identify which messages include them.
- Reply-To differs from From: see the actual destination before sending.
- Switch from a group reply to a single recipient: verify the prepared draft
  and the visible audience agree.
- Change conversations and return: preserve the draft and its chosen audience.
- Read a long message beginning with a greeting: show substantive text before
  requiring expansion; retain the existing option to fold long messages.
- Inspect a received copy without Bcc data: distinguish unavailable information
  from confirmed absence.
- Use keyboard and touch: open identity and message info without hover, dismiss
  disclosures and restore focus, and keep long addresses readable on a phone.

## Continue with the interaction, then the shared projection

Three parallel research tasks covered code, primary-source product patterns and
an independent UX critique. Their useful claims were checked against the fetched
code and primary sources. The recommendations remain proposals.

An interaction study with fictional data lives at
`/var/folders/3p/n76j7jp14ljcvr1bhf7qmlh00000gn/T/agent-htmlify/20261023-162700-messages-people-and-audience.html`.
It demonstrates identity disclosure, message-specific audiences, conversation
switching and a reply-audience choice. It is a temporary companion; this note
owns the reasoning. Eleven DOM interaction/layout checks passed at 728px and
375px using T3's HTML renderer. The collaborative browser's snapshot timed out
twice. Those checks establish prototype behavior, not human usability.

Next action: review the interaction with BK, then implement the agreed surface
from a fresh `origin/main`. Start with identity disclosure and compact
conversation navigation; add the message audience to the shared daemon/protocol
projection and generate the web types; derive the composer audience from shared
reply preparation. Preserve client parity for any new daemon capability.
Use focused Vitest tests for presentation and scoped Rust tests for projection
changes. No application code changed and no application builds or tests ran
during this research.

Unresolved questions:

- Does compact conversation navigation preserve enough overview for BK's real
  message history? The temporary study demonstrates two conversations only.
- Should a future person history interleave private topics chronologically?
  Defer that model change; it must retain explicit topic and audience boundaries.
- What should replace the separate Quiet person band? Removing the repeated
  topic label does not settle that grouping decision.
