import { readFileSync } from "node:fs";
import { fileURLToPath, URL } from "node:url";

export function mxrVersion(): string {
  const path = fileURLToPath(new URL("../../../Cargo.toml", import.meta.url));
  const manifest = readFileSync(path, "utf8");
  const version = /^\[workspace\.package\][^[]*?^version\s*=\s*"([^"]+)"/m.exec(manifest)?.[1];
  if (!version) throw new Error(`No [workspace.package] version in ${path}`);
  return version;
}
