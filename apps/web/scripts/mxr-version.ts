/*
 * The mxr version this web build ships in: the workspace `Cargo.toml`'s,
 * which release-please bumps with every release. Vite bakes it in as
 * `MXR_BUILD_VERSION`, so the web app expires moved-key notices at the same
 * version as the TUI built beside it (blueprint 35, D171).
 */

import { readFileSync } from "node:fs";
import { fileURLToPath, URL } from "node:url";

export function mxrVersion(): string {
  const path = fileURLToPath(new URL("../../../Cargo.toml", import.meta.url));
  const manifest = readFileSync(path, "utf8");
  const version = /^\[workspace\.package\][^[]*?^version\s*=\s*"([^"]+)"/m.exec(manifest)?.[1];
  if (!version) throw new Error(`No [workspace.package] version in ${path}`);
  return version;
}
