import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import { FileText, Mail, Paperclip, Plus, RefreshCw, Trash2 } from "lucide-react";
import { toast } from "sonner";

import { deleteDraft, fetchDrafts, type DraftSummary } from "./api";
import { EmptyState } from "@/components/EmptyState";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
  AlertDialogTrigger,
} from "@/components/ui/alert-dialog";
import { Button } from "@/components/ui/button";

export function DraftsRoute() {
  const queryClient = useQueryClient();
  const drafts = useQuery({ queryKey: ["drafts"], queryFn: fetchDrafts });
  const remove = useMutation({
    mutationFn: deleteDraft,
    onSuccess: () => {
      toast.success("Draft deleted");
      void queryClient.invalidateQueries({ queryKey: ["drafts"] });
    },
    onError: (error) => toast.error("Delete failed", { description: error.message }),
  });

  if (drafts.isLoading) {
    return <div className="p-6 text-xs text-muted-foreground">Loading drafts...</div>;
  }
  if (drafts.isError) {
    return (
      <EmptyState
        icon={RefreshCw}
        title="Drafts unavailable"
        description={drafts.error.message}
        action={<Button onClick={() => drafts.refetch()}>Retry</Button>}
      />
    );
  }

  const rows = drafts.data?.drafts ?? [];
  return (
    <div className="flex min-w-0 flex-1 flex-col bg-background">
      <header className="flex items-center justify-between gap-4 border-b border-border px-6 py-4">
        <div>
          <div className="font-mono text-2xs uppercase tracking-wide text-muted-foreground">
            Local mailbox
          </div>
          <h1 className="text-xl font-semibold tracking-tight">Drafts</h1>
          <p className="mt-1 text-2xs text-muted-foreground">
            Drafts saved in mxr. Open one to edit or copy it to a supported mail provider.
          </p>
        </div>
        <Button asChild size="sm">
          <Link to="/compose/new">
            <Plus className="size-3.5" />
            New draft
          </Link>
        </Button>
      </header>

      {rows.length === 0 ? (
        <EmptyState
          icon={Mail}
          title="No saved drafts"
          description="Drafts persisted in mxr’s local store appear here."
        />
      ) : (
        <div className="divide-y divide-border" data-testid="draft-list">
          {rows.map((draft) => (
            <DraftRow
              key={draft.id}
              draft={draft}
              deleting={remove.isPending}
              onDelete={() => remove.mutate(draft.id)}
            />
          ))}
        </div>
      )}
    </div>
  );
}

function DraftRow({
  draft,
  deleting,
  onDelete,
}: {
  draft: DraftSummary;
  deleting: boolean;
  onDelete: () => void;
}) {
  const subject = draft.subject.trim() || "(no subject)";
  return (
    <div className="flex items-center pr-4 transition-colors hover:bg-muted/50">
      <Link
        to="/compose/$draftId"
        params={{ draftId: draft.id }}
        className="flex min-w-0 flex-1 items-center gap-3 px-6 py-4"
      >
        <FileText className="size-4 shrink-0 text-muted-foreground" />
        <div className="min-w-0 flex-1">
          <div className="truncate text-sm font-medium">{subject}</div>
          <div className="mt-1 truncate text-2xs text-muted-foreground">
            {draft.recipients || "No recipients"}
          </div>
        </div>
        <div className="flex shrink-0 items-center gap-3 text-2xs text-muted-foreground">
          {draft.content_kind === "html" ? (
            <span className="rounded bg-muted px-1.5 py-0.5 font-mono uppercase">HTML</span>
          ) : null}
          {draft.attachment_count > 0 ? (
            <span className="inline-flex items-center gap-1">
              <Paperclip className="size-3" />
              {draft.attachment_count}
            </span>
          ) : null}
          <time dateTime={draft.updated_at} title={draft.updated_at_full}>
            {draft.updated_at_relative}
          </time>
        </div>
      </Link>
      <AlertDialog>
        <AlertDialogTrigger asChild>
          <Button
            variant="ghost"
            size="icon-sm"
            disabled={deleting}
            aria-label={`Delete draft ${subject}`}
          >
            <Trash2 className="size-3.5" />
          </Button>
        </AlertDialogTrigger>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Delete this draft?</AlertDialogTitle>
            <AlertDialogDescription>
              “{subject}” will be permanently removed from mxr’s local draft store.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction variant="destructive" onClick={onDelete}>
              Delete draft
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  );
}
