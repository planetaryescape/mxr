import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useNavigate, useParams } from "@tanstack/react-router";
import { Check, Play, RefreshCw } from "lucide-react";
import { useEffect, useState } from "react";
import { toast } from "sonner";

import { dryRunRule, fetchRuleForm, fetchRuleHistory, upsertRuleForm, type RuleForm } from "./api";
import { fetchSearch } from "@/features/search/api";
import {
  archiveMessages,
  markReadMessages,
  modifyLabels,
  moveMessagesToLabel,
  readAndArchiveMessages,
  shellKey,
  spamMessages,
  starMessages,
  trashMessages,
  undoMutation,
} from "@/features/mailbox/api";
import type { MutationResponse } from "@/features/mailbox/types";
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
} from "@/components/ui/alert-dialog";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";

const emptyRule: RuleForm = {
  name: "",
  condition: "",
  action: "archive",
  priority: 100,
  enabled: true,
};
type RulePreview = { results: unknown[] };

export function RuleEditorRoute() {
  const { id } = useParams({ from: "/rules/$id" });
  const isNew = id === "new";
  const navigate = useNavigate();
  const qc = useQueryClient();
  const formQuery = useQuery({
    queryKey: ["rule-form", id],
    queryFn: () => fetchRuleForm(id),
    enabled: !isNew,
    retry: false,
  });
  const [form, setForm] = useState<RuleForm>(emptyRule);
  const [applyConfirmOpen, setApplyConfirmOpen] = useState(false);

  useEffect(() => {
    if (formQuery.data?.form) setForm(formQuery.data.form);
    if (isNew) setForm(emptyRule);
  }, [formQuery.data?.form, isNew]);

  const dryRun = useQuery({
    queryKey: ["rule-dry-run", isNew ? form.condition : id, form.condition],
    queryFn: async (): Promise<RulePreview> => {
      if (!isNew) return dryRunRule(id);
      const search = await fetchSearch({ q: form.condition, limit: 20 });
      return { results: search.groups.flatMap((group) => group.rows) };
    },
    enabled: (isNew ? form.condition : id).trim().length > 0,
  });
  const history = useQuery({
    queryKey: ["rule-history", id],
    queryFn: () => fetchRuleHistory(id),
    enabled: !isNew,
  });
  const save = useMutation({
    mutationFn: () => upsertRuleForm(form, isNew ? null : id),
    onSuccess: async () => {
      toast.success("Rule saved");
      void qc.invalidateQueries({ queryKey: ["rules"] });
      if (isNew) await navigate({ to: "/rules" });
    },
    onError: (error) => toast.error("Save failed", { description: error.message }),
  });
  const applyNow = useMutation({
    mutationFn: async () => {
      const action = mailActions(form.action);
      const ids: string[] = [];
      for (const row of previewRows) {
        const previewId = messageId(row);
        if (previewId) ids.push(previewId);
      }
      if (!action) throw new Error("This action is not supported by apply-now yet");
      if (ids.length === 0) throw new Error("No preview messages to apply this rule to");
      return runMailActions(action, ids);
    },
    onSuccess: (response) => {
      setApplyConfirmOpen(false);
      if (!response) return;
      const count = response.result?.succeeded ?? 0;
      const mutationId = response.result?.mutation_id;
      if (mutationId) {
        toast.success(`Applied rule to ${count} messages`, {
          duration: 60_000,
          action: {
            label: "Undo",
            onClick: () => {
              undoMutation(mutationId)
                .then(() => {
                  toast.success("Rule application undone");
                  void qc.invalidateQueries({ queryKey: ["mailbox"] });
                  void qc.invalidateQueries({ queryKey: shellKey });
                })
                .catch((error: Error) =>
                  toast.error("Undo failed", { description: error.message }),
                );
            },
          },
        });
      } else {
        toast.success(`Applied rule to ${count} messages`);
      }
    },
    onError: (error) => toast.error("Apply failed", { description: error.message }),
    onSettled: () => {
      void qc.invalidateQueries({ queryKey: ["mailbox"] });
      void qc.invalidateQueries({ queryKey: shellKey });
    },
  });

  if (formQuery.isError && !isNew)
    return (
      <EmptyState icon={RefreshCw} title="Rule unavailable" description={formQuery.error.message} />
    );
  const previewRows = dryRun.data?.results ?? [];

  return (
    <div className="grid min-w-0 flex-1 grid-cols-1 bg-background lg:grid-cols-[minmax(360px,520px)_1fr]">
      <section className="border-r border-border p-6">
        <div className="mb-5">
          <div className="font-mono text-2xs uppercase tracking-wide text-muted-foreground">
            Rule builder
          </div>
          <h1 className="text-xl font-semibold tracking-tight">
            {isNew ? "New rule" : form.name || id}
          </h1>
        </div>
        <div className="space-y-4">
          <Field label="Name">
            <Input
              value={form.name}
              onChange={(event) => setForm({ ...form, name: event.target.value })}
              placeholder="Archive newsletters"
            />
          </Field>
          <Field label="Condition">
            <Input
              value={form.condition}
              onChange={(event) => setForm({ ...form, condition: event.target.value })}
              placeholder="from:news@example.com"
            />
          </Field>
          <Field label="Action">
            <Input
              value={form.action}
              onChange={(event) => setForm({ ...form, action: event.target.value })}
              placeholder="mark-read,archive or label:News,archive"
            />
            <div className="mt-2 flex flex-wrap gap-1.5">
              {[
                "mark-read,archive",
                "archive",
                "trash",
                "spam",
                "star",
                "read",
                "unread",
                "read-and-archive",
                "label:Receipts",
                "move:Archive",
              ].map((action) => (
                <Button
                  key={action}
                  type="button"
                  variant={form.action === action ? "default" : "outline"}
                  size="sm"
                  onClick={() => setForm({ ...form, action })}
                >
                  {action}
                </Button>
              ))}
            </div>
          </Field>
          <Field label="Priority">
            <Input
              type="number"
              value={form.priority}
              onChange={(event) => setForm({ ...form, priority: Number(event.target.value) })}
            />
          </Field>
          <div className="flex items-center justify-between rounded-lg border border-border px-3 py-2">
            <div>
              <div className="text-xs font-medium">Enabled</div>
              <div className="text-2xs text-muted-foreground">
                Daemon can run this rule during sync.
              </div>
            </div>
            <Switch
              checked={form.enabled}
              onCheckedChange={(enabled) => setForm({ ...form, enabled })}
            />
          </div>
          <Button
            onClick={() => save.mutate()}
            disabled={
              save.isPending || !form.name.trim() || !form.condition.trim() || !form.action.trim()
            }
          >
            <Check className="size-3" />
            Save rule
          </Button>
          <Button
            variant="outline"
            onClick={() => setApplyConfirmOpen(true)}
            disabled={!mailActions(form.action) || previewRows.length === 0 || applyNow.isPending}
          >
            <Play className="size-3" />
            Apply preview now
          </Button>
          <AlertDialog open={applyConfirmOpen} onOpenChange={setApplyConfirmOpen}>
            <AlertDialogContent>
              <AlertDialogHeader>
                <AlertDialogTitle>Apply {form.action} to preview?</AlertDialogTitle>
                <AlertDialogDescription>
                  This will mutate {previewRows.length} preview messages using the same path as
                  mailbox bulk actions.
                </AlertDialogDescription>
              </AlertDialogHeader>
              <AlertDialogFooter>
                <AlertDialogCancel disabled={applyNow.isPending}>Cancel</AlertDialogCancel>
                <AlertDialogAction
                  disabled={applyNow.isPending}
                  onClick={(event) => {
                    event.preventDefault();
                    applyNow.mutate();
                  }}
                >
                  Apply now
                </AlertDialogAction>
              </AlertDialogFooter>
            </AlertDialogContent>
          </AlertDialog>
          {!mailActions(form.action) ? (
            <div className="text-2xs text-muted-foreground">
              Apply-now supports ordered chains of archive, trash, spam, star, read, unread,
              label:Name, and move:Name.
            </div>
          ) : null}
        </div>
      </section>
      <section className="min-h-0 overflow-auto p-6">
        <div className="grid gap-4 xl:grid-cols-2">
          <Panel title="Always-visible dry-run">
            {dryRun.isLoading ? (
              <div className="text-xs text-muted-foreground">Running preview...</div>
            ) : previewRows.length === 0 ? (
              <div className="text-xs text-muted-foreground">No matches yet.</div>
            ) : (
              <pre className="max-h-[52vh] overflow-auto rounded-md bg-muted p-3 text-2xs">
                {JSON.stringify(previewRows.slice(0, 20), null, 2)}
              </pre>
            )}
          </Panel>
          <Panel title="History">
            {isNew ? (
              <div className="text-xs text-muted-foreground">
                Save first to collect run history.
              </div>
            ) : (
              <pre className="max-h-[52vh] overflow-auto rounded-md bg-muted p-3 text-2xs">
                {JSON.stringify(history.data?.entries ?? [], null, 2)}
              </pre>
            )}
          </Panel>
        </div>
      </section>
    </div>
  );
}

