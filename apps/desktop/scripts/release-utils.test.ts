import { mkdtemp, readFile, stat, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, describe, expect, it } from "vitest";
import {
  stageBundledBinary,
  syncDesktopVersion,
} from "./release-utils.mjs";

describe("release utils", () => {
  const tempDirs: string[] = [];

  afterEach(async () => {
    await Promise.all(
      tempDirs.map(async (dir) => {
        await import("node:fs/promises").then(({ rm }) =>
          rm(dir, { recursive: true, force: true }),
        );
      }),
    );
    tempDirs.length = 0;
  });

  it("syncs desktop package metadata to workspace version", async () => {
    const root = await mkdtemp(join(tmpdir(), "mxr-desktop-release-"));
    tempDirs.push(root);
    const cargoTomlPath = join(root, "Cargo.toml");
    const packageJsonPath = join(root, "package.json");
    const packageLockPath = join(root, "package-lock.json");

    await writeFile(
      cargoTomlPath,
      ['[workspace.package]', 'version = "1.2.3"'].join("\n"),
    );
    await writeFile(
      packageJsonPath,
      JSON.stringify({ name: "mxr-desktop", version: "0.0.1" }, null, 2),
    );
    await writeFile(
      packageLockPath,
      JSON.stringify(
        {
          name: "mxr-desktop",
          version: "0.0.1",
          packages: { "": { name: "mxr-desktop", version: "0.0.1" } },
        },
        null,
        2,
      ),
    );

    const result = await syncDesktopVersion({
      cargoTomlPath,
      packageJsonPath,
      packageLockPath,
    });

    expect(result.version).toBe("1.2.3");
    expect(result.changed).toBe(true);
    expect(JSON.parse(await readFile(packageJsonPath, "utf8")).version).toBe("1.2.3");
    const lockfile = JSON.parse(await readFile(packageLockPath, "utf8"));
    expect(lockfile.version).toBe("1.2.3");
    expect(lockfile.packages[""].version).toBe("1.2.3");
  });

  it("stages a bundled mxr binary into resources/bin", async () => {
    const root = await mkdtemp(join(tmpdir(), "mxr-desktop-bundle-"));
    tempDirs.push(root);
    const sourcePath = join(root, "mxr");
    const resourcesDir = join(root, "resources");

    await writeFile(sourcePath, "#!/bin/sh\necho mxr\n");

    const stagedPath = await stageBundledBinary({
      sourcePath,
      resourcesDir,
    });

    expect(stagedPath).toBe(join(resourcesDir, "bin", "mxr"));
    expect(await readFile(stagedPath, "utf8")).toContain("echo mxr");
    expect((await stat(stagedPath)).mode & 0o111).not.toBe(0);
  });
});
