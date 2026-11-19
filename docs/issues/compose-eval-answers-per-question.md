# The answers grader reads words, so some answers about a subject are misgraded

**Status:** open · **Found:** review of the clause-bound answers check ·
**Severity:** minor

## What happens

`graders::answers` in `crates/composer/src/check/graders.rs` binds each
labelled answer to the clause that names its subject. A word-level check
cannot tell what a negation is about, or link an answer to a subject in
another sentence. The `call-empty` case asks "is 3pm fine" and "will Priya
join". The correct answers are yes and no.

## These drafts pass, but Priya is joining

The negation is about something else:

    3pm works. Priya will join, but she can't stay long.
    3pm works. Priya will join. She can't stay long.
    3pm works. Priya will sort it out and join.
    3pm works. Priya will not miss it.

## These drafts fail, but the answers are correct

A bare "no" in its own sentence is not bound to Priya, because it could answer
the 3pm question:

    3pm works. No. Priya is travelling.
    3pm works. Unfortunately, no. Priya is away.

A double negative reads as a refusal of 3pm:

    3pm is not bad. Priya is out.

The answer uses a word outside the case's phrase list:

    3pm works. Priya isn't joining.
    3pm works. Priya is unable to join.

## Acceptance for the fix

- every draft under "These drafts pass, but Priya is joining" fails `answers`;
- every draft under "These drafts fail, but the answers are correct" passes
  `answers`;
- every recording in `call-empty` keeps its current grade;
- the `an_answer_is_a_whole_word_so_now_does_not_answer_no` test still passes.

## Not in scope here

Moving the answers check to a model is a grader design choice. It belongs with
the rung that moves answers to a stronger check.
