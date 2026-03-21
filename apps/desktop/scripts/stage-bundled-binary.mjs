import { existsSync } from "node:fs";
import { resolve } from "node:path";
import { stageBundledBinary } from "./release-utils.mjs";

const sourcePath =
  process.env.MXR_DESKTOP_BUNDLED_BINARY?.trim() || resolve("../../target/release/mxr");

if (!existsSync(sourcePath)) {
  console.error(`Bundled mxr binary not found at ${sourcePath}`);
  process.exit(1);
}

const stagedPath = await stageBundledBinary({
  sourcePath,
  resourcesDir: resolve("./resources"),
});

console.log(stagedPath);
