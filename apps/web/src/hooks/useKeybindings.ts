import { useEffect } from "react";
import { tinykeys, type KeyBindingMap } from "tinykeys";

interface Options {
  /** When true, keybindings are not installed. Useful while editors have focus. */
  disabled?: boolean;
}

export function useKeybindings(map: KeyBindingMap, opts: Options = {}): void {
  useEffect(() => {
    if (opts.disabled) return;
    return tinykeys(window, map);
  }, [map, opts.disabled]);
}
