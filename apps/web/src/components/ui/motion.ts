/*
 * Shared motion for overlays (dialogs, popovers, menus, tooltips). Entries
 * take the base duration and exits the fast one, so a closing surface never
 * holds up the next action. Durations and easing come from tokens.css.
 */
export const overlayMotion =
  "ease-out data-[state=open]:duration-base data-[state=delayed-open]:duration-base data-[state=instant-open]:duration-base data-[state=closed]:duration-fast";

/** A scrim fades in over the fast duration (docs/blueprint/26 "Motion"). */
export const scrimMotion = `${overlayMotion} data-[state=open]:duration-fast`;

/** A sheet rises 8px as it fades in; leaving it only fades. */
export const sheetMotion = "data-[state=open]:slide-in-from-bottom-2";
