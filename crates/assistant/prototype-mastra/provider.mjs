// Same scripted OpenAI-compatible provider as the Eve experiment.
import assert from "node:assert/strict";
import { createServer } from "node:http";
import { writeFileSync } from "node:fs";
import { resolve } from "node:path";

export async function scriptedProvider(evidence) {
  let sequence = 0;
  const server = createServer(async (req, res) => {
    try {
      let body = "";
      for await (const part of req) body += part;
      const request = JSON.parse(body);
      writeFileSync(resolve(evidence, `model-${++sequence}.json`), JSON.stringify(request, null, 2));
      const calls = request.messages.flatMap((m) => m.tool_calls ?? []);
      const toolResults = request.messages.filter((m) => m.role === "tool");
      const user = request.messages.filter((m) => m.role === "user").map((m) => JSON.stringify(m.content)).join(" ");
      let name, args;
      const content = "Completed synthetic mailbox operation.";
      if (user.includes("[scenario:send]")) {
        const succeeded = toolResults.some((m) => m.content?.includes('"sent":true'));
        if (!succeeded) {
          name = "mail_send";
          args = { operation_id: "send-once", to: "recipient@example.invalid", body: "Synthetic hello" };
        }
      } else if (!calls.some((c) => c.function.name === "mail_search")) {
        name = "mail_search";
        args = { query: "shipping" };
      } else if (!user.includes("[scenario:read]")) {
        const archives = calls.filter((c) => c.function.name === "mail_archive");
        if (archives.length === 0) {
          name = "mail_archive";
          args = { ids: ["shipping-1", "shipping-2"], dry_run: true };
        } else if (archives.length === 1) {
          const preview = toolResults.map((m) => {
            try { return JSON.parse(m.content); } catch { return null; }
          }).find((r) => r?.preview_token);
          assert(preview, "Actual tool must provide preview token");
          name = "mail_archive";
          args = { ids: preview.ids, dry_run: false, preview_token: preview.preview_token };
        }
      }
      res.writeHead(200, { "content-type": "text/event-stream" });
      const chunk = (delta, finish_reason = null) => res.write(`data: ${JSON.stringify({ id: `chatcmpl-${sequence}`, object: "chat.completion.chunk", created: Math.floor(Date.now() / 1000), model: "scripted-fixture", choices: [{ index: 0, delta, finish_reason }] })}\n\n`);
      const tool_calls = name ? [{ index: 0, id: `call-${sequence}`, type: "function", function: { name, arguments: JSON.stringify(args) } }] : undefined;
      chunk(name ? { role: "assistant", tool_calls } : { role: "assistant", content });
      chunk({}, name ? "tool_calls" : "stop");
      res.end("data: [DONE]\n\n");
    } catch (error) {
      console.error(error);
      if (!res.headersSent) res.writeHead(500);
      res.end(String(error));
    }
  });
  await new Promise((r) => server.listen(0, "127.0.0.1", r));
  return server;
}
