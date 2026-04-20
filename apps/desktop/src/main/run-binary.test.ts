import { describe, expect, it } from "vitest";
import { runBinary } from "./run-binary.js";

describe("runBinary", () => {
  it("captures stdout from a successful command", async () => {
    const result = await runBinary(process.execPath, ["-e", "process.stdout.write('desktop-ok')"]);

    expect(result).toEqual({
      stdout: "desktop-ok",
      stderr: "",
    });
  });

  it("times out long-running commands", async () => {
    await expect(
      runBinary(process.execPath, ["-e", "setTimeout(() => process.stdout.write('late'), 5000)"], {
        timeoutMs: 25,
        stopTimeoutMs: 25,
      }),
    ).rejects.toThrow(`Timed out running ${process.execPath} -e`);
  });
});
