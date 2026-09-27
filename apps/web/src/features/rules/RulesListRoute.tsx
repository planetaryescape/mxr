import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Link, useNavigate } from "@tanstack/react-router";
import { ListFilter, Plus, Trash2 } from "lucide-react";
import { useState } from "react";
import { toast } from "sonner";

import { deleteRule, fetchRules, upsertRuleForm, type RuleForm, type RuleListItem } from "./api";
import { Page } from "@/components/Page";
import { PageEmpty, PageError, PageSkeleton, RuledList, RuledRow } from "@/components/PageParts";
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
import { Switch } from "@/components/ui/switch";
import { plural } from "@/lib/format";

export function RulesListRoute() {
  const qc = useQueryClient();
  const navigate = useNavigate();
  const [confirmDelete, setConfirmDelete] = useState<string | null>(null);
  const rules = useQuery({ queryKey: ["rules"], queryFn: fetchRules });
  const remove = useMutation({
    mutationFn: deleteRule,
    onSuccess: (_result, name) => {
      toast.success(`Deleted ${name}`);
      setConfirmDelete(null);
      void qc.invalidateQueries({ queryKey: ["rules"] });
    },
    onError: (error) => toast.error("Delete failed", { description: error.message }),
  });
  const toggle = useMutation({
    mutationFn: ({ rule, enabled }: { rule: RuleListItem; enabled: boolean }) =>
      upsertRuleForm({ ...ruleForm(rule), enabled }, ruleName(rule)),
    onSuccess: (_result, { rule, enabled }) => {
      toast.success(`${ruleName(rule)} ${enabled ? "enabled" : "disabled"}`);
      void qc.invalidateQueries({ queryKey: ["rules"] });
    },
    onError: (error, { rule }) =>
      toast.error(`Could not update ${ruleName(rule)}`, { description: error.message }),
  });
  const rows = rules.data?.rules ?? [];

  return (
    <Page
      title="Rules"
      description="Automations the daemon runs during sync. Dry-run a rule before you trust it."
      actions={
        <Button size="sm" asChild>
          <Link to="/rules/$id" params={{ id: "new" }}>
            <Plus className="size-3" />
            New rule
          </Link>
        </Button>
      }
    >
      {rules.isPending ? (
        <PageSkeleton label="Loading rules" />
      ) : rules.isError ? (
        <PageError
          title="Rules unavailable"
          error={rules.error}
          onRetry={() => void rules.refetch()}
        />
      ) : rows.length === 0 ? (
        <PageEmpty
          icon={<ListFilter className="size-5" />}
          title="No rules yet"
          body="Write a condition, check the dry run, then save it."
          action={
            <Button size="sm" asChild>
              <Link to="/rules/$id" params={{ id: "new" }}>
                Create a rule
              </Link>
            </Button>
          }
        />
      ) : (
        <>
          <p className="mb-2 font-mono text-2xs text-muted-foreground">
            {plural(rows.length, "rule")}, lowest priority number runs first
          </p>
          <RuledList label="Rules">
            {rows.map((rule) => {
              const name = ruleName(rule);
              const enabled = rule.enabled !== false;
              return (
                <RuledRow
                  key={`${name}-${rule.priority ?? 0}`}
                  title={
                    <span className={enabled ? undefined : "text-muted-foreground"}>{name}</span>
                  }
                  meta={`if ${String(rule.condition ?? "?")} then ${String(rule.action ?? "?")}`}
                  aside={`priority ${rule.priority ?? 0}`}
                  onOpen={() => void navigate({ to: "/rules/$id", params: { id: name } })}
                  openLabel={`Edit ${name}`}
                  actions={
                    <>
                      <Switch
                        checked={enabled}
                        onCheckedChange={(next) => toggle.mutate({ rule, enabled: next })}
                        disabled={toggle.isPending || !canPersistRule(rule)}
                        aria-label={`${enabled ? "Disable" : "Enable"} ${name}`}
                      />
                      <Button
                        variant="ghost"
                        size="icon-sm"
                        disabled={remove.isPending}
                        aria-label={`Delete ${name}`}
                        onClick={() => setConfirmDelete(name)}
                      >
                        <Trash2 className="size-3.5" />
                      </Button>
                    </>
                  }
                />
              );
            })}
          </RuledList>
        </>
      )}
      <AlertDialog
        open={confirmDelete !== null}
        onOpenChange={(open) => !open && setConfirmDelete(null)}
      >
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Delete {confirmDelete}?</AlertDialogTitle>
            <AlertDialogDescription>
              The rule stops running. Mail it already changed stays as it is.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel disabled={remove.isPending}>Cancel</AlertDialogCancel>
            <AlertDialogAction
              variant="destructive"
              disabled={remove.isPending}
              onClick={(event) => {
                event.preventDefault();
                if (confirmDelete) remove.mutate(confirmDelete);
              }}
            >
              Delete rule
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </Page>
  );
}

function ruleName(rule: RuleListItem): string {
  return String(rule.name ?? rule.id ?? rule.rule ?? "unnamed");
}

function ruleForm(rule: RuleListItem): RuleForm {
  return {
    name: ruleName(rule),
    condition: String(rule.condition ?? ""),
    action: String(rule.action ?? ""),
    priority: Number(rule.priority ?? 100),
    enabled: rule.enabled !== false,
  };
}

function canPersistRule(rule: RuleListItem): boolean {
  const form = ruleForm(rule);
  return Boolean(form.name.trim() && form.condition.trim() && form.action.trim());
}
