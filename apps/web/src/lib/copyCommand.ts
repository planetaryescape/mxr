import { toast } from "sonner";

/** The command that brings a stopped mxr back (it also starts one that isn't running). */
export const START_COMMAND = "mxr restart";

/** The first step of the protocol-mismatch fix the banner already lists. */
export const UPDATE_COMMAND = "brew upgrade mxr";

/** Copy a shell command for the person to run, and say whether that worked. */
export function copyCommand(command: string): void {
  if (!navigator.clipboard) {
    toast.error("Copy is unavailable in this browser");
    return;
  }
  void navigator.clipboard
    .writeText(command)
    .then(() => toast.success("Command copied"))
    .catch(() => toast.error("Could not copy the command"));
}
