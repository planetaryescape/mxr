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
 * Time from a key's keydown to the frame after the app handled it: a
 * capture-phase listener stamps the keydown before the app sees it, and a
 * double requestAnimationFrame lands after the next paint.
 */
export async function installKeyToPaintProbe(page: Page): Promise<void> {
  await page.evaluate(() => {
    const samples: number[] = [];
    (window as unknown as { __keyToPaint: number[] }).__keyToPaint = samples;
    window.addEventListener(
      "keydown",
      () => {
        const start = performance.now();
        requestAnimationFrame(() =>
          requestAnimationFrame(() => samples.push(performance.now() - start)),
        );
      },
      { capture: true },
    );
  });
}

export async function takeKeyToPaintSamples(page: Page): Promise<number[]> {
  return page.evaluate(() => {
    const store = (window as unknown as { __keyToPaint: number[] }).__keyToPaint;
    return store.splice(0, store.length);
  });
}

/** Press `key` `times` times, one frame apart, and return each keydown-to-paint time. */
export async function measureKey(page: Page, key: string, times: number): Promise<number[]> {
  await takeKeyToPaintSamples(page);
  for (let press = 0; press < times; press += 1) {
    await page.keyboard.press(key);
    // Let the probe's two frames land before the next press.
    await page.evaluate(
      () => new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve))),
    );
  }
  return takeKeyToPaintSamples(page);
}

export function p95(samples: number[]): number {
  const sorted = samples.toSorted((a, b) => a - b);
  return sorted[Math.min(sorted.length - 1, Math.ceil(sorted.length * 0.95) - 1)] ?? 0;
}
