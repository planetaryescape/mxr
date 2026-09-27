/*
 * Shared motion for overlays (dialogs, popovers, menus, tooltips). Entries
 * take the base duration and exits the fast one, so a closing surface never
 * holds up the next action. Durations and easing come from tokens.css.
 */
export const overlayMotion =
  "ease-out data-[state=open]:duration-base data-[state=delayed-open]:duration-base data-[state=instant-open]:duration-base data-[state=closed]:duration-fast";
