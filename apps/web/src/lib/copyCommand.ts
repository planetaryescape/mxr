import { toast } from "sonner";

export const START_COMMAND = "mxr restart";

export const UPDATE_COMMAND = "brew upgrade mxr";

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
