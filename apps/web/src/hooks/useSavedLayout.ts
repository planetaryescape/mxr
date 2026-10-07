import { useDefaultLayout, type LayoutStorage } from "react-resizable-panels";

/*
 * Split sizes live in this browser's localStorage, one key per split. The
 * library reads the key during render and parses it without a guard, so a
 * blocked store (private windows, disabled site data) or a corrupt value
 * would break the page. This adapter turns both into "no saved size": the
 * split opens at its default and resizing still works, it just won't stick.
 */
export const layoutStorage: LayoutStorage = {
  getItem(key) {
    try {
      const raw = window.localStorage.getItem(key);
      return raw !== null && isLayout(raw) ? raw : null;
    } catch {
      return null;
    }
  },
  setItem(key, value) {
    try {
      window.localStorage.setItem(key, value);
    } catch {
      // Quota or a blocked store: the size lasts this session only.
    }
  },
};

/** A saved layout: an object of panel id to a finite percentage. */
function isLayout(raw: string): boolean {
  try {
    const parsed: unknown = JSON.parse(raw);
    return (
      typeof parsed === "object" &&
      parsed !== null &&
      !Array.isArray(parsed) &&
      Object.values(parsed).every((size) => typeof size === "number" && Number.isFinite(size))
    );
  } catch {
    return false;
  }
}

/**
 * `defaultLayout` and the save callbacks for a Group, saved under
 * `mxr:split:<id>`. Pass `save: false` while the split isn't showing both
 * panes (a phone, a hidden list): the library sizes panes against what is
 * visible, so a layout taken then would overwrite the side-by-side one.
 *
 * `onLayoutChanged` saves the moment a drag or key ends. `onLayoutChange`
 * (debounced by the library) also catches resizes this split didn't start:
 * dragging the sidebar resizes the splits beside it, keeping their pixels.
 */
export function useSavedLayout(id: string, { save }: { save: boolean }) {
  const { defaultLayout, onLayoutChange, onLayoutChanged } = useDefaultLayout({
    id: `mxr:split:${id}`,
    storage: layoutStorage,
  });
  return save ? { defaultLayout, onLayoutChange, onLayoutChanged } : { defaultLayout };
}
