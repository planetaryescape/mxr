import { CircleDot } from "lucide-react";
import { toast, type ExternalToast } from "sonner";

const icon = <CircleDot aria-hidden className="size-4" strokeWidth={1.75} />;

export const assistantToast = {
  error: (message: string, options?: ExternalToast) => toast.error(message, { icon, ...options }),
  success: (message: string, options?: ExternalToast) =>
    toast.success(message, { icon, ...options }),
};
