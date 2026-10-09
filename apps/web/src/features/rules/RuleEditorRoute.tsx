import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useNavigate, useParams } from "@tanstack/react-router";
import { Check, FlaskConical, Play, Trash2 } from "lucide-react";
import { useEffect, useState, type ReactNode } from "react";
import { toast } from "sonner";

import {
  deleteRule,
  dryRunRule,
  fetchRuleForm,
  fetchRuleHistory,
  upsertRuleForm,
  previewTreatment,
  type RuleForm,
} from "./api";
import { mailActions, runMailActions } from "./ruleActions";
import { Page, PageSection } from "@/components/Page";
import {
  PageEmpty,
  PageError,
  PageNote,
  PageSkeleton,
  RuledList,
  RuledRow,
} from "@/components/PageParts";
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
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import { shellKey, undoMutation } from "@/features/mailbox/api";
import { fetchAccounts } from "@/features/accounts/api";
import { fetchSearch } from "@/features/search/api";
import { formatRelative, plural } from "@/lib/format";

const emptyRule: RuleForm = {
  name: "",
  condition: "",
  action: "archive",
  priority: 100,
  enabled: true,
};

const ACTION_PRESETS = [
  "treatment:messages",
  "treatment:updates",
  "treatment:reading",
  "archive",
  "mark-read,archive",
  "label:Receipts",
  "move:Archive",
  "star",
  "trash",
  "spam",
];

/** A message the rule would touch; the same ids Apply now acts on. */
interface PreviewMatch {
  message_id: string;
  from: string;
  subject: string;
  before?: string;
  after?: string;
  reason?: string;
  blocked?: boolean;
}

export function RuleEditorRoute() {
  const { id } = useParams({ from: "/rules/$id" });
  const isNew = id === "new";
  const formQuery = useQuery({
    queryKey: ["rule-form", id],
    queryFn: () => fetchRuleForm(id),
    enabled: !isNew,
    retry: false,
  });

  if (!isNew && formQuery.isPending)
    return (
      <Page title="Rule" eyebrow="Rules" width="default">
        <PageSkeleton label="Loading rule" />
      </Page>
    );
  if (!isNew && formQuery.isError)
    return (
      <Page title="Rule" eyebrow="Rules">
        <PageError
          title="Rule unavailable"
          error={formQuery.error}
          onRetry={() => void formQuery.refetch()}
        />
      </Page>
    );
  return <RuleEditor key={id} id={id} saved={isNew ? null : (formQuery.data?.form ?? null)} />;
}

