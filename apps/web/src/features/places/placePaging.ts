import type { PlaceBundle, PlaceMessage, PlaceResponse } from "./api";
import { bundleKey } from "./placeCopy";

export function nextBundleOffset(pages: readonly PlaceResponse[]): number | undefined {
  const last = pages.at(-1);
  if (!last) return 0;
  const loaded = pages.reduce((sum, page) => sum + page.bundles.length, 0);
  return last.bundles.length > 0 && loaded < last.total_bundles ? loaded : undefined;
}

export function mergeBundles(
  pages: readonly PlaceResponse[],
  more: ReadonlyMap<string, readonly PlaceMessage[]> = new Map(),
): PlaceBundle[] {
  const seen = new Set<string>();
  const bundles: PlaceBundle[] = [];
  for (const bundle of pages.flatMap((page) => page.bundles)) {
    const key = bundleKey(bundle);
    if (seen.has(key)) continue;
    seen.add(key);
    const extra = more.get(key);
    if (!extra || extra.length === 0) {
      bundles.push(bundle);
      continue;
    }
    const ids = new Set(bundle.messages.map((message) => message.message_id));
    const messages = [...bundle.messages];
    for (const message of extra) {
      if (ids.has(message.message_id)) continue;
      ids.add(message.message_id);
      messages.push(message);
    }
    bundles.push({ ...bundle, messages });
  }
  return bundles;
}

export function messagesLeft(bundle: Pick<PlaceBundle, "message_count" | "messages">): number {
  return Math.max(0, bundle.message_count - bundle.messages.length);
}

function isPlaceResponse(value: unknown): value is PlaceResponse {
  return (
    typeof value === "object" &&
    value !== null &&
    Array.isArray((value as { bundles?: unknown }).bundles)
  );
}

function isPaged(value: unknown): value is { pages: unknown[] } {
  return (
    typeof value === "object" &&
    value !== null &&
    Array.isArray((value as { pages?: unknown }).pages)
  );
}

export function mapPlaceData(
  data: unknown,
  update: (page: PlaceResponse) => PlaceResponse,
): unknown {
  if (isPaged(data)) {
    return {
      ...data,
      pages: data.pages.map((page) => (isPlaceResponse(page) ? update(page) : page)),
    };
  }
  return isPlaceResponse(data) ? update(data) : data;
}
