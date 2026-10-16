/*
 * The "install mxr as an app" offer. Chromium fires `beforeinstallprompt`
 * when the page can be installed as its own window; that event is the only
 * signal, so the offer shows only when it has fired and the page is not
 * already running as an installed app. Other browsers never fire it, and
 * the offer stays hidden there; the browser menu still installs the app.
 */

import { create } from "zustand";

/** Chromium's event; not in the DOM lib typings. */
export interface BeforeInstallPromptEvent extends Event {
  prompt: () => Promise<void>;
  userChoice: Promise<{ outcome: "accepted" | "dismissed" }>;
}

const DISMISSED_KEY = "mxr.installOffer.dismissed";

interface InstallState {
  /** The deferred prompt, held until the user accepts or dismisses it. */
  deferred: BeforeInstallPromptEvent | null;
  setDeferred: (deferred: BeforeInstallPromptEvent | null) => void;
}

export const useInstallPrompt = create<InstallState>((set) => ({
  deferred: null,
  setDeferred: (deferred) => set({ deferred }),
}));

/** Whether the offer should show: installable, not already an app, not dismissed. */
export function shouldOfferInstall(input: {
  installable: boolean;
  standalone: boolean;
  dismissed: boolean;
}): boolean {
  return input.installable && !input.standalone && !input.dismissed;
}

export function isStandalone(win: Window = window): boolean {
  if (win.matchMedia?.("(display-mode: standalone)").matches) return true;
  // iOS Safari reports installed apps through a non-standard flag.
  return (win.navigator as Navigator & { standalone?: boolean }).standalone === true;
}

/** A storage failure (private window, blocked site data) means not dismissed. */
export function readDismissed(storage: Storage | null = safeStorage()): boolean {
  try {
    return storage?.getItem(DISMISSED_KEY) === "1";
  } catch {
    return false;
  }
}

export function writeDismissed(storage: Storage | null = safeStorage()): void {
  try {
    storage?.setItem(DISMISSED_KEY, "1");
  } catch {
    // The offer comes back next visit; nothing else depends on this.
  }
}

function safeStorage(): Storage | null {
  try {
    return typeof localStorage === "undefined" ? null : localStorage;
  } catch {
    return null;
  }
}

let listening = false;

/** Track the browser's install signals, once per app. */
export function listenForInstallPrompt(win: Window = window): void {
  if (listening) return;
  listening = true;
  win.addEventListener("beforeinstallprompt", (event) => {
    // Hold the prompt so the offer can show it on the user's click.
    event.preventDefault();
    useInstallPrompt.getState().setDeferred(event as BeforeInstallPromptEvent);
  });
  // Once installed the browser stops offering; drop the held prompt so the offer goes too.
  win.addEventListener("appinstalled", () => useInstallPrompt.getState().setDeferred(null));
}

/** Show the browser's install dialog. Resolves to whether the user accepted. */
export async function promptInstall(): Promise<boolean> {
  const { deferred, setDeferred } = useInstallPrompt.getState();
  if (!deferred) return false;
  // A prompt can be used once; the browser fires a new event if it wants to offer again.
  setDeferred(null);
  await deferred.prompt();
  const { outcome } = await deferred.userChoice;
  return outcome === "accepted";
}
