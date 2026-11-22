import { toast } from "sonner";

export function lineSender(name: string, senderEmail: string | null | undefined): string {
  const host = senderEmail?.split("@").pop()?.toLowerCase() ?? "";
  return host && host !== name.toLowerCase() ? `${name} (${host})` : name;
}

export function copyCode(code: string, from: string): void {
  void navigator.clipboard
    .writeText(code)
    .then(() => toast.success(`Code ${code} copied`, { description: `From ${from}` }))
    .catch(() => toast.error("Couldn't copy to the clipboard"));
}