type SupportedRuleAction =
  | { kind: "archive" }
  | { kind: "trash" }
  | { kind: "spam" }
  | { kind: "star" }
  | { kind: "read" }
  | { kind: "unread" }
  | { kind: "read-and-archive" }
  | { kind: "label-add"; label: string }
  | { kind: "label-remove"; label: string }
  | { kind: "move"; label: string };

function mailActions(value: string): SupportedRuleAction[] | null {
  const actions = value
    .split(/[;,]/)
    .map((part) => part.trim())
    .filter(Boolean)
    .map(mailAction);
  if (actions.length === 0 || actions.some((action) => action === null)) return null;
  return actions as SupportedRuleAction[];
}

function mailAction(value: string): SupportedRuleAction | null {
  const normalized = value.trim();
  const lower = normalized.toLowerCase();
  if (lower === "archive") return { kind: "archive" };
  if (lower === "trash") return { kind: "trash" };
  if (lower === "spam") return { kind: "spam" };
  if (lower === "star") return { kind: "star" };
  if (lower === "read" || lower === "mark-read" || lower === "mark_read")
    return { kind: "read" };
  if (lower === "unread" || lower === "mark-unread" || lower === "mark_unread")
    return { kind: "unread" };
  if (lower === "read-and-archive" || lower === "read_and_archive")
    return { kind: "read-and-archive" };
  const labelMatch = normalized.match(/^(?:add-label|label):(.+)$/i);
  if (labelMatch && labelMatch[1]?.trim()) {
    return { kind: "label-add", label: labelMatch[1].trim() };
  }
  const removeLabelMatch = normalized.match(/^(?:remove-label|unlabel):(.+)$/i);
  if (removeLabelMatch && removeLabelMatch[1]?.trim()) {
    return { kind: "label-remove", label: removeLabelMatch[1].trim() };
  }
  const moveMatch = normalized.match(/^move:(.+)$/i);
  if (moveMatch && moveMatch[1]?.trim()) {
    return { kind: "move", label: moveMatch[1].trim() };
  }
  return null;
}

