# Phase 7 — Rules editor with always-visible dry-run

Goal: rules are deterministic before they are intelligent (CLAUDE.md mandate). The web rules editor surfaces the existing engine with a builder, an always-visible dry-run preview, history, and per-run undo.

## Deliverables

1. `/rules` — list view with name / match summary / action summary / enabled toggle / last-fired count + timestamp / priority.
2. `/rules/new` and `/rules/$id` — two-column builder + dry-run.
3. **Always-visible dry-run preview** — a debounced re-run on every form change, showing matching messages from the current account in a table.
4. **Apply confirm modal** — destructive actions need confirmation; preview the action's effect on the matched set.
5. `/rules/$id?tab=history` — past runs with timestamps, count, undo per run.
6. **Enable/disable toggle** in list and detail.
7. **Priority** field in the form (higher = earlier).

## Bridge endpoints used

- `GET /api/v1/platform/rules` — list rules.
- `GET /api/v1/platform/rules/detail?rule=` — full rule.
- `GET /api/v1/platform/rules/form?rule=` — UI-shaped form data.
- `GET /api/v1/platform/rules/history?rule=` — run history.
- `GET /api/v1/platform/rules/dry-run?rule=` — dry-run a saved rule.
- `POST /api/v1/platform/rules/upsert` — create or update.
- `POST /api/v1/platform/rules/upsert-form` — UI-shaped upsert.
- `POST /api/v1/platform/rules/delete` { rule }.

(Verify all in generated.ts.)

## Files

```
src/features/rules/
  RulesRoute.tsx                  # /rules list
  RulesList.tsx
  RulesEmpty.tsx
  RuleEditorRoute.tsx             # /rules/$id
  RuleBuilder/
    RuleBuilder.tsx               # left column
    ConditionRow.tsx              # one condition; type-aware inputs
    ConditionTypeSelector.tsx     # from / subject / has-attachment / age / label / domain ...
    ActionRow.tsx                 # one action; type-aware inputs
    ActionTypeSelector.tsx        # archive / trash / label / move / star / mark-read ...
  RuleDryRun/
    DryRunPanel.tsx               # right column
    DryRunResults.tsx             # virtualized list of matches
  RuleHistoryTab.tsx              # /rules/$id?tab=history
  RuleApplyConfirm.tsx            # blocking modal
  useRuleForm.ts                  # react-hook-form + zod
  useRuleDryRun.ts                # debounced query
  rule-schema.ts                  # zod schema mirroring server's rule shape
```

## Form schema (zod sketch)

```ts
const conditionSchema = z.discriminatedUnion("type", [
  z.object({ type: z.literal("from"), op: z.enum(["equals", "contains", "matches"]), value: z.string() }),
  z.object({ type: z.literal("subject"), op: z.enum(["contains", "matches"]), value: z.string() }),
  z.object({ type: z.literal("hasAttachment"), value: z.boolean() }),
  z.object({ type: z.literal("ageGreaterThanDays"), value: z.number().int().positive() }),
  z.object({ type: z.literal("label"), value: z.string() }),
  z.object({ type: z.literal("domain"), op: z.enum(["equals", "endsWith"]), value: z.string() }),
]);

const actionSchema = z.discriminatedUnion("type", [
  z.object({ type: z.literal("archive") }),
  z.object({ type: z.literal("trash") }),
  z.object({ type: z.literal("label"), name: z.string() }),
  z.object({ type: z.literal("move"), label: z.string() }),
  z.object({ type: z.literal("star") }),
  z.object({ type: z.literal("markRead") }),
]);

const ruleSchema = z.object({
  name: z.string().min(1),
  enabled: z.boolean(),
  priority: z.number().int().min(0).max(1000).default(100),
  conditions: z.array(conditionSchema).min(1),
  actions: z.array(actionSchema).min(1),
});
```

This must mirror what the bridge's `upsert-form` accepts. Cross-reference with generated.ts; the local zod schema is the UI contract, the bridge type is authoritative.

## Dry-run flow

```ts
// useRuleDryRun.ts (sketch)
const debounced = useDebouncedValue(form.watch(), 300);
useQuery({
  queryKey: ["rule-dry-run", debounced],
  queryFn: () => api.GET("/api/v1/platform/rules/dry-run", { params: { query: serialize(debounced) } }),
  enabled: ruleSchema.safeParse(debounced).success,
});
```

Re-run on every form change (debounced). Render a virtualized list of matches.

## Apply confirm

When user clicks "Apply rule now" (i.e. run actions on the matched set, not just save the rule):
1. Show modal: "Apply 'Archive newsletters' to 47 messages?" with Cancel / Apply.
2. On Apply: POST to a `rules/apply` endpoint (verify) — bridge runs the actions and returns a run-id.
3. Toast offers "Undo run" for 60 s using the run-id.

Saving the rule itself doesn't touch any messages — it just persists the rule.

## Verification

1. `/rules` → existing rules listed.
2. `/rules/new` → empty builder + empty dry-run pane.
3. Add condition `from: contains alice@example.com` → dry-run table populates with alice's messages.
4. Add action `archive` → dry-run still shows the matched set; the action preview hint says "Will archive these 12 messages".
5. Click Save → toast "Saved". URL becomes `/rules/$id`.
6. Click "Apply now" → confirm modal → Apply → toast "Applied to 12 messages — Undo (60s)".
7. Click Undo → mutations reversed.
8. Switch to History tab → see the run logged.
9. Toggle Enabled off in list → row updates without reload.

## Decisions

- 2026-05-10 — Save is separate from Apply. Saving persists the rule; Apply runs it now. The bridge probably has a separate apply endpoint; if not, use a search→bulk-mutation fallback.
- 2026-05-10 — Dry-run results are read-only; no inline mutation buttons. To act, save → apply.
