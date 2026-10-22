import { toast } from "sonner";

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
