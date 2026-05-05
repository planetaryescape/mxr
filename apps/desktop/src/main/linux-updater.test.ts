import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";
import { describe, expect, it, vi } from "vitest";
import { LinuxUpdateManager } from "./linux-updater.js";

vi.mock("electron", () => ({
  shell: {
    openPath: vi.fn().mockResolvedValue(""),
  },
}));

describe("LinuxUpdateManager", () => {
  it("finds a newer Linux installer and verifies its checksum", async () => {
    const installer = Buffer.from("installer bytes");
    const sha = createHash("sha256").update(installer).digest("hex");
    const fetchImpl = vi.fn(async (url: string) => {
      if (url.endsWith("/latest")) {
        return jsonResponse({
          tag_name: "v0.4.64",
          html_url: "https://github.com/planetaryescape/mxr/releases/tag/v0.4.64",
          assets: [
            {
              name: "mxr-v0.4.64-desktop-linux-x64.deb",
              browser_download_url: "https://example.com/mxr.deb",
            },
            {
              name: "mxr-v0.4.64-desktop-linux-x64.deb.sha256",
              browser_download_url: "https://example.com/mxr.deb.sha256",
            },
          ],
        });
      }
      if (url.endsWith(".sha256")) {
        return textResponse(`${sha}  mxr-v0.4.64-desktop-linux-x64.deb\n`);
      }
      return bytesResponse(installer);
    });
    const updater = new LinuxUpdateManager({
      currentVersion: "0.4.63",
      packaged: true,
      platform: "linux",
      arch: "x64",
      fetchImpl: fetchImpl as unknown as typeof fetch,
    });

    await expect(updater.check()).resolves.toMatchObject({
      status: "available",
      version: "0.4.64",
    });
    const downloaded = await updater.download();

    expect(downloaded).toMatchObject({
      status: "downloaded",
      sha256: sha,
    });
    if (downloaded.status === "downloaded") {
      await expect(readFile(downloaded.path, "utf8")).resolves.toBe("installer bytes");
    }
  });

  it("refuses an unverified installer", async () => {
    const fetchImpl = vi.fn(async (url: string) => {
      if (url.endsWith("/latest")) {
        return jsonResponse({
          tag_name: "v0.4.64",
          assets: [
            {
              name: "mxr-v0.4.64-desktop-linux-x64.deb",
              browser_download_url: "https://example.com/mxr.deb",
            },
          ],
        });
      }
      return bytesResponse(Buffer.from("installer bytes"));
    });
    const updater = new LinuxUpdateManager({
      currentVersion: "0.4.63",
      packaged: true,
      platform: "linux",
      arch: "x64",
      fetchImpl: fetchImpl as unknown as typeof fetch,
    });

    await expect(updater.download()).resolves.toMatchObject({
      status: "unavailable",
    });
  });
});

function jsonResponse(value: unknown): Response {
  return new Response(JSON.stringify(value), {
    status: 200,
    headers: { "content-type": "application/json" },
  });
}

function textResponse(value: string): Response {
  return new Response(value, { status: 200 });
}

function bytesResponse(value: Buffer): Response {
  return new Response(new Uint8Array(value), { status: 200 });
}
