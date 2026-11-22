#!/usr/bin/env node

import { readdirSync, readFileSync } from "node:fs";
import { join, relative } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const BANNED = /^(?:Lucide)?(?:Sparkles?|WandSparkles|BotMessageSquare)(?:Icon)?$/;
const IMPORT = /import\s+(?:type\s+)?\{([^}]*)\}\s+from\s+["']lucide-react(?:\/[^"']*)?["']/g;

const lineOf = (text, index) => text.slice(0, index).split("\n").length;

export function checkScript(text) {
  const findings = [];
  for (const match of text.matchAll(IMPORT)) {
    for (const specifier of match[1].split(",")) {
      const name = specifier
        .trim()
        .replace(/^type\s+/, "")
        .split(/\s+as\s+/)[0];
      if (BANNED.test(name)) {
        findings.push({
          line: lineOf(text, match.index + match[0].indexOf(name)),
          message: `${name}: no icon stands for AI (D141). Name the feature for what it does and use a task icon (PenLine to draft, TextQuote to summarize).`,
        });
      }
    }
  }
  for (const match of text.matchAll(/✨/g)) {
    findings.push({
      line: lineOf(text, match.index),
      message: "sparkle emoji: no icon stands for AI (D141). Use words or a task icon.",
    });
  }
  return findings;
}

function* files(dir) {
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const path = join(dir, entry.name);
    if (entry.isDirectory()) yield* files(path);
    else yield path;
  }
}

function main() {
  const root = fileURLToPath(new URL("..", import.meta.url));
  const findings = [];
  for (const path of files(join(root, "src"))) {
    if (!/\.tsx?$/.test(path) || path.endsWith("routeTree.gen.ts")) continue;
    for (const { line, message } of checkScript(readFileSync(path, "utf8"))) {
      findings.push(`${relative(root, path)}:${line}: ${message}`);
    }
  }
  if (findings.length > 0) {
    console.error(`No icon stands for AI (D141):\n${findings.join("\n")}`);
    process.exit(1);
  }
}

if (import.meta.url === pathToFileURL(process.argv[1] ?? "").href) main();
