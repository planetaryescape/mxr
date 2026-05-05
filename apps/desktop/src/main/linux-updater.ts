import { createHash } from "node:crypto";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { basename, join } from "node:path";
import { shell } from "electron";
import type { DesktopUpdateState } from "../shared/types.js";

const RELEASES_LATEST_URL =
  "https://api.github.com/repos/planetaryescape/mxr/releases/latest";

type FetchLike = typeof fetch;

interface GitHubReleaseAsset {
  name: string;
  browser_download_url: string;
}

interface GitHubRelease {
  tag_name?: string;
  html_url?: string;
  assets?: GitHubReleaseAsset[];
}

interface ResolvedUpdate {
  version: string;
  currentVersion: string;
  asset: GitHubReleaseAsset;
  checksumAsset: GitHubReleaseAsset | null;
  releaseUrl: string;
}

export class LinuxUpdateManager {
  private readonly currentVersion: string;
  private readonly packaged: boolean;
  private readonly platform: NodeJS.Platform;
  private readonly arch: NodeJS.Architecture;
  private readonly fetchImpl: FetchLike;
  private lastUpdate: ResolvedUpdate | null = null;
  private downloaded: Extract<DesktopUpdateState, { status: "downloaded" }> | null =
    null;

  constructor(options: {
    currentVersion: string;
    packaged: boolean;
    platform?: NodeJS.Platform;
    arch?: NodeJS.Architecture;
    fetchImpl?: FetchLike;
  }) {
    this.currentVersion = stripVersionPrefix(options.currentVersion);
    this.packaged = options.packaged;
    this.platform = options.platform ?? process.platform;
    this.arch = options.arch ?? process.arch;
    this.fetchImpl = options.fetchImpl ?? fetch;
  }

  async check(): Promise<DesktopUpdateState> {
    this.downloaded = null;
    this.lastUpdate = null;

    if (this.platform !== "linux") {
      return {
        status: "unsupported",
        message: "Desktop update downloads are only needed on Linux.",
      };
    }
    if (!this.packaged) {
      return {
        status: "not-packaged",
        message: "Update checks run from packaged desktop builds.",
      };
    }

    const release = await this.fetchLatestRelease();
    const version = stripVersionPrefix(release.tag_name ?? "");
    if (!version) {
      return { status: "unavailable", message: "Latest release has no version tag." };
    }
    if (compareVersions(version, this.currentVersion) <= 0) {
      return {
        status: "up-to-date",
        message: `mxr Desktop ${this.currentVersion} is up to date.`,
      };
    }

    const asset = selectLinuxAsset(release.assets ?? [], version, this.arch);
    if (!asset) {
      return {
        status: "unavailable",
        message: `No Linux ${this.arch} desktop installer found for ${version}.`,
      };
    }

    const checksumAsset = selectChecksumAsset(release.assets ?? [], asset.name);
    this.lastUpdate = {
      version,
      currentVersion: this.currentVersion,
      asset,
      checksumAsset,
      releaseUrl: release.html_url ?? asset.browser_download_url,
    };

    return {
      status: "available",
      version,
      currentVersion: this.currentVersion,
      assetName: asset.name,
      releaseUrl: this.lastUpdate.releaseUrl,
      message: `mxr Desktop ${version} is available.`,
    };
  }

  async download(): Promise<DesktopUpdateState> {
    if (!this.lastUpdate) {
      const checked = await this.check();
      if (checked.status !== "available" || !this.lastUpdate) {
        return checked;
      }
    }

    const update = this.lastUpdate;
    if (!update.checksumAsset) {
      return {
        status: "unavailable",
        message: `No checksum found for ${update.asset.name}; refusing unverified download.`,
      };
    }

    const dir = join(tmpdir(), "mxr-desktop-updates", update.version);
    await mkdir(dir, { recursive: true });
    const path = join(dir, basename(update.asset.name));
    await downloadFile(this.fetchImpl, update.asset.browser_download_url, path);
    const expectedSha = await this.downloadExpectedChecksum(update);
    const actualSha = await sha256File(path);
    if (actualSha !== expectedSha) {
      throw new Error(
        `Downloaded installer checksum mismatch: expected ${expectedSha}, got ${actualSha}`,
      );
    }

    this.downloaded = {
      status: "downloaded",
      version: update.version,
      assetName: update.asset.name,
      path,
      sha256: actualSha,
      message: `Downloaded verified installer ${update.asset.name}.`,
    };
    return this.downloaded;
  }

