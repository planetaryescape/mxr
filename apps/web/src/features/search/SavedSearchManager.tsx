import { useMutation } from "@tanstack/react-query";
import { Pin, PinOff, Trash2 } from "lucide-react";
import { useState } from "react";
import { toast } from "sonner";

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
import { Button } from "@/components/ui/button";

import { deleteSavedSearch, updateSavedSearch, type SavedSearch } from "./api";

/** Pin, colour and delete saved searches (TUI sidebar n / e / d). */
export function SavedSearchManager({
  searches,
  onChange,
  onRun,
}: {
  searches: SavedSearch[];
  onChange: () => void;
  onRun: (search: SavedSearch) => void;
}) {
  const [deleting, setDeleting] = useState<SavedSearch | null>(null);
  const update = useMutation({
    mutationFn: ({
      name,
      patch,
    }: {
      name: string;
      patch: Parameters<typeof updateSavedSearch>[1];
    }) => updateSavedSearch(name, patch),
    onSuccess: () => {
      onChange();
      toast.success("Saved search updated");
    },
    onError: (error: Error) =>
      toast.error("Couldn't update the saved search", { description: error.message }),
  });
  const remove = useMutation({
    mutationFn: (name: string) => deleteSavedSearch(name),
    onSuccess: (_, name) => {
      onChange();
      toast.success(`Deleted “${name}”`);
    },
    onError: (error: Error) =>
      toast.error("Couldn't delete the saved search", { description: error.message }),
  });

  if (searches.length === 0) {
    return (
      <p className="px-4 py-6 text-[13px] text-muted-foreground">
        No saved searches yet. Run a search and press Save to keep it in the sidebar.
      </p>
    );
  }

  return (
    <>
      <ul className="divide-y divide-border/70">
        {searches.map((search, index) => {
          const pinned = (search.position ?? 0) < 0;
          return (
            <li key={search.id} className="group flex items-center gap-3 px-4 py-2.5">
              <span className="w-8 shrink-0 font-mono text-2xs text-faint">
                {index < 9 ? `g ${index + 1}` : ""}
              </span>
              <button
                type="button"
                onClick={() => onRun(search)}
                className="min-w-0 flex-1 text-left"
              >
                <span className="block truncate text-[13px] font-medium">{search.name}</span>
                <span className="block truncate font-mono text-2xs text-muted-foreground">
                  {search.query}
                </span>
              </button>
              <span className="flex items-center gap-1 opacity-60 group-hover:opacity-100">
                <input
                  type="color"
                  aria-label={`Colour for ${search.name}`}
                  defaultValue={search.icon ?? "#57d5ff"}
                  onBlur={(event) =>
                    update.mutate({ name: search.name, patch: { icon: event.target.value } })
                  }
                  className="h-6 w-7 cursor-pointer rounded border border-border bg-transparent"
                />
                <Button
                  variant="ghost"
                  size="icon-sm"
                  aria-label={pinned ? `Unpin ${search.name}` : `Pin ${search.name}`}
                  disabled={update.isPending}
                  onClick={() =>
                    update.mutate({ name: search.name, patch: { position: pinned ? 0 : -1 } })
                  }
                >
                  {pinned ? <PinOff className="size-3.5" /> : <Pin className="size-3.5" />}
                </Button>
                <Button
                  variant="ghost"
                  size="icon-sm"
                  aria-label={`Delete ${search.name}`}
                  onClick={() => setDeleting(search)}
                >
                  <Trash2 className="size-3.5 text-destructive" />
                </Button>
              </span>
            </li>
          );
        })}
      </ul>
      <AlertDialog open={deleting !== null} onOpenChange={(open) => !open && setDeleting(null)}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Delete “{deleting?.name}”?</AlertDialogTitle>
            <AlertDialogDescription>
              The saved search leaves the sidebar. No mail changes.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction
              variant="destructive"
              onClick={() => {
                if (deleting) remove.mutate(deleting.name);
                setDeleting(null);
              }}
            >
              Delete
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </>
  );
}
