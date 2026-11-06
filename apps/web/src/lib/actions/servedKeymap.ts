/*
 * The person's keys from the daemon (`GET /api/v1/platform/keymap`,
 * blueprint chapter 35): a `keys.toml` remap or the single-key switch
 * applies in the web app as in the TUI.
 *
 * Nothing waits for this request before the first paint. The last answer is
 * cached per bridge, so a returning app has the person's keys before its
 * first render; the request then refreshes that cache in the background and
 * `useKeymapStore` re-renders every view that reads keys from the registry.
 * On a first visit, or whenever the daemon can't answer, the generated
 * definition's keys stand.
 *
 * The cache is read from module evaluation, not from `main.tsx`'s body:
 * modules that build their key labels once (`components/navMap.ts`,
 * `components/sidebarRail.ts`) are evaluated while `@/App` is imported, which
 * happens before the entry module's body runs. `main.tsx` imports this module
 * first for that reason.
 */

import { apiFetch } from "@/api/client";
import { getBridgeBaseUrl } from "@/lib/tokenStorage";
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

/** Cache key: one bridge's `keys.toml`, so two daemons can't share keys. */
const CACHE_PREFIX = "mxr.servedKeymap.v1:";
/** A served keymap is ~184 kB; anything past this is not one we wrote. */
const CACHE_MAX_CHARS = 1_000_000;
const MAX_ACTIONS = 2_000;
const MAX_KEYS = 32;
const MAX_CHORD_LENGTH = 64;
const MAX_ID_LENGTH = 120;
/** Ids that would poison an object built from the payload, were one ever built. */
const UNSAFE_IDS = new Set(["__proto__", "constructor", "prototype"]);
const ACTION_ID = /^[A-Za-z0-9][A-Za-z0-9_.:-]*$/;

function cacheKey(): string | null {
  try {
    return `${CACHE_PREFIX}${getBridgeBaseUrl()}`;
  } catch {
    return null;
  }
}

function stringArray(value: unknown, max: number, maxLength: number): string[] | undefined {
  if (!Array.isArray(value) || value.length > max) return undefined;
  const out: string[] = [];
  for (const item of value) {
    if (typeof item !== "string" || item.length > maxLength) return undefined;
    out.push(item);
  }
  return out;
}

/**
 * The served keymap in a cached record, rebuilt field by field. Whatever the
 * browser holds is untrusted (another script on the origin, a hand-edited
 * value): an entry that isn't the shape the daemon serves is dropped, never
 * passed through.
 */
export function parseCachedKeymap(raw: string | null): ServedKeymap | null {
  if (!raw || raw.length > CACHE_MAX_CHARS) return null;
  let record: unknown;
  try {
    record = JSON.parse(raw);
  } catch {
    return null;
  }
  if (!record || typeof record !== "object") return null;
  const value = record as Record<string, unknown>;
  const cached = value.keymap;
  if (!cached || typeof cached !== "object") return null;
  const fields = cached as Record<string, unknown>;
  if (!Array.isArray(fields.actions) || fields.actions.length > MAX_ACTIONS) return null;

  const actions: ServedKeymap["actions"] = [];
  for (const entry of fields.actions) {
    if (!entry || typeof entry !== "object") return null;
    const action = entry as Record<string, unknown>;
    const id = action.id;
    if (
      typeof id !== "string" ||
      id.length === 0 ||
      id.length > MAX_ID_LENGTH ||
      UNSAFE_IDS.has(id) ||
      !ACTION_ID.test(id)
    ) {
      return null;
    }
    const keys =
      action.keys === undefined ? undefined : stringArray(action.keys, MAX_KEYS, MAX_CHORD_LENGTH);
    const macKeys =
      action.mac_keys === undefined
        ? undefined
        : stringArray(action.mac_keys, MAX_KEYS, MAX_CHORD_LENGTH);
    if ((action.keys !== undefined && !keys) || (action.mac_keys !== undefined && !macKeys))
      return null;
    actions.push({ id, ...(keys ? { keys } : {}), ...(macKeys ? { mac_keys: macKeys } : {}) });
  }

  const singleKeys = fields.single_keys;
  if (singleKeys !== undefined && typeof singleKeys !== "boolean") return null;
  const remapped =
    fields.remapped === undefined
      ? undefined
      : stringArray(fields.remapped, MAX_ACTIONS, MAX_ID_LENGTH);
  const problems =
    fields.problems === undefined ? undefined : stringArray(fields.problems, MAX_ACTIONS, 500);
  if ((fields.remapped !== undefined && !remapped) || (fields.problems !== undefined && !problems))
    return null;

  return {
    actions,
    ...(singleKeys === undefined ? {} : { single_keys: singleKeys }),
    ...(remapped ? { remapped } : {}),
    ...(problems ? { problems } : {}),
  };
}

function storage(): Storage | null {
  try {
    return window.localStorage ?? null;
  } catch {
    // Private mode, or storage disabled: the defaults stand until the daemon answers.
    return null;
  }
}

/** Note this answer for the next boot. A full or disabled storage is not an error. */
function cacheKeymap(keymap: ServedKeymap): void {
  const key = cacheKey();
  if (!key) return;
  try {
    storage()?.setItem(key, JSON.stringify({ savedAt: new Date().toISOString(), keymap }));
  } catch {
    // Quota, or storage disabled: this boot still has the keys from the daemon.
  }
}

/** The last keymap this bridge served, if there is one. */
export function readCachedKeymap(): ServedKeymap | null {
  const key = cacheKey();
  if (!key) return null;
  try {
    return parseCachedKeymap(storage()?.getItem(key) ?? null);
  } catch {
    return null;
  }
}

/**
 * Put the cached keys in place before the first render, so a returning app
 * shows the person's keys rather than the definition's defaults.
 */
export function applyCachedKeymap(): boolean {
  const keymap = readCachedKeymap();
  if (!keymap) return false;
  applyServedKeymap(keymap);
  return true;
}

// Read and applied on import, ahead of `@/App` (see the module comment).
applyCachedKeymap();

/**
 * Ask the daemon for the person's keys and apply them when they arrive. The
 * app has already painted by then; the store's version makes every view that
 * reads keys recompute, and the registry makes the new chords live at once.
 */
export async function loadServedKeymap(timeoutMs = 8000): Promise<void> {
  let timer: ReturnType<typeof setTimeout> | undefined;
  const timeout = new Promise<null>((resolve) => {
    timer = setTimeout(() => resolve(null), timeoutMs);
  });
  try {
    const response = await Promise.race([
      apiFetch<{ keymap: ServedKeymap }>("/api/v1/platform/keymap"),
      timeout,
    ]);
    if (response) {
      applyServedKeymap(response.keymap);
      cacheKeymap(response.keymap);
    }
  } catch {
    // No daemon yet, or no token: the cached or definition's keys stand.
  } finally {
    clearTimeout(timer);
  }
}
