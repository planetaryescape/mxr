import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Check, Minus, Pencil, Plus, Tag, Trash2 } from "lucide-react";
import { useMemo, useState } from "react";
import { toast } from "sonner";

import { Button } from "@/components/ui/button";
import {
  Command,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
} from "@/components/ui/command";
import { Dialog, DialogContent, DialogDescription, DialogTitle } from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import {
  applyCategoryMembership,
  createCategory,
  fetchCategories,
  fetchCategoryMemberships,
  previewCategoryMembership,
  deleteCategory,
  previewDeleteCategory,
  renameCategory,
  shellKey,
  type CategoryDeletePreview,
  type CategoryMembershipPreview,
  type PersonalCategoryView,
} from "@/features/mailbox/api";
import { plural } from "@/lib/format";
import { cn } from "@/lib/utils";
import { useUndo } from "@/state/undoStore";

import { describeTarget } from "../mailVerbs";
import { claimUndo, offerUndo } from "../mailUndo";
import { undoCategoryChanges } from "../categoryUndo";
import type { MailTarget } from "../target";

export function CategoriesDialog({ target, onClose }: { target: MailTarget; onClose: () => void }) {
  const queryClient = useQueryClient();
  const [query, setQuery] = useState("");
  const [staged, setStaged] = useState<Map<string, boolean>>(new Map());
  const [managing, setManaging] = useState<PersonalCategoryView | null>(null);
  const [assignmentPreview, setAssignmentPreview] = useState<CategoryMembershipPreview | null>(
    null,
  );
  const accountId =
    target.accountId ??
    (new Set(target.rows.map((row) => row.account_id)).size === 1
      ? target.rows[0]?.account_id
      : undefined);
  const categories = useQuery({
    queryKey: [...shellKey, "personal-categories", accountId ?? "all"],
    // The account scopes counts and the category lens. The local catalog is
    // profile-wide, including categories not yet assigned in this account.
    queryFn: () => fetchCategories(accountId),
  });
  const categoryIds = categories.data?.map((category) => category.id) ?? [];
  const membership = useQuery({
    queryKey: ["category-memberships", accountId, target.messageIds, categoryIds],
    queryFn: async () => {
      const chunks = Array.from({ length: Math.ceil(categoryIds.length / 40) }, (_, index) =>
        categoryIds.slice(index * 40, (index + 1) * 40),
      );
      return (
        await Promise.all(
          chunks.map((ids) =>
            fetchCategoryMemberships({
              accountId,
              messageIds: target.messageIds,
              categoryIds: ids,
            }),
          ),
        )
      ).flat();
    },
    enabled: target.messageIds.length > 0 && categoryIds.length > 0,
  });
  const states = useMemo(() => {
    const byId = new Map((membership.data ?? []).map((state) => [state.category_id, state]));
    return new Map(
      (categories.data ?? []).map((category) => {
        const state = byId.get(category.id);
        const assigned = state?.assigned_messages ?? 0;
        const count = state?.selected_messages ?? target.messageIds.length;
        return [
          category.id,
          assigned === 0 ? "none" : assigned === count ? "all" : "some",
        ] as const;
      }),
    );
  }, [categories.data, membership.data, target.messageIds.length]);
  const create = useMutation({
    mutationFn: createCategory,
    onSuccess: async (_, name) => {
      await queryClient.invalidateQueries({ queryKey: [...shellKey, "personal-categories"] });
      setQuery("");
      toast.success(`Created ${name}`);
    },
    onError: (error) => toast.error("Couldn't create category", { description: error.message }),
  });
  const items = categories.data ?? [];
  const trimmed = query.trim();
  const exists = items.some(
    (item) => item.name.localeCompare(trimmed, undefined, { sensitivity: "accent" }) === 0,
  );
  const toggle = (id: string) =>
    setStaged((previous) => {
      const next = new Map(previous);
      const original = states.get(id) ?? "none";
      const current = next.get(id) ?? original === "all";
      const wanted = !current;
      if ((wanted && original === "all") || (!wanted && original === "none")) next.delete(id);
      else next.set(id, wanted);
      return next;
    });
  const add = [...staged].filter(([, on]) => on).map(([id]) => id);
  const remove = [...staged].filter(([, on]) => !on).map(([id]) => id);
  const stagePreview = async () => {
    try {
      const preview = await previewCategoryMembership({
        accountId,
        messageIds: target.messageIds,
        addCategoryIds: add,
        removeCategoryIds: remove,
      });
      setAssignmentPreview(preview);
    } catch (error) {
      toast.error("Couldn't preview category changes", {
        description: error instanceof Error ? error.message : String(error),
      });
    }
  };
  const apply = async () => {
    if (!assignmentPreview) return;
    const previousUndo = useUndo.getState();
    const claim = claimUndo();
    try {
      const result = await applyCategoryMembership(assignmentPreview.preview_token);
      if (result.added + result.removed === 0 && !result.mutation_id) {
        // A stale preview can become a no-op; keep the previous action undoable.
        if (useUndo.getState().lastUndo === claim.run) {
          useUndo.setState({
            lastUndo: previousUndo.lastUndo,
            lastUndoAt: previousUndo.lastUndoAt,
            lastMutationId: previousUndo.lastMutationId,
          });
        }
        claim.settle(null);
        toast.info("Categories already match this selection");
        onClose();
        void queryClient.invalidateQueries();
        return;
      }
      const reverse = result.mutation_id
        ? () => undoCategoryChanges(result.mutation_id, queryClient)
        : null;
      claim.settle(
        offerUndo(
          "categories",
          `Updated ${plural(result.added + result.removed, "category assignment")}`,
          `categories-${result.mutation_id}`,
          reverse,
          claim.run,
        ),
      );
      onClose();
      void queryClient.invalidateQueries();
    } catch (error) {
      claim.settle(null);
      toast.error("Couldn't apply category changes", {
        description: error instanceof Error ? error.message : String(error),
      });
    }
  };

  return (
    <>
      <Dialog open onOpenChange={(open) => !open && onClose()}>
        <DialogContent className="max-w-md gap-0 overflow-hidden p-0">
          <div className="border-b border-border px-4 pb-3 pt-4">
            <DialogTitle>Categories</DialogTitle>
            <DialogDescription className="mt-1 truncate">
              {describeTarget(target)}
            </DialogDescription>
          </div>
          <Command loop>
            <CommandInput
              value={query}
              onValueChange={setQuery}
              placeholder="Find or create a category…"
            />
            <CommandList className="max-h-72">
              {categories.isError ? (
                <p role="alert" className="px-4 py-3 text-sm text-destructive">
                  Couldn't load categories: {categories.error.message}
                </p>
              ) : null}
              {membership.isError ? (
                <p role="alert" className="px-4 py-3 text-sm text-destructive">
                  Couldn't load current assignments: {membership.error.message}
                </p>
              ) : null}
              <CommandEmpty>
                {items.length === 0
                  ? "No categories yet. Type a name to create one."
                  : "No matching categories"}
              </CommandEmpty>
              <CommandGroup>
                {items.map((category) => {
                  const presence = states.get(category.id) ?? "none";
                  const changed = staged.has(category.id);
                  const current = changed ? (staged.get(category.id) ? "all" : "none") : presence;
                  return (
                    <CommandItem
                      key={category.id}
                      value={category.name}
                      onSelect={() => toggle(category.id)}
                      disabled={membership.isLoading || membership.isError}
                      className="group gap-2.5"
                      aria-label={`${category.name}: ${current === "all" ? "applied" : current === "some" ? "on some" : "not applied"}`}
                      data-state-presence={current}
                    >
                      <span
                        aria-hidden
                        className={cn(
                          "grid size-4 place-items-center rounded-[4px] border",
                          current === "none"
                            ? "border-border-strong"
                            : "border-primary bg-primary text-primary-foreground",
                        )}
                      >
                        {current === "all" ? (
                          <Check className="size-3" strokeWidth={3} />
                        ) : current === "some" ? (
                          <Minus className="size-3" strokeWidth={3} />
                        ) : null}
                      </span>
                      <span className="min-w-0 flex-1 truncate">{category.name}</span>
                      {changed ? (
                        <span className="font-mono text-2xs text-primary">
                          {current === "all" ? "+ add" : "− remove"}
                        </span>
                      ) : null}
                      <button
                        type="button"
                        tabIndex={0}
                        aria-label={`Manage ${category.name}`}
                        className="grid size-7 place-items-center rounded text-muted-foreground hover:bg-muted hover:text-foreground focus-visible:outline focus-visible:outline-2 focus-visible:outline-primary"
                        onKeyDown={(event) => {
                          // cmdk owns Enter on its items; let this button activate itself.
                          if (event.key === "Enter" || event.key === " ") event.stopPropagation();
                        }}
                        onClick={(event) => {
                          event.stopPropagation();
                          setManaging(category);
                        }}
                      >
                        <Pencil className="size-3" />
                      </button>
                    </CommandItem>
                  );
                })}
                {trimmed && !exists ? (
                  <CommandItem
                    value={`create ${trimmed}`}
                    onSelect={() => create.mutate(trimmed)}
                    disabled={create.isPending}
                  >
                    <Plus className="size-4" />
                    Create “{trimmed}”
                  </CommandItem>
                ) : null}
              </CommandGroup>
            </CommandList>
          </Command>
          <div className="flex items-center justify-between gap-2 border-t border-border px-4 py-3">
            <span className="text-2xs text-muted-foreground">
              {add.length + remove.length
                ? `${plural(add.length, "add")}, ${plural(remove.length, "removal")}`
                : "Enter toggles a category"}
            </span>
            <span className="flex gap-2">
              <Button variant="ghost" size="sm" onClick={onClose}>
                Cancel
              </Button>
              <Button
                size="sm"
                disabled={membership.isLoading || add.length + remove.length === 0}
                onClick={() => void stagePreview()}
              >
                <Tag className="size-3.5" /> Preview
              </Button>
            </span>
          </div>
        </DialogContent>
      </Dialog>
      {managing ? <ManageCategory category={managing} onClose={() => setManaging(null)} /> : null}
      <AlertDialog
        open={Boolean(assignmentPreview)}
        onOpenChange={(open) => !open && setAssignmentPreview(null)}
      >
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Apply category changes?</AlertDialogTitle>
            <AlertDialogDescription>
              This preview will add {assignmentPreview?.added ?? 0} and remove{" "}
              {assignmentPreview?.removed ?? 0} assignments across{" "}
              {assignmentPreview?.message_count ?? 0} messages.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction onClick={() => void apply()}>Apply changes</AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </>
  );
}

