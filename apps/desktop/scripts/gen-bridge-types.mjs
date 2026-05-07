#!/usr/bin/env node
//
// Generate TypeScript types from the bridge's OpenAPI 3.1 spec.
//
// Source of the spec: `cargo run --example dump_openapi_spec -p mxr-web`
// (see slice 7). This script:
//   1. Builds + runs that example to capture the latest spec
//   2. Pipes it through `openapi-typescript` to produce
//      apps/desktop/src/shared/api.generated.ts
//
// Run after touching crates/protocol or crates/web routes:
//   pnpm --filter mxr-desktop gen:types
//
// Generated output is committed so packaged builds don't depend on a
// Rust toolchain at install time.

import { spawnSync } from "node:child_process";
import { mkdirSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(here, "../../..");
const target = resolve(here, "..", "src/shared/api.generated.ts");

console.log("[gen:types] dumping OpenAPI spec via cargo example…");
const dump = spawnSync(
  "cargo",
  ["run", "--quiet", "--example", "dump_openapi_spec", "-p", "mxr-web"],
  { cwd: repoRoot, encoding: "utf8", stdio: ["ignore", "pipe", "inherit"] },
);
if (dump.status !== 0) {
  console.error("[gen:types] cargo run failed");
  process.exit(dump.status ?? 1);
}

mkdirSync(dirname(target), { recursive: true });

const tmpSpec = resolve(here, "..", ".openapi-spec.json");
writeFileSync(tmpSpec, dump.stdout);

console.log("[gen:types] running openapi-typescript…");
const gen = spawnSync(
  "npx",
  ["--yes", "openapi-typescript", tmpSpec, "--output", target],
  { cwd: resolve(here, ".."), stdio: "inherit" },
);
if (gen.status !== 0) {
  console.error("[gen:types] openapi-typescript failed");
  process.exit(gen.status ?? 1);
}

console.log(`[gen:types] wrote ${target}`);