async function runMailActions(actions: SupportedRuleAction[], ids: string[]): Promise<MutationResponse> {
  if (actions.length === 2 && actions[0]?.kind === "read" && actions[1]?.kind === "archive") {
    return readAndArchiveMessages(ids);
  }
  let last: MutationResponse | null = null;
  for (const action of actions) {
    last = await runMailAction(action, ids);
  }
  if (!last) throw new Error("No actions to apply");
  return last;
}

function runMailAction(action: SupportedRuleAction, ids: string[]): Promise<MutationResponse> {
  switch (action.kind) {
    case "archive":
      return archiveMessages(ids);
    case "trash":
      return trashMessages(ids);
    case "spam":
      return spamMessages(ids);
    case "star":
      return starMessages(ids, true);
    case "read":
      return markReadMessages(ids, true);
    case "unread":
      return markReadMessages(ids, false);
    case "read-and-archive":
      return readAndArchiveMessages(ids);
    case "label-add":
      return modifyLabels(ids, [action.label], []);
    case "label-remove":
      return modifyLabels(ids, [], [action.label]);
    case "move":
      return moveMessagesToLabel(ids, action.label);
  }
}

function messageId(row: unknown): string | undefined {
  if (!row || typeof row !== "object") return undefined;
  const candidate = row as Record<string, unknown>;
  for (const key of ["id", "message_id", "messageId"]) {
    if (typeof candidate[key] === "string") return candidate[key];
  }
  const message = candidate.message;
  if (
    message &&
    typeof message === "object" &&
    typeof (message as Record<string, unknown>).id === "string"
  ) {
    return (message as Record<string, unknown>).id as string;
  }
  return undefined;
}

function Field({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="space-y-1">
      <Label>{label}</Label>
      {children}
    </div>
  );
}

function Panel({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <Card>
      <CardHeader className="pb-3">
        <CardTitle>{title}</CardTitle>
      </CardHeader>
      <CardContent>{children}</CardContent>
    </Card>
  );
}
