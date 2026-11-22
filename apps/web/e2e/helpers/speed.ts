import type { Page } from "@playwright/test";

export const KEY_TO_PAINT_P95_MS = Number(process.env.MXR_KEY_TO_PAINT_BUDGET_MS ?? 50);

export type PaintedChange =
  | "cursor-moves"
  | "row-leaves"
  | "star-toggles"
  | "thread-opens"
  | "lamp-moves";

const MAX_FRAMES = 60;

interface ProbeWindow {
  __keyToPaint: {
    samples: number[];
    misses: number;
    armed: { key: string; change: PaintedChange } | null;
  };
}

export async function installKeyToPaintProbe(page: Page): Promise<void> {
  await page.evaluate((maxFrames) => {
    const probe: ProbeWindow["__keyToPaint"] = { samples: [], misses: 0, armed: null };
    (window as unknown as ProbeWindow).__keyToPaint = probe;

    const list = () => document.querySelector("[data-testid='mailbox-list']");
    const activeId = () => list()?.getAttribute("aria-activedescendant") ?? null;
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
        case "lamp-moves": {
          const litId = () =>
            document
              .querySelector("[data-lit='true'] [data-row-id]")
              ?.getAttribute("data-row-id") ?? null;
          const before = litId();
          return () => {
            const now = litId();
            return now !== null && now !== before;
          };
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
  change: PaintedChange;
  between?: () => Promise<void>;
}

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
