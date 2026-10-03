#!/usr/bin/env node
// npm audit has no way to accept a single advisory, so a gate that must stay
// strict for everything else needs a small allowlist. Each entry names the
// advisory, why it does not apply here, and a date after which it fails again.
import { execFileSync } from "node:child_process";
import { readFileSync, existsSync } from "node:fs";

const allowlistPath = process.argv[2] ?? "audit-allowlist.json";
const allow = existsSync(allowlistPath) ? JSON.parse(readFileSync(allowlistPath, "utf8")) : [];
const today = new Date().toISOString().slice(0, 10);
const active = new Map(allow.filter((a) => a.until >= today).map((a) => [a.advisory, a]));

let raw;
try {
  raw = execFileSync("npm", ["audit", "--json"], { encoding: "utf8", stdio: ["ignore", "pipe", "inherit"] });
} catch (err) {
  raw = err.stdout; // npm audit exits non-zero when it finds anything
}
const report = JSON.parse(raw);
const levels = ["moderate", "high", "critical"];
const failing = [];
for (const [name, vuln] of Object.entries(report.vulnerabilities ?? {})) {
  for (const via of vuln.via) {
    if (typeof via === "string" || !levels.includes(via.severity)) continue;
    const id = via.url?.split("/").pop();
    if (active.has(id)) {
      console.log(`allowed until ${active.get(id).until}: ${id} (${name}) ${active.get(id).reason}`);
    } else {
      failing.push(`${via.severity} ${id} in ${name}: ${via.title}`);
    }
  }
}
for (const a of allow.filter((a) => a.until < today)) console.log(`expired allowlist entry: ${a.advisory}`);
if (failing.length) {
  console.error(failing.join("\n"));
  process.exit(1);
}
console.log("npm audit: no moderate or higher advisories outside the allowlist");
