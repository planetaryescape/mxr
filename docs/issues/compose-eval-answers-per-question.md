# The answers grader matches a phrase anywhere in the draft, not to its question

**Status:** open · **Found:** cross-model review of the drafting-eval rung
(round 2, `f2d564e89`) · **Severity:** should-fix · **Not fixed now** by
decision: the grader stays as it is until this is scheduled.

## What happens

`graders::answers` in `crates/composer/src/check/graders.rs` passes a case's
answer when any of its phrases appears as a whole word anywhere in the draft.
Each phrase is shared across questions. The `call-empty` case asks two
questions, "is 3pm fine" and "will Priya join", and the phrase list for the
second includes `not`, `no`, `out` and `can't`. A `not` that answers the time
satisfies the Priya question.

## Repro

Replay `call-empty` (`crates/provider-fake/src/fixtures/compose/pairs_b.rs`)
with the draft:

    Hi Sam,

    3pm does not work. Priya will join.

    Alex

Every check passes, although the draft says the time is wrong and Priya will
attend. The grader reports `answers` as passing because `not` matches the
time sentence.

The converse also happens: "3pm works for me. Priya cannot make it." fails
the `answers` check, because "cannot" is not in the phrase list, even though
it is the right answer.

## The invariant a fix must keep

Each answer is bound to its own question and its polarity. The phrase for
"will Priya join" must be found in the sentence that answers that question
(the sentence with the question's subject, or the one after the question),
and a "no" phrase counts only with that polarity. Whole-word matching stays:
`now` never matches `no`.

Acceptance for the fix:

- the `call-empty` repro above fails `answers`;
- "3pm works for me. Priya cannot make it." passes `answers` once "cannot"
  is recognised for that question;
- the existing `an_answer_is_a_whole_word_so_now_does_not_answer_no` test
  still passes.

## Not in scope here

Deciding how a sentence is matched to a question (by subject words, by
position, or by a model) is a grader design choice. It belongs with the
rung that moves answers to a stronger check.
