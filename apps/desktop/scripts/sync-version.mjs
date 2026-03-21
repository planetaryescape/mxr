import { resolve } from "node:path";
import { syncDesktopVersion } from "./release-utils.mjs";

const check = process.argv.includes("--check");

const result = await syncDesktopVersion({
  cargoTomlPath: resolve("../../Cargo.toml"),
  packageJsonPath: resolve("./package.json"),
  packageLockPath: resolve("./package-lock.json"),
  check,
});

if (check) {
  console.log(`apps/desktop version matches Cargo.toml (${result.version})`);
} else if (result.changed) {
  console.log(`synced apps/desktop version to ${result.version}`);
} else {
  console.log(`apps/desktop already at ${result.version}`);
}
