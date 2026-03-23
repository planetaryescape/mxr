import { HeaderActionButton } from "./shared";
import { formatJson, stringField } from "./formatters";

export function RulesWorkspace(props: {
  rules: Array<Record<string, unknown>>;
  selectedRuleId: string | null;
  panelMode: "details" | "history" | "dryRun";
  detail: Record<string, unknown> | null;
  history: Array<Record<string, unknown>>;
  dryRun: Array<Record<string, unknown>>;
  status: string | null;
  onSelect: (ruleId: string) => void;
  onNew: () => void;
  onEdit: () => void;
  onToggle: () => void;
  onHistory: () => void;
  onDryRun: () => void;
  onDelete: () => void;
}) {
  return (
    <div className="grid h-full min-h-0 grid-cols-1 xl:grid-cols-[22rem_minmax(0,1fr)]">
      <section className="subtle-scrollbar min-h-0 overflow-y-auto border-r border-outline bg-panel px-4 py-4">
        <div className="flex items-center justify-between gap-3 border-b border-outline pb-3">
          <div>
            <p className="mono-meta">Rules</p>
            <h1 className="mt-2 text-2xl font-semibold tracking-tight text-foreground">Rules</h1>
          </div>
          <HeaderActionButton label="New" onClick={props.onNew} />
        </div>
        {props.status ? (
          <div className="mt-3 border border-outline bg-panel-elevated px-3 py-2 text-sm text-foreground-muted">
            {props.status}
          </div>
        ) : null}
        <div className="mt-3 space-y-px">
          {props.rules.length === 0 ? (
            <div className="border border-outline bg-panel-elevated px-3 py-3 text-sm text-foreground-muted">
              No rules yet.
            </div>
          ) : (
            props.rules.map((rule, index) => {
              const ruleId = String(rule.id ?? rule.name ?? index);
              return (
                <button
                  key={ruleId}
                  type="button"
                  className="w-full border border-transparent px-3 py-2.5 text-left data-[selected=true]:border-outline-strong data-[selected=true]:bg-panel-elevated"
                  data-selected={props.selectedRuleId === ruleId}
                  onClick={() => props.onSelect(ruleId)}
                >
                  <div className="flex items-center justify-between gap-3">
                    <h2 className="text-sm font-medium text-foreground">
                      {stringField(rule.name) ?? `Rule ${index + 1}`}
                    </h2>
                    <span className="font-mono text-[10px] uppercase tracking-[0.12em] text-foreground-subtle">
                      {String(rule.enabled ?? "unknown")}
                    </span>
                  </div>
                  {stringField(rule.condition) ? (
                    <p className="mt-1 line-clamp-2 text-[12px] leading-5 text-foreground-muted">
                      {stringField(rule.condition)}
                    </p>
                  ) : null}
                </button>
              );
            })
          )}
        </div>
      </section>
      <section className="subtle-scrollbar min-h-0 overflow-y-auto bg-panel-muted px-4 py-4">
        <div className="flex flex-wrap items-start justify-between gap-3 border-b border-outline pb-3">
          <div>
            <p className="mono-meta">
              {props.panelMode === "history"
                ? "Rule history"
                : props.panelMode === "dryRun"
                  ? "Rule dry run"
                  : "Rule details"}
            </p>
            <h2 className="mt-2 text-2xl font-semibold tracking-tight text-foreground">
              {stringField(props.detail?.name) ?? "Select a rule"}
            </h2>
          </div>
          <div className="flex flex-wrap gap-2">
            <HeaderActionButton
              label="Edit"
              disabled={!props.selectedRuleId}
              onClick={props.onEdit}
            />
            <HeaderActionButton
              label="Toggle"
              disabled={!props.selectedRuleId}
              onClick={props.onToggle}
            />
            <HeaderActionButton
              label="History"
              disabled={!props.selectedRuleId}
              onClick={props.onHistory}
            />
            <HeaderActionButton
              label="Dry run"
              disabled={!props.selectedRuleId}
              onClick={props.onDryRun}
            />
            <HeaderActionButton
              label="Delete"
              disabled={!props.selectedRuleId}
              onClick={props.onDelete}
            />
          </div>
        </div>
        <div className="mt-4 border border-outline bg-panel px-4 py-4">
          <pre className="whitespace-pre-wrap text-sm leading-6 text-foreground-muted">
            {props.panelMode === "history"
              ? formatJson(props.history)
              : props.panelMode === "dryRun"
                ? formatJson(props.dryRun)
                : formatJson(props.detail)}
          </pre>
        </div>
      </section>
    </div>
  );
}
