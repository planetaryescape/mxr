import { createOpenAICompatible } from "@ai-sdk/openai-compatible";
import { defineAgent } from "eve";

const local = createOpenAICompatible({
  name: "ollama",
  baseURL: process.env.SPIKE_MODEL_URL ?? "http://127.0.0.1:11434/v1",
  includeUsage: true,
});

export default defineAgent({
  model: local(process.env.SPIKE_MODEL ?? "gemma4:latest"),
  modelContextWindowTokens: 16_384,
  tool: false,
  defaultTools: false,
  experimental: { workflow: { retention: 0 } },
});
