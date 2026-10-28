import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import { Clock, FileText, Paperclip, Plus, Trash2 } from "lucide-react";
import type { MouseEvent } from "react";
import { toast } from "sonner";

import { deleteDraft, fetchDrafts, type DraftSummary } from "./api";
import { OrphanedDrafts } from "./OrphanedDrafts";
import { ScheduledSends } from "./ScheduledSends";
import { Page, PageSection } from "@/components/Page";
import { PageEmpty, PageError, PageSkeleton } from "@/components/PageParts";
import { Row } from "@/components/Row";
import { useLitRow, type LitRowProps } from "@/components/useLitRow";
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
import { formatLongDate } from "@/lib/format";
import { draftIntent, newMessageIntent, useComposeUi } from "@/features/compose/composeUiStore";
import { useHeldPending } from "@/hooks/useDelayedPending";

export function DraftsRoute() {
  const queryClient = useQueryClient();
  const drafts = useQuery({ queryKey: ["drafts"], queryFn: fetchDrafts });
  const draftsHeld = useHeldPending(drafts.isPending);
  const remove = useMutation({
    mutationFn: deleteDraft,
    onSuccess: () => {
      toast.success("Draft deleted");
      void queryClient.invalidateQueries({ queryKey: ["drafts"] });
    },
    onError: (error) => toast.error("Delete failed", { description: error.message }),
  });

  const rows = drafts.data?.drafts ?? [];
  const { rowProps } = useLitRow(rows.map((draft) => draft.id));
  return (
    <Page
      title="Drafts"
      description="Drafts saved in mxr. Open one to keep writing."
      primary={
        <Button
          size="sm"
          onClick={() => useComposeUi.getState().openCompose(newMessageIntent(), "overlay")}
        >
          <Plus className="size-3" />
          New draft
        </Button>
      }
    >
      <ScheduledSends />
      <OrphanedDrafts />
      <PageSection title="Drafts">
        {drafts.isPending || draftsHeld ? (
          <PageSkeleton rows={5} label="Loading drafts" />
        ) : drafts.isError ? (
          <PageError
            title="Drafts unavailable"
            error={drafts.error}
            onRetry={() => void drafts.refetch()}
          />
        ) : rows.length === 0 ? (
          <PageEmpty
            title="No saved drafts"
            body="Drafts persisted in mxr’s local store appear here."
          />
        ) : (
          <ul data-testid="draft-list" aria-label="Drafts">
            {rows.map((draft) => (
              <DraftRow
                key={draft.id}
                draft={draft}
                deleting={remove.isPending}
                onDelete={() => remove.mutate(draft)}
                rowProps={rowProps(draft.id)}
              />
            ))}
          </ul>
        )}
      </PageSection>
    </Page>
  );
}

function DraftRow({
  draft,
  deleting,
  onDelete,
  rowProps,
}: {
  draft: DraftSummary;
  deleting: boolean;
  onDelete: () => void;
  rowProps: LitRowProps;
}) {
  const subject = draft.subject.trim() || "(no subject)";
  return (
    <Row
      accountId={draft.account_id}
      {...rowProps}
      lead={<FileText aria-hidden className="size-4 text-text-3" />}
      primary={subject}
      secondary={draft.recipients || "No recipients"}
      renderMain={(content, className) => (
        <Link
          to="/compose/$draftId"
          params={{ draftId: draft.id }}
          onClick={(event: MouseEvent<HTMLAnchorElement>) => openDraftInPlace(event, draft.id)}
          className={className}
        >
          {content}
        </Link>
      )}
      trail={
        <>
          {draft.send_at ? (
            <span
              className="inline-flex items-center gap-1 text-signal"
              title={formatLongDate(draft.send_at)}
            >
              <Clock className="size-3.5" aria-hidden="true" />
              scheduled
            </span>
          ) : null}
          {draft.content_kind === "html" ? <span>HTML</span> : null}
          {draft.attachment_count > 0 ? (
            <span className="inline-flex items-center gap-1">
              <Paperclip className="size-3.5" />
              {draft.attachment_count}
            </span>
          ) : null}
        </>
      }
      time={
        <time dateTime={draft.updated_at} title={draft.updated_at_full}>
          {draft.updated_at_relative}
        </time>
      }
      verbs={
        <AlertDialog>
          <AlertDialogTrigger asChild>
            <button
              type="button"
              disabled={deleting}
              aria-label={`Delete draft ${subject}`}
              className="inline-flex h-7 items-center gap-1.5 px-1.5 text-danger hover:text-danger disabled:opacity-50"
            >
              <Trash2 aria-hidden className="size-3.5" />
              <span className="hidden sm:inline">Delete</span>
            </button>
          </AlertDialogTrigger>
          <AlertDialogContent yesNo>
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
      }
    />
  );
}

/** A plain click opens the draft in the compose surface over this list;
 * modified clicks keep the link's deep-link behaviour (new tab, etc.). */
function openDraftInPlace(event: MouseEvent<HTMLAnchorElement>, draftId: string) {
  if (event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) {
    return;
  }
  event.preventDefault();
  useComposeUi.getState().openCompose(draftIntent(draftId), "overlay");
}
