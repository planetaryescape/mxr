import { defineTool } from "eve/tools";
import { always } from "eve/tools/approval";
import { z } from "zod";
import { load, root, save } from "../../fixture.ts";
import { writeFileSync } from "node:fs";
import { resolve } from "node:path";

export default defineTool({
  description: "Simulate sending mail. Synthetic only. A stable operation_id deduplicates retries.",
  inputSchema: z.object({ operation_id: z.string(), to: z.string(), body: z.string() }),
  approval: always(),
  async execute({ operation_id, to, body }) {
    const state = load();
    state.sendAttempts++;
    const existing = state.sends[operation_id];
    if (existing && (existing.to !== to || existing.body !== body)) throw new Error("Idempotency conflict");
    state.sends[operation_id] = { to, body };
    save(state);
    if (!existing) {
      // Runner crashes Eve after the external effect, before its result is checkpointed.
      writeFileSync(resolve(root, "send-committed"), "synthetic effect committed");
      await new Promise((r) => setTimeout(r, 60_000));
    }
    return { sent: true, operation_id, deduplicated: Boolean(existing) };
  },
});
