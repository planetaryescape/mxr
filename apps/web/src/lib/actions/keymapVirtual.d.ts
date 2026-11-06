/// <reference types="vite/client" />

/*
 * The two keymap modules `keymapPlugin.ts` builds from
 * `keymap.generated.json`. They are virtual: the plugin reads the generated
 * file at build time, so the entry never imports that 290 kB JSON directly.
 *
 * The types stay in `keymapPlugin.ts`; the `import()` queries below are
 * type-only, so this file stays a script and its declarations are ambient.
 */

type KeyIndexModule = import("./keymapPlugin").KeyIndex;
type ActionCatalogModule = import("./keymapPlugin").ActionCatalog;

declare module "virtual:mxr-key-index" {
  const index: KeyIndexModule;
  export default index;
}

declare module "virtual:mxr-action-catalog" {
  const catalog: ActionCatalogModule;
  export default catalog;
}
