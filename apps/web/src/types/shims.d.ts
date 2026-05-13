// Type shims for libraries whose package.json exports don't expose .d.ts in the
// path tsc resolves. We re-export the typings from their actual source.

declare module "tinykeys" {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  export type KeyBindingMap = Record<string, (event: KeyboardEvent) => any>;
  export type KeyBindingHandler = (event: KeyboardEvent) => void;
  export interface KeyBindingOptions {
    event?: "keydown" | "keyup";
    capture?: boolean;
    timeout?: number;
  }
  export function tinykeys(
    target: Window | HTMLElement,
    keyBindingMap: KeyBindingMap,
    options?: KeyBindingOptions,
  ): () => void;
}
