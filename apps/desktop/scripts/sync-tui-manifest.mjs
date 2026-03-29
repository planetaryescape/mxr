import { execFileSync } from "node:child_process";
import { mkdir, writeFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(__dirname, "../../..");
const outputPath = resolve(repoRoot, "apps/desktop/src/shared/generated/tui-manifest.ts");

const manifestJson = execFileSync(
  "cargo",
  ["run", "-q", "-p", "mxr", "--example", "export_desktop_manifest"],
  {
    cwd: repoRoot,
    encoding: "utf8",
  },
);

const fileContents = `export const tuiDesktopManifest = ${manifestJson.trim()} as const;\n`;

await mkdir(dirname(outputPath), { recursive: true });
await writeFile(outputPath, fileContents);