  async openDownloaded(): Promise<void> {
    if (!this.downloaded) {
      const result = await this.download();
      if (result.status !== "downloaded") {
        throw new Error(result.message);
      }
    }
    const errorMessage = await shell.openPath(this.downloaded!.path);
    if (errorMessage) {
      throw new Error(errorMessage);
    }
  }

  private async fetchLatestRelease(): Promise<GitHubRelease> {
    const response = await this.fetchImpl(RELEASES_LATEST_URL, {
      headers: {
        accept: "application/vnd.github+json",
        "user-agent": "mxr-desktop-updater",
      },
    });
    if (!response.ok) {
      throw new Error(`GitHub release check failed with HTTP ${response.status}`);
    }
    return (await response.json()) as GitHubRelease;
  }

  private async downloadExpectedChecksum(update: ResolvedUpdate): Promise<string> {
    const response = await this.fetchImpl(update.checksumAsset!.browser_download_url, {
      headers: { "user-agent": "mxr-desktop-updater" },
    });
    if (!response.ok) {
      throw new Error(`Checksum download failed with HTTP ${response.status}`);
    }
    const text = await response.text();
    return parseChecksum(text, update.asset.name);
  }
}

function selectLinuxAsset(
  assets: GitHubReleaseAsset[],
  version: string,
  arch: NodeJS.Architecture,
): GitHubReleaseAsset | null {
  const releaseArch = arch === "x64" ? "x64" : arch;
  const preferredExts = preferredInstallerExtensions();
  for (const ext of preferredExts) {
    const expected = `mxr-v${version}-desktop-linux-${releaseArch}.${ext}`;
    const found = assets.find((asset) => asset.name === expected);
    if (found) {
      return found;
    }
  }
  return null;
}

function selectChecksumAsset(
  assets: GitHubReleaseAsset[],
  assetName: string,
): GitHubReleaseAsset | null {
  return (
    assets.find((asset) => asset.name === `${assetName}.sha256`) ??
    assets.find((asset) => asset.name === "SHA256SUMS") ??
    null
  );
}

function preferredInstallerExtensions(): string[] {
  if (process.env.MXR_DESKTOP_LINUX_INSTALLER === "rpm") {
    return ["rpm", "deb", "zip"];
  }
  if (process.env.MXR_DESKTOP_LINUX_INSTALLER === "zip") {
    return ["zip", "deb", "rpm"];
  }
  return ["deb", "rpm", "zip"];
}

async function downloadFile(
  fetchImpl: FetchLike,
  url: string,
  path: string,
): Promise<void> {
  const response = await fetchImpl(url, {
    headers: { "user-agent": "mxr-desktop-updater" },
  });
  if (!response.ok) {
    throw new Error(`Installer download failed with HTTP ${response.status}`);
  }
  await writeFile(path, Buffer.from(await response.arrayBuffer()));
}

async function sha256File(path: string): Promise<string> {
  const bytes = await readFile(path);
  return createHash("sha256").update(bytes).digest("hex");
}

function parseChecksum(text: string, assetName: string): string {
  for (const line of text.split(/\r?\n/)) {
    const [hash, name] = line.trim().split(/\s+/);
    if (!hash) {
      continue;
    }
    if (!name || basename(name) === assetName) {
      if (/^[a-f0-9]{64}$/i.test(hash)) {
        return hash.toLowerCase();
      }
    }
  }
  throw new Error(`Checksum file does not contain ${assetName}`);
}

function compareVersions(left: string, right: string): number {
  const leftParts = left.split(".").map((part) => Number.parseInt(part, 10) || 0);
  const rightParts = right.split(".").map((part) => Number.parseInt(part, 10) || 0);
  const length = Math.max(leftParts.length, rightParts.length);
  for (let index = 0; index < length; index += 1) {
    const diff = (leftParts[index] ?? 0) - (rightParts[index] ?? 0);
    if (diff !== 0) {
      return diff;
    }
  }
  return 0;
}

function stripVersionPrefix(value: string): string {
  return value.trim().replace(/^v/i, "");
}
