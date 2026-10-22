import { toast } from "sonner";

/**
 * "Google (accounts.google.com)" for a line: the name a sender chose beside
 * the host its mail came from, so a borrowed brand shows in the toast.
 */
export function lineSender(name: string, senderEmail: string | null | undefined): string {
  const host = senderEmail?.split("@").pop()?.toLowerCase() ?? "";
  return host && host !== name.toLowerCase() ? `${name} (${host})` : name;
}

/**
 * Put a one-time code on the clipboard and say whose it is, so a code is
 * never pasted into the wrong sign-in.
 */
export function copyCode(code: string, from: string): void {
  void navigator.clipboard
    .writeText(code)
    .then(() => toast.success(`Code ${code} copied`, { description: `From ${from}` }))
    .catch(() => toast.error("Couldn't copy to the clipboard"));
}
