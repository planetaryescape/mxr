import type { Page } from "@playwright/test";

/**
 * Keydown-to-paint budget at p95 (rubric B1): 50 ms. Measured against the
 * Vite dev build, whose React is several times slower than production, and
 * quantized to frames (about 16 ms on a 60 Hz headless run), so a local run
 * sits near one frame. A slower CI runner can raise the bar through
 * MXR_KEY_TO_PAINT_BUDGET_MS, which keeps the change visible in the
 * workflow rather than in a loosened assertion here.
 */
export const KEY_TO_PAINT_P95_MS = Number(process.env.MXR_KEY_TO_PAINT_BUDGET_MS ?? 50);

/**
 * What a key must change on screen before its timing counts. Timing the
 * next paint alone would pass a key the app ignored.
 *
 * - `cursor-moves`: the list's active row is a different row.
 * - `row-leaves`: the row under the cursor is gone from the list.
 * - `star-toggles`: the row under the cursor flipped its starred state.
 * - `thread-opens`: the reader shows a conversation with its messages.
 */
export type PaintedChange = "cursor-moves" | "row-leaves" | "star-toggles" | "thread-opens";

/** Frames to wait for the change before calling the sample a miss (about 1 s at 60 Hz). */
const MAX_FRAMES = 60;

interface ProbeWindow {
  __keyToPaint: {
    samples: number[];
    misses: number;
    armed: { key: string; change: PaintedChange } | null;
  };
}

/**
 * Install the probe: a capture-phase keydown listener snapshots the state
 * before the app sees the key, then checks the snapshot on every animation
 * frame. The first frame whose callback sees the change is the frame that
 * paints it, so the sample is taken one frame later, after that paint. A
 * key whose change never shows is a miss, and a miss fails `measureKey`.
 */
export async function installKeyToPaintProbe(page: Page): Promise<void> {
  await page.evaluate((maxFrames) => {
    const probe: ProbeWindow["__keyToPaint"] = { samples: [], misses: 0, armed: null };
    (window as unknown as ProbeWindow).__keyToPaint = probe;

    const list = () => document.querySelector("[data-testid='mailbox-list']");
    const activeId = () => list()?.getAttribute("aria-activedescendant") ?? null;
    // Rows name their state first: "Unread., Starred., Maya, …".
    const starred = (id: string | null) =>
      id === null
        ? null
        : (document.getElementById(id)?.getAttribute("aria-label") ?? "")
            .split(", ")
            .includes("Starred.");
    const openThread = () => {
      const article = document.querySelector("article[aria-label^='Conversation:']");
      if (!article?.querySelector("[data-testid='thread-message']")) return null;
      return article.getAttribute("aria-label");
    };

    type Check = () => boolean;
    const watch = (change: PaintedChange): Check => {
      switch (change) {
        case "cursor-moves": {
          const before = activeId();
          return () => {
            const now = activeId();
            return now !== null && now !== before;
          };
        }
        case "row-leaves": {
          const before = activeId();
          return () => before !== null && document.getElementById(before) === null;
        }
        case "star-toggles": {
          const id = activeId();
          const before = starred(id);
          return () => id !== null && starred(id) !== before;
        }
        case "thread-opens": {
          const before = openThread();
          return () => {
            const now = openThread();
            return now !== null && now !== before;
          };
        }
      }
    };

    window.addEventListener(
      "keydown",
      (event) => {
        const armed = probe.armed;
        if (!armed || event.key !== armed.key || event.repeat) return;
        const start = performance.now();
        const changed = watch(armed.change);
        let frames = 0;
        const onFrame = () => {
          frames += 1;
          if (changed()) {
            // This frame paints the change; the next callback runs after it.
            requestAnimationFrame(() => probe.samples.push(performance.now() - start));
          } else if (frames >= maxFrames) {
            probe.misses += 1;
          } else {
            requestAnimationFrame(onFrame);
          }
        };
        requestAnimationFrame(onFrame);
      },
      { capture: true },
    );
  }, MAX_FRAMES);
}

export interface MeasureOptions {
  /** The change each press must paint (see `PaintedChange`). */
  change: PaintedChange;
  /** Runs after each measured press, to set up the next (its keys aren't timed). */
  between?: () => Promise<void>;
}

/**
 * Press `key` `times` times and return each keydown-to-paint time. Throws
 * when any press didn't paint its change, so a key the app dropped fails
 * the gate instead of recording a fast empty frame.
 */
export async function measureKey(
  page: Page,
  key: string,
  times: number,
  { change, between }: MeasureOptions,
): Promise<number[]> {
  await page.evaluate(
    ([armedKey, armedChange]) => {
      const probe = (window as unknown as ProbeWindow).__keyToPaint;
      probe.samples.length = 0;
      probe.misses = 0;
      probe.armed = { key: armedKey, change: armedChange };
    },
    [key, change] as const,
  );
  for (let press = 0; press < times; press += 1) {
    await page.keyboard.press(key);
    // Wait until this press is sampled (or missed) before the next one.
    await page.waitForFunction((count) => {
      const probe = (window as unknown as ProbeWindow).__keyToPaint;
      return probe.samples.length + probe.misses >= count;
    }, press + 1);
    await between?.();
  }
  const { samples, misses } = await page.evaluate(() => {
    const probe = (window as unknown as ProbeWindow).__keyToPaint;
    probe.armed = null;
    return { samples: [...probe.samples], misses: probe.misses };
  });
  if (misses > 0) {
    throw new Error(`${key}: ${misses} of ${times} presses never painted "${change}"`);
  }
  return samples;
}

export function p95(samples: number[]): number {
  const sorted = samples.toSorted((a, b) => a - b);
  return sorted[Math.min(sorted.length - 1, Math.ceil(sorted.length * 0.95) - 1)] ?? 0;
}