function ManageCategory({
  category,
  onClose,
}: {
  category: PersonalCategoryView;
  onClose: () => void;
}) {
  const queryClient = useQueryClient();
  const [name, setName] = useState(category.name);
  const [deletePreview, setDeletePreview] = useState<CategoryDeletePreview | null>(null);
  const rename = useMutation({
    mutationFn: () => renameCategory(category.id, name),
    onSuccess: () => {
      void queryClient.invalidateQueries();
      onClose();
      toast.success("Category renamed");
    },
    onError: (error) => toast.error("Couldn't rename category", { description: error.message }),
  });
  const remove = async () => {
    try {
      if (!deletePreview) {
        setDeletePreview(await previewDeleteCategory(category.id));
        return;
      }
      await deleteCategory(deletePreview.preview_token);
      await queryClient.invalidateQueries();
      onClose();
      toast.success("Category deleted");
    } catch (error) {
      toast.error("Couldn't delete category", {
        description: error instanceof Error ? error.message : String(error),
      });
    }
  };
  return (
    <Dialog open onOpenChange={(open) => !open && onClose()}>
      <DialogContent className="max-w-sm">
        <DialogTitle>Manage category</DialogTitle>
        <DialogDescription>Rename keeps this category’s assignments.</DialogDescription>
        <Input
          value={name}
          onChange={(event) => setName(event.target.value)}
          aria-label="Category name"
        />
        <div className="flex justify-between">
          <Button variant="destructive" onClick={() => void remove()}>
            <Trash2 className="size-4" />
            {deletePreview ? "Confirm delete" : "Preview delete"}
          </Button>
          <div className="flex gap-2">
            <Button variant="ghost" onClick={onClose}>
              Cancel
            </Button>
            <Button onClick={() => rename.mutate()}>Save name</Button>
          </div>
        </div>
        {deletePreview ? (
          <p role="status" className="text-sm text-muted-foreground">
            Deleting “{deletePreview.name}” will remove it from{" "}
            {plural(deletePreview.affected_messages, "message")}.
          </p>
        ) : null}
      </DialogContent>
    </Dialog>
  );
}
