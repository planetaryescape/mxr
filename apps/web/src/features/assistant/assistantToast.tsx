/*
 * The assistant's toasts wear its glyph, the one in the top bar and the
 * panel header (docs/blueprint/29), so a toast says who spoke. Sonner's
 * `icon` replaces the status icon only: an error keeps its colour, and
 * the one-at-a-time and timing rules stay the Toaster's (D140).
 */

import { CircleDot } from "lucide-react";
import { toast, type ExternalToast } from "sonner";

const icon = <CircleDot aria-hidden className="size-4" strokeWidth={1.75} />;

export const assistantToast = {
  error: (message: string, options?: ExternalToast) => toast.error(message, { icon, ...options }),
  success: (message: string, options?: ExternalToast) =>
    toast.success(message, { icon, ...options }),
};
