// Minimal synthetic JSON-RPC MCP server; exercises stdio without touching mxr.
import { createInterface } from "node:readline";
import { search } from "./fixture.ts";

for await (const line of createInterface({ input: process.stdin })) {
  const request = JSON.parse(line);
  if (request.id === undefined) continue;
  let result;
  if (request.method === "initialize") {
    result = { protocolVersion: "2025-06-18", capabilities: { tools: {} }, serverInfo: { name: "synthetic-mail", version: "0.0.0" } };
  } else if (request.method === "tools/list") {
    result = { tools: [{ name: "mail_search", description: "Search synthetic mailbox subjects", inputSchema: { type: "object", properties: { query: { type: "string" } }, required: ["query"], additionalProperties: false } }] };
  } else if (request.method === "tools/call" && request.params.name === "mail_search") {
    const data = search(request.params.arguments.query);
    result = { content: [{ type: "text", text: JSON.stringify(data) }], structuredContent: data };
  } else if (request.method === "ping") result = {};
  else {
    process.stdout.write(JSON.stringify({ jsonrpc: "2.0", id: request.id, error: { code: -32601, message: "Unknown method" } }) + "\n");
    continue;
  }
  process.stdout.write(JSON.stringify({ jsonrpc: "2.0", id: request.id, result }) + "\n");
}
