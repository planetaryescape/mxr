# Sign in with ChatGPT is parked until OpenAI publishes data terms for plan usage

**Status:** blocked on OpenAI · **Found:** 2026-10-02 · **Research:**
[docs/research/chatgpt-sign-in.md](../research/chatgpt-sign-in.md)

OpenAI launched Sign in with ChatGPT at DevDay on 2026-09-29. Its "ChatGPT
plan usage" permission lets a local, open-source app send Responses API
requests billed to the user's Plus or Pro plan, with no API key. The OAuth
and daemon mechanics fit mxr. The data terms don't.

mxr sends third parties' mail only to a processor whose terms cover it.
For plan usage, OpenAI publishes no statement on training or retention and
offers no DPA for Plus or Pro, and its nearest published rule puts Codex on
a personal plan under the consumer "Improve the model for everyone"
setting. So the bar in `docs/blueprint/22-email-modes.md` ("adopted only
if its data terms cover third parties' mail the way API terms do") is not
met. The user's own API key is the supported cloud path.

It is also a poor fit for background classification even with terms: it
spends the user's Codex allowance (and on Plus a five-hour window shared
across apps), the route rejects `max_output_tokens` and `temperature`, and
a limit is a 429 with no reset time.

## Evidence

- Plan usage scope and eligibility:
  https://developers.openai.com/siwc/quickstart,
  https://help.openai.com/en/articles/20001542
- Open-source self-serve path:
  https://developers.openai.com/siwc/token-sharing-open-source
- Request limits and unsupported fields:
  https://developers.openai.com/siwc/token-sharing-open-source/models-and-inference
- Limit errors with no reset time:
  https://developers.openai.com/siwc/token-sharing-open-source/errors-and-recovery
- Codex on a personal plan follows consumer controls:
  https://help.openai.com/en/articles/7730893-data-controls-faq
- Consumer terms allow training on content:
  https://openai.com/policies/row-terms-of-use/
- DPA only for Business, Enterprise and the API:
  https://openai.com/enterprise-privacy/
- API data is not used for training by default:
  https://developers.openai.com/api/docs/guides/your-data

help.openai.com and openai.com pages were read through a reader proxy on
2026-10-02; check the quotes against the live pages before citing them
outside the repo.

## Revisit when

OpenAI states in writing that plan-usage requests fall under API-equivalent
terms (no training by default, a DPA or processor terms covering the
content). Then adopt it for on-demand features (draft, summarise, ask)
first, never background classification, behind a provider data-terms
attribute so the privacy gate can refuse unverified terms.
