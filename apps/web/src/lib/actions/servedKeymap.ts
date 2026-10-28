/*
 * The person's keys from the daemon (`GET /api/v1/platform/keymap`,
 * blueprint chapter 35): a `keys.toml` remap or the single-key switch
 * applies in the web app as in the TUI. The app reads it once before its
 * first render and again after a save in settings; until then, and
 * whenever the daemon can't answer, the generated definition's keys stand.
 */

import { apiFetch } from "@/api/client";
import { useKeymapStore } from "@/state/keymapStore";

import { reregisterCatalog } from "./catalog";
import { setServedKeys, type ServedKeymap } from "./definition";

/** Rebuild the registry with these keys, and note the switch. */
export function applyServedKeymap(keymap: ServedKeymap): void {
  setServedKeys(keymap);
  try {
    reregisterCatalog();
  } catch (error) {
    // Keys this build's registry rejects (a daemon from another release)
    // must not take the keyboard away: the defaults still work.
    console.warn("mxr: the daemon's keys don't fit this build; using the defaults", error);
    setServedKeys(null);
    reregisterCatalog();
  }
  useKeymapStore.getState().setServed({
    singleKeys: keymap.single_keys ?? true,
    remapped: keymap.remapped ?? [],
    problems: keymap.problems ?? [],
  });
}

/** Ask the daemon for the person's keys, waiting at most `timeoutMs`. */
export async function loadServedKeymap(timeoutMs = 1500): Promise<void> {
  let timer: ReturnType<typeof setTimeout> | undefined;
  const timeout = new Promise<null>((resolve) => {
    timer = setTimeout(() => resolve(null), timeoutMs);
  });
  try {
    const response = await Promise.race([
      apiFetch<{ keymap: ServedKeymap }>("/api/v1/platform/keymap"),
      timeout,
    ]);
    if (response) applyServedKeymap(response.keymap);
  } catch {
    // No daemon yet, or no token: the definition's keys stand.
  } finally {
    clearTimeout(timer);
  }
}
