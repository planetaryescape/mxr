import type { QueryClient } from "@tanstack/react-query";
import { toast } from "sonner";

import { undoCategoryMembership } from "@/features/mailbox/api";
import { plural } from "@/lib/format";

export async function undoCategoryChanges(id: string, queryClient: QueryClient): Promise<boolean> {
  try {
    const result = await undoCategoryMembership(id);
    void queryClient.invalidateQueries();
    if (result.skipped > 0) {
      toast.warning(`Restored ${plural(result.restored, "category assignment")}`, {
        description: `Left ${plural(result.skipped, "assignment")} changed since then untouched.`,
      });
      return false;
    }
    toast.success("Category changes undone");
    return true;
  } catch (error) {
    toast.error("Couldn't undo category changes", {
      description: error instanceof Error ? error.message : String(error),
    });
    return false;
  }
}
