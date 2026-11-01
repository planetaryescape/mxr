import assert from "node:assert/strict";
import { MCPClient } from "@mastra/mcp";
import { mkdirSync, mkdtempSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";

mkdirSync("evidence", { recursive: true });
const evidence = mkdtempSync(resolve("evidence", "mcp-"));
process.env.SPIKE_STATE = resolve(evidence, "fixture");
const fixture = await import("./fixture.ts");
fixture.save(fixture.initial());
const client = new MCPClient({
  id: "synthetic-stdio-probe",
  servers: {
    mail: {
      command: process.execPath,
      args: [resolve("mcp-fixture.mjs")],
      env: { SPIKE_STATE: fixture.root, MASTRA_TELEMETRY_DISABLED: "1" },
    },
  },
  timeout: 5_000,
});
try {
  const tools = await client.listTools();
  assert.deepEqual(Object.keys(tools), ["mail_mail_search"]);
  const result = await tools.mail_mail_search.execute({ query: "shipping" }, {});
  const text = JSON.stringify(result);
  assert(text.includes("shipping-1") && text.includes("shipping-2"));
  assert.equal(fixture.load().archiveExecutions.length, 0);
  const observation = { passed: true, transport: "stdio", serverProtocol: "2025-06-18", tools: Object.keys(tools), result, evidence };
  writeFileSync(resolve(evidence, "results.json"), JSON.stringify(observation, null, 2));
  console.log(JSON.stringify(observation));
} finally {
  await client.disconnect();
}