function RuleEditor({ id, saved }: { id: string; saved: RuleForm | null }) {
  const isNew = saved === null;
  const navigate = useNavigate();
  const qc = useQueryClient();
  const [form, setForm] = useState<RuleForm>(saved ?? emptyRule);
  const [applyConfirmOpen, setApplyConfirmOpen] = useState(false);
  const condition = useDebounced(form.condition.trim(), 350);
  const isSorting = form.action
    .split(/[;,]/)
    .some((action) => action.trim().toLowerCase().startsWith("treatment:"));
  const sortingForm = useDebounced(form, 350);
  const sortingDraftPending = sortingForm !== form;
  const accounts = useQuery({ queryKey: ["rule-accounts"], queryFn: fetchAccounts });
  const sortingPreview = useQuery({
    queryKey: ["rule-treatment-preview", sortingForm],
    queryFn: () => previewTreatment(sortingForm),
    enabled:
      isSorting &&
      Boolean(sortingForm.account_id && sortingForm.name.trim() && sortingForm.condition.trim()),
    retry: false,
  });
  // The daemon's dry run evaluates the saved rule. While the condition is
  // unsaved (new rule, or edited), preview with the same query as a search.
  const usesSavedRule = !isNew && condition === saved.condition.trim();

  const preview = useQuery({
    queryKey: ["rule-preview", usesSavedRule ? `saved:${id}` : `query:${condition}`],
    queryFn: async (): Promise<PreviewMatch[]> => {
      if (usesSavedRule) {
        const response = await dryRunRule(id);
        return response.results.flatMap((result) => result.matches);
      }
      const search = await fetchSearch({ q: condition, limit: 50, scope: "messages" });
      return search.groups
        .flatMap((group) => group.rows)
        .map((row) => ({ message_id: row.id, from: row.sender, subject: row.subject }));
    },
    enabled: condition.length > 0 && !isSorting,
  });
  const history = useQuery({
    queryKey: ["rule-history", id],
    queryFn: () => fetchRuleHistory(id),
    enabled: !isNew,
  });
  const save = useMutation({
    mutationFn: (submitted: RuleForm) => upsertRuleForm(submitted, isNew ? null : id),
    onSuccess: async (_, submitted) => {
      toast.success(`Saved ${submitted.name}`);
      void qc.invalidateQueries({ queryKey: ["rules"] });
      void qc.invalidateQueries({ queryKey: ["rule-form"] });
      void qc.invalidateQueries({ queryKey: ["rule-preview"] });
      void qc.invalidateQueries({ queryKey: ["rule-treatment-preview"] });
      if (isNew || submitted.name !== id)
        await navigate({ to: "/rules/$id", params: { id: submitted.name } });
    },
    onError: (error) => toast.error("Save failed", { description: error.message }),
  });
  const remove = useMutation({
    mutationFn: () => deleteRule(id),
    onSuccess: async () => {
      toast.success(`Deleted ${id}`);
      void qc.invalidateQueries({ queryKey: ["rules"] });
      await navigate({ to: "/rules" });
    },
    onError: (error) => toast.error("Delete failed", { description: error.message }),
  });

  const shownPreview = isSorting ? sortingPreview : preview;
  const matches: PreviewMatch[] = isSorting
    ? (sortingPreview.data?.result.matches ?? [])
    : (preview.data ?? []);
  const parsedActions = mailActions(form.action);
  const applySorting = useMutation({
    mutationFn: async () => {
      const token = sortingPreview.data?.token;
      if (sortingDraftPending) throw new Error("Wait for the current draft preview");
      if (!token) throw new Error("Preview this sorting rule first");
      return previewTreatment(form, token);
    },
    onSuccess: async (response) => {
      setApplyConfirmOpen(false);
      toast.success(`Sorted ${plural(response.result.matches.length, "message")}`);
      void qc.invalidateQueries({ queryKey: ["rules"] });
      void qc.invalidateQueries({ queryKey: ["rule-form"] });
      void qc.invalidateQueries({ queryKey: ["rule-treatment-preview"] });
      void qc.invalidateQueries({ queryKey: shellKey });
      if (response.rule_id) await navigate({ to: "/rules/$id", params: { id: response.rule_id } });
    },
    onError: (error) => toast.error("Sorting failed", { description: error.message }),
  });
  const applyNow = useMutation({
    mutationFn: async () => {
      if (!parsedActions) throw new Error("This action cannot be applied from the web yet");
      const ids = matches.map((match) => match.message_id);
      if (ids.length === 0) throw new Error("The preview has no messages to apply this rule to");
      return runMailActions(parsedActions, ids);
    },
    onSuccess: (response) => {
      setApplyConfirmOpen(false);
      const count = response.result?.succeeded ?? 0;
      const mutationId = response.result?.mutation_id;
      toast.success(`Applied to ${plural(count, "message")}`, {
        duration: mutationId ? 60_000 : undefined,
        action: mutationId
          ? {
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
            }
          : undefined,
      });
    },
    onError: (error) => toast.error("Apply failed", { description: error.message }),
    onSettled: () => {
      void qc.invalidateQueries({ queryKey: ["mailbox"] });
      void qc.invalidateQueries({ queryKey: shellKey });
      void qc.invalidateQueries({ queryKey: ["rule-preview"] });
      void qc.invalidateQueries({ queryKey: ["rule-treatment-preview"] });
    },
  });

  const canSave = Boolean(
    form.name.trim() &&
    form.condition.trim() &&
    form.action.trim() &&
    (!isSorting || form.account_id),
  );

  return (
    <Page
      eyebrow="Rules"
      title={isNew ? "New rule" : saved.name}
      description="Preview what a rule matches before you save or apply it."
      actions={
        <>
          {!isNew ? (
            <AlertDialog>
              <AlertDialogTrigger asChild>
                <Button variant="ghost" size="sm" disabled={remove.isPending}>
                  <Trash2 className="size-3" />
                  Delete
                </Button>
              </AlertDialogTrigger>
              <AlertDialogContent>
                <AlertDialogHeader>
                  <AlertDialogTitle>Delete {saved.name}?</AlertDialogTitle>
                  <AlertDialogDescription>
                    The rule stops running. Mail it already changed stays as it is.
                  </AlertDialogDescription>
                </AlertDialogHeader>
                <AlertDialogFooter>
                  <AlertDialogCancel>Cancel</AlertDialogCancel>
                  <AlertDialogAction variant="destructive" onClick={() => remove.mutate()}>
                    Delete rule
                  </AlertDialogAction>
                </AlertDialogFooter>
              </AlertDialogContent>
            </AlertDialog>
          ) : null}
          <Button
            size="sm"
            onClick={() => save.mutate(form)}
            disabled={!canSave || save.isPending || applySorting.isPending || applyNow.isPending}
          >
            <Check className="size-3" />
            {isNew ? "Save rule" : "Save changes"}
          </Button>
        </>
      }
    >
      <div className="grid gap-x-10 lg:grid-cols-[minmax(0,22rem)_minmax(0,1fr)]">
        <PageSection title="Definition">
          <fieldset
            className="space-y-4"
            disabled={save.isPending || applySorting.isPending || applyNow.isPending}
          >
            <Field
              label="Account"
              htmlFor="rule-account"
              hint="Sorting requires one account. All actions in this rule use this scope."
            >
              <select
                id="rule-account"
                value={form.account_id ?? ""}
                onChange={(event) => setForm({ ...form, account_id: event.target.value || null })}
              >
                <option value="" disabled={Boolean(saved?.account_id)}>
                  All accounts
                </option>
                {accounts.data?.accounts.map((account) => (
                  <option key={account.account_id} value={account.account_id}>
                    {account.name}
                  </option>
                ))}
              </select>
            </Field>
            <Field label="Name" htmlFor="rule-name">
              <Input
                id="rule-name"
                value={form.name}
                onChange={(event) => setForm({ ...form, name: event.target.value })}
                placeholder="Archive newsletters"
                className="h-8 text-[13px]"
              />
            </Field>
            <Field
              label="When a message matches"
              htmlFor="rule-condition"
              hint={
                isSorting
                  ? "Header conditions, e.g. from:news@example.com subject:weekly. Body and link-density conditions are unsupported."
                  : "Search syntax, e.g. from:news@example.com older_than:7d"
              }
            >
              <Input
                id="rule-condition"
                value={form.condition}
                onChange={(event) => setForm({ ...form, condition: event.target.value })}
                placeholder="from:news@example.com"
                className="h-8 font-mono text-xs"
              />
            </Field>
            <Field label="Do" htmlFor="rule-action" hint="Comma-separated steps, run in order.">
              <Input
                id="rule-action"
                value={form.action}
                onChange={(event) => setForm({ ...form, action: event.target.value })}
                placeholder="mark-read,archive"
                className="h-8 font-mono text-xs"
              />
              <div className="mt-2 flex flex-wrap gap-1.5">
                {ACTION_PRESETS.map((action) => (
                  <Button
                    key={action}
                    type="button"
                    variant={form.action === action ? "secondary" : "outline"}
                    size="xs"
                    className="font-mono"
                    onClick={() => setForm({ ...form, action })}
                  >
                    {action.startsWith("treatment:")
                      ? `Sort into ${action.slice(10).replace(/^./, (letter) => letter.toUpperCase())}`
                      : action}
                  </Button>
                ))}
              </div>
            </Field>
            <Field label="Priority" htmlFor="rule-priority" hint="Lower runs first.">
              <Input
                id="rule-priority"
                type="number"
                value={form.priority}
                onChange={(event) => setForm({ ...form, priority: Number(event.target.value) })}
                className="h-8 w-24 text-xs"
              />
            </Field>
            <div className="flex items-center justify-between gap-4 border-t border-border/60 pt-3">
              <Label htmlFor="rule-enabled" className="text-[13px] font-normal">
                Run during sync
              </Label>
              <Switch
                id="rule-enabled"
                checked={form.enabled}
                onCheckedChange={(enabled) => setForm({ ...form, enabled })}
              />
            </div>
          </fieldset>
        </PageSection>
        <div className="min-w-0">
          <PageSection
            title="Dry run"
            description={
              condition.length === 0
                ? undefined
                : isSorting
                  ? "See where matching mail will go. Your manual choices are preserved."
                  : usesSavedRule
                    ? "The daemon's dry run of the saved rule."
                    : "Messages matching the unsaved condition. Save to run the daemon's dry run."
            }
            actions={
              <Button
                variant="outline"
                size="sm"
                onClick={() => setApplyConfirmOpen(true)}
                disabled={
                  (isSorting
                    ? !sortingPreview.data?.token ||
                      sortingPreview.isFetching ||
                      sortingDraftPending
                    : !parsedActions) ||
                  matches.length === 0 ||
                  applyNow.isPending ||
                  applySorting.isPending ||
                  save.isPending
                }
              >
                <Play className="size-3" />
                Apply to {plural(matches.length, "message")}
              </Button>
            }
          >
            {isSorting && sortingPreview.data ? (
              <PageNote>
                {sortingPreview.data.result.complete
                  ? "Complete selection."
                  : `Sample of the newest ${sortingPreview.data.result.scan_limit} messages; older mail is excluded.`}
                {` ${sortingPreview.data.result.notice}`}
              </PageNote>
            ) : null}
            {!isSorting && !parsedActions && form.action.trim() ? (
              <PageNote>
                Apply now supports archive, trash, spam, star, read, unread, label:Name,
                unlabel:Name and move:Name. The daemon still runs other actions during sync.
              </PageNote>
            ) : null}
            {condition.length === 0 ? (
              <PageEmpty
                icon={<FlaskConical className="size-5" />}
                title="Nothing to preview"
                body="Write a condition to see which messages this rule would change."
              />
            ) : shownPreview.isPending ? (
              <PageSkeleton rows={5} label="Running dry run" />
            ) : shownPreview.isError ? (
              <PageError
                title="Dry run failed"
                error={shownPreview.error}
                onRetry={() => void shownPreview.refetch()}
              />
            ) : matches.length === 0 ? (
              <PageEmpty title="No matches" body="This rule would not change any message now." />
            ) : (
              <>
                <p className="mb-2 font-mono text-2xs text-muted-foreground">
                  {plural(matches.length, "message")} would get: {form.action || "no action"}
                </p>
                <RuledList label="Dry run matches">
                  {(isSorting ? matches : matches.slice(0, 50)).map((match) => (
                    <RuledRow
                      key={match.message_id}
                      title={match.subject.trim() || "(no subject)"}
                      meta={`${match.from}${match.after ? ` · ${match.before} → ${match.after} · ${match.reason}${match.blocked ? " · personal choice preserved" : ""}` : ""}`}
                    />
                  ))}
                </RuledList>
              </>
            )}
          </PageSection>
          {!isNew ? (
            <PageSection title="History" description="Recent runs of this rule.">
              {history.isPending ? (
                <PageSkeleton rows={3} label="Loading history" />
              ) : history.isError ? (
                <PageError
                  title="History unavailable"
                  error={history.error}
                  onRetry={() => void history.refetch()}
                />
              ) : history.data.entries.length === 0 ? (
                <p className="text-[13px] text-muted-foreground">No action history recorded.</p>
              ) : (
                <RuledList label="Rule history">
                  {history.data.entries.map((entry) => (
                    <RuledRow
                      key={`${entry.timestamp}-${entry.message_id}`}
                      title={entry.actions_applied.join(", ") || "no actions"}
                      meta={`message ${entry.message_id.slice(0, 8)}${entry.error ? ` · ${entry.error}` : ""}`}
                      aside={
                        <span className={entry.success ? undefined : "text-destructive"}>
                          {entry.success ? formatRelative(entry.timestamp) : "failed"}
                        </span>
                      }
                    />
                  ))}
                </RuledList>
              )}
            </PageSection>
          ) : null}
        </div>
      </div>
      <AlertDialog open={applyConfirmOpen} onOpenChange={setApplyConfirmOpen}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Apply to {plural(matches.length, "message")}?</AlertDialogTitle>
            <AlertDialogDescription>
              {isSorting ? (
                "Saves this rule and applies only its sorting treatment to the unchanged preview. Personal choices are preserved."
              ) : (
                <>
                  Runs <span className="font-mono">{form.action}</span> on exactly the messages in
                  the dry run, through the same path as mailbox bulk actions. You can undo from the
                  toast.
                </>
              )}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel
              disabled={
                applyNow.isPending ||
                applySorting.isPending ||
                sortingPreview.isFetching ||
                sortingDraftPending
              }
            >
              Cancel
            </AlertDialogCancel>
            <AlertDialogAction
              disabled={
                applyNow.isPending ||
                applySorting.isPending ||
                sortingPreview.isFetching ||
                sortingDraftPending
              }
              onClick={(event) => {
                event.preventDefault();
                if (isSorting) applySorting.mutate();
                else applyNow.mutate();
              }}
            >
              Apply now
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </Page>
  );
}

function useDebounced<T>(value: T, ms: number): T {
  const [debounced, setDebounced] = useState(value);
  useEffect(() => {
    const timer = setTimeout(() => setDebounced(value), ms);
    return () => clearTimeout(timer);
  }, [value, ms]);
  return debounced;
}

function Field({
  label,
  htmlFor,
  hint,
  children,
}: {
  label: string;
  htmlFor: string;
  hint?: string;
  children: ReactNode;
}) {
  return (
    <div className="space-y-1">
      <Label htmlFor={htmlFor} className="text-xs">
        {label}
      </Label>
      {children}
      {hint ? <p className="text-2xs text-muted-foreground">{hint}</p> : null}
    </div>
  );
}
