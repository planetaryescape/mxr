#!/usr/bin/env node
//
// Generate TypeScript types from the bridge OpenAPI 3.1 spec.
//
// Spec source: the mxr-web dump_openapi_spec example, built through the queue.
// Output is committed so the SPA build does not need a Rust toolchain.
//
// Run after touching crates/protocol or crates/web routes:
//   npm run gen:types

import { spawnSync } from "node:child_process";
import { mkdirSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(here, "../../..");
const target = resolve(here, "..", "src/api/generated.ts");

console.info("[gen:types] building OpenAPI spec exporter…");
const build = spawnSync(
  "cargo",
  [
    "build",
    "--quiet",
    "--example",
    "dump_openapi_spec",
    "-p",
    "mxr-web",
    "--target-dir",
    "target-cli",
  ],
  { cwd: repoRoot, encoding: "utf8", stdio: ["ignore", "pipe", "inherit"] },
);
if (build.status !== 0) {
  console.error("[gen:types] cargo build failed");
  process.exit(build.status ?? 1);
}

const dump = spawnSync(resolve(repoRoot, "target-cli/debug/examples/dump_openapi_spec"), [], {
  cwd: repoRoot,
  encoding: "utf8",
  stdio: ["ignore", "pipe", "inherit"],
  // The spec is past spawnSync's 1 MiB default.
  maxBuffer: 64 * 1024 * 1024,
});
if (dump.status !== 0) {
  console.error(
    `[gen:types] dumping the spec failed: ${dump.error?.message ?? `exit ${dump.status}`}`,
  );
  process.exit(dump.status ?? 1);
}

mkdirSync(dirname(target), { recursive: true });

const tmpSpec = resolve(here, "..", ".openapi-spec.tmp.json");
writeFileSync(tmpSpec, dump.stdout);

console.info("[gen:types] running openapi-typescript…");
const gen = spawnSync("npx", ["--yes", "openapi-typescript", tmpSpec, "--output", target], {
  cwd: resolve(here, ".."),
  stdio: "inherit",
});
if (gen.status !== 0) {
  console.error("[gen:types] openapi-typescript failed");
  process.exit(gen.status ?? 1);
}

// Preserve the generated file's existing four-space formatting, including unions.
const format = spawnSync("npx", ["--no-install", "prettier", "--tab-width", "4", "--write", target], {
  cwd: resolve(here, ".."),
  stdio: "inherit",
});
if (format.status !== 0) process.exit(format.status ?? 1);

console.info(`[gen:types] wrote ${target}`);
