# LLM tracing follow-ups

Left open by tracing model calls to a local Langfuse (D149).

- **Streamed replies from OpenAI-compatible servers carry no token usage.**
  The assistant's `chat` generations on Ollama show no usage: those servers
  report usage on a stream only when the request asks with
  `stream_options: {"include_usage": true}`, and mxr doesn't send it.
  Adding it changes the request body for every OpenAI-compatible server, so
  check which of Ollama, LM Studio, OpenAI, Groq and OpenRouter accept it
  first. Anthropic's streams already report usage.
- **Thinking from OpenAI-compatible streams isn't kept.** `OpenAiStream`
  folds only text and tool calls, so a streamed reply's `reasoning` deltas
  don't reach the trace. Non-streamed completions and Claude keep it.
- **A tool span doesn't name the call it answers.** `ToolHost::call` gets
  the tool name and arguments, not the call id, so the `tool` observation
  can't be matched to its `tool_calls` entry except by order. The next
  generation's input does pair them by `tool_call_id`.
- **Tracing state is only in the log.** Whether the daemon traces, and
  where, shows as one line in `mxr logs`; `mxr status` and `mxr doctor` don't
  report it.
