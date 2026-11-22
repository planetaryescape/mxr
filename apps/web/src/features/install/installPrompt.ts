import { create } from "zustand";

export interface BeforeInstallPromptEvent extends Event {
  prompt: () => Promise<void>;
  userChoice: Promise<{ outcome: "accepted" | "dismissed" }>;
}

const DISMISSED_KEY = "mxr.installOffer.dismissed";

interface InstallState {
  deferred: BeforeInstallPromptEvent | null;
  setDeferred: (deferred: BeforeInstallPromptEvent | null) => void;
}

export const useInstallPrompt = create<InstallState>((set) => ({
  deferred: null,
  setDeferred: (deferred) => set({ deferred }),
}));

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
  } catch {}
}

function safeStorage(): Storage | null {
  try {
    return typeof localStorage === "undefined" ? null : localStorage;
  } catch {
    return null;
  }
}

let listening = false;

export function listenForInstallPrompt(win: Window = window): void {
  if (listening) return;
  listening = true;
  win.addEventListener("beforeinstallprompt", (event) => {
    event.preventDefault();
    useInstallPrompt.getState().setDeferred(event as BeforeInstallPromptEvent);
  });
  win.addEventListener("appinstalled", () => useInstallPrompt.getState().setDeferred(null));
}

export async function promptInstall(): Promise<boolean> {
  const { deferred, setDeferred } = useInstallPrompt.getState();
  if (!deferred) return false;
  setDeferred(null);
  await deferred.prompt();
  const { outcome } = await deferred.userChoice;
  return outcome === "accepted";
}
