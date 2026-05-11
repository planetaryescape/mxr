import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import { FileText, Plus, RefreshCw, Trash2 } from "lucide-react";
import { toast } from "sonner";

import { deleteRule, fetchRules, upsertRuleForm, type RuleForm, type RuleListItem } from "./api";
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
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Switch } from "@/components/ui/switch";

export function RulesListRoute() {
  const qc = useQueryClient();
  const rules = useQuery({ queryKey: ["rules"], queryFn: fetchRules });
  const remove = useMutation({
    mutationFn: deleteRule,
    onSuccess: () => {
      toast.success("Rule deleted");
      void qc.invalidateQueries({ queryKey: ["rules"] });
    },
    onError: (error) => toast.error("Delete failed", { description: error.message }),
  });
  const toggle = useMutation({
    mutationFn: ({ rule, enabled }: { rule: RuleListItem; enabled: boolean }) =>
      upsertRuleForm({ ...ruleForm(rule), enabled }, ruleName(rule)),
    onSuccess: () => {
      toast.success("Rule updated");
      void qc.invalidateQueries({ queryKey: ["rules"] });
    },
    onError: (error) => toast.error("Update failed", { description: error.message }),
  });

  if (rules.isLoading)
    return <div className="p-6 text-xs text-muted-foreground">Loading rules...</div>;
  if (rules.isError)
    return (
      <EmptyState
        icon={RefreshCw}
        title="Rules unavailable"
        description={rules.error.message}
        action={<Button onClick={() => rules.refetch()}>Retry</Button>}
      />
    );
  const rows = rules.data?.rules ?? [];

  return (
    <div className="flex min-w-0 flex-1 flex-col bg-background">
      <header className="flex items-center gap-3 border-b border-border px-6 py-4">
        <div className="flex-1">
          <h1 className="text-xl font-semibold tracking-tight">Rules</h1>
          <p className="text-2xs text-muted-foreground">
            Deterministic mail automation. Save is separate from apply.
          </p>
        </div>
        <Button asChild>
          <Link to="/rules/$id" params={{ id: "new" }}>
            <Plus className="size-3" />
            New rule
          </Link>
        </Button>
      </header>
      {rows.length === 0 ? (
        <EmptyState
          icon={FileText}
          title="No rules yet"
          description="Create a rule, dry-run it, then enable it."
          action={
            <Button asChild>
              <Link to="/rules/$id" params={{ id: "new" }}>
                Create rule
              </Link>
            </Button>
          }
        />
      ) : (
        <div className="min-h-0 flex-1 overflow-auto p-4">
          <Card className="overflow-hidden">
            {rows.map((rule) => (
              <RuleRow
                key={ruleKey(rule)}
                rule={rule}
                onDelete={() => remove.mutate(ruleName(rule))}
                onToggle={(enabled) => toggle.mutate({ rule, enabled })}
                deleting={remove.isPending}
                toggling={toggle.isPending}
              />
            ))}
          </Card>
        </div>
      )}
    </div>
  );
}

function RuleRow({
  rule,
  onDelete,
  onToggle,
  deleting,
  toggling,
}: {
  rule: RuleListItem;
  onDelete: () => void;
  onToggle: (enabled: boolean) => void;
  deleting: boolean;
  toggling: boolean;
}) {
  const name = ruleName(rule);
  return (
    <div className="grid grid-cols-[1fr_auto] gap-3 border-b border-border px-4 py-3 last:border-b-0">
      <Link to="/rules/$id" params={{ id: name }} className="min-w-0">
        <div className="flex items-center gap-2">
          <span className="truncate text-sm font-medium">{name}</span>
          <Badge
            variant={rule.enabled === false ? "secondary" : "default"}
            className={rule.enabled === false ? "text-muted-foreground" : undefined}
          >
            {rule.enabled === false ? "disabled" : "enabled"}
          </Badge>
        </div>
        <div className="mt-1 truncate text-2xs text-muted-foreground">
          if {String(rule.condition ?? "...")} → {String(rule.action ?? "...")}
        </div>
        <div className="mt-1 font-mono text-2xs text-muted-foreground">
          priority {rule.priority ?? 0} · fired {rule.fire_count ?? 0}
        </div>
      </Link>
      <div className="flex items-center gap-2">
        <Switch
          checked={rule.enabled !== false}
          onCheckedChange={onToggle}
          disabled={toggling || !canPersistRule(rule)}
          aria-label={`${rule.enabled === false ? "Enable" : "Disable"} ${name}`}
        />
        <AlertDialog>
          <AlertDialogTrigger asChild>
            <Button variant="ghost" size="icon" disabled={deleting} aria-label={`Delete ${name}`}>
              <Trash2 className="size-3" />
            </Button>
          </AlertDialogTrigger>
          <AlertDialogContent>
            <AlertDialogHeader>
              <AlertDialogTitle>Delete {name}?</AlertDialogTitle>
              <AlertDialogDescription>
                This removes the rule definition. Already-applied mail changes are not undone.
              </AlertDialogDescription>
            </AlertDialogHeader>
            <AlertDialogFooter>
              <AlertDialogCancel disabled={deleting}>Cancel</AlertDialogCancel>
              <AlertDialogAction variant="destructive" disabled={deleting} onClick={onDelete}>
                Delete
              </AlertDialogAction>
            </AlertDialogFooter>
          </AlertDialogContent>
        </AlertDialog>
      </div>
    </div>
  );
}

function ruleName(rule: RuleListItem): string {
  return String(rule.name ?? rule.id ?? rule.rule ?? "unnamed");
}

function ruleKey(rule: RuleListItem): string {
  return `${ruleName(rule)}-${rule.priority ?? 0}`;
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
