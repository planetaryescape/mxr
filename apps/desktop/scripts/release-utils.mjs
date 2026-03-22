import { chmod, copyFile, mkdir, readFile, writeFile } from "node:fs/promises";
import { join, resolve } from "node:path";

export function parseWorkspaceVersion(cargoToml) {
  const workspacePackageMatch = cargoToml.match(
    /\[workspace\.package\][\s\S]*?^\s*version\s*=\s*"([^"]+)"/m,
  );
  if (!workspacePackageMatch) {
    throw new Error("Could not find [workspace.package] version in Cargo.toml");
  }
  return workspacePackageMatch[1];
}

export async function syncDesktopVersion({
  cargoTomlPath,
  packageJsonPath,
  packageLockPath,
  check = false,
}) {
  const version = parseWorkspaceVersion(await readFile(cargoTomlPath, "utf8"));
  const packageJson = JSON.parse(await readFile(packageJsonPath, "utf8"));
  const packageLock = JSON.parse(await readFile(packageLockPath, "utf8"));

  let changed = false;

  if (packageJson.version !== version) {
    packageJson.version = version;
    changed = true;
  }

  if (packageLock.version !== version) {
    packageLock.version = version;
    changed = true;
  }

  if (packageLock.packages?.[""]?.version !== version) {
    packageLock.packages = packageLock.packages ?? {};
    packageLock.packages[""] = packageLock.packages[""] ?? {};
    packageLock.packages[""].version = version;
    changed = true;
  }

  if (changed && check) {
    throw new Error(`apps/desktop package version is out of sync with Cargo.toml (${version})`);
  }

  if (changed && !check) {
    await writeFile(packageJsonPath, `${JSON.stringify(packageJson, null, 2)}\n`);
    await writeFile(packageLockPath, `${JSON.stringify(packageLock, null, 2)}\n`);
  }

  return { changed, version };
}

export async function stageBundledBinary({ sourcePath, resourcesDir }) {
  const resolvedSourcePath = resolve(sourcePath);
  const targetPath = join(resolve(resourcesDir), "bin", "mxr");
  await mkdir(join(resolve(resourcesDir), "bin"), { recursive: true });
  await copyFile(resolvedSourcePath, targetPath);
  await chmod(targetPath, 0o755);
  return targetPath;
}
