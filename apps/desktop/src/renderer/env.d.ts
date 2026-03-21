import type { DesktopApi } from "../shared/types";

declare global {
  interface Window {
    mxrDesktop: DesktopApi;
  }
}

export {};
