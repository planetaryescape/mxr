export const overlayMotion =
  "ease-out data-[state=open]:duration-base data-[state=delayed-open]:duration-base data-[state=instant-open]:duration-base data-[state=closed]:duration-fast";

export const scrimMotion = `${overlayMotion} data-[state=open]:duration-fast`;

export const sheetMotion = "data-[state=open]:slide-in-from-bottom-2";
