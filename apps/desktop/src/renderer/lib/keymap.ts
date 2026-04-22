import type {
  DesktopKeymapBindings,
  DesktopKeymapContext,
} from "../../shared/types";
import { parse, printParseErrorCode, type ParseError } from "jsonc-parser";
import { tuiDesktopManifest } from "../../shared/generated/tui-manifest";

type GeneratedBindings = typeof tuiDesktopManifest.bindings;
type GeneratedBindingContext = keyof GeneratedBindings;
type GeneratedBinding = GeneratedBindings[GeneratedBindingContext][number];
type GeneratedCommand = (typeof tuiDesktopManifest.commands)[number];
const DESKTOP_KEYMAP_CONTEXTS = new Set<DesktopKeymapContext>([
  "mailList",
  "threadView",
  "messageView",
  "rules",
  "accounts",
  "diagnostics",
]);

export type DesktopAction = GeneratedBinding["action"] | GeneratedCommand["action"] | string;
export type DesktopBindingContext = DesktopKeymapContext;

export type DesktopBinding = {
  action: DesktopAction;
  label: string;
  display: string;
  tokens: string[];
};

export type DesktopCommand = {
  action: GeneratedCommand["action"];
  category: GeneratedCommand["category"];
  label: GeneratedCommand["label"];
  shortcut: string;
};

export type EffectiveDesktopKeymap = Record<
  DesktopKeymapContext,
  DesktopBinding[]
>;

export type PendingBinding = {
  tokens: string[];
  deadline: number;
};

const DEFAULT_EXTRA_BINDINGS: Record<
  Exclude<DesktopKeymapContext, GeneratedBindingContext>,
  Record<string, DesktopAction>
> = {
  rules: {
    n: "open_rule_form_new",
    e: "open_rule_form_edit",
    d: "show_rule_dry_run",
    h: "show_rule_history",
    x: "toggle_rule_enabled",
  },
  accounts: {
    n: "open_account_form_new",
    t: "test_account_form",
    d: "set_default_account",
  },
  diagnostics: {
    O: "open_diagnostics_section:overview",
    D: "open_diagnostics_section:drafts",
    S: "open_diagnostics_section:subscriptions",
    Z: "open_diagnostics_section:snoozed",
    M: "open_diagnostics_section:semantic",
    L: "open_diagnostics_section:labels",
    A: "open_diagnostics_section:saved-searches",
    T: "open_diagnostics_section:settings",
    B: "generate_bug_report",
  },
};

const ACTION_LABELS = new Map<string, string>([
  ...tuiDesktopManifest.commands.map((command) => [command.action, command.label] as const),
  ...Object.values(tuiDesktopManifest.bindings)
    .flat()
    .map((binding) => [binding.action, binding.label] as const),
  ["open_rule_form_new", "New Rule"],
  ["open_rule_form_edit", "Edit Rule"],
  ["show_rule_dry_run", "Rule Dry Run"],
  ["show_rule_history", "Rule History"],
  ["toggle_rule_enabled", "Toggle Rule Enabled"],
  ["open_account_form_new", "New Account"],
  ["test_account_form", "Test Account"],
  ["set_default_account", "Set Default Account"],
  ["generate_bug_report", "Generate Bug Report"],
  ["open_diagnostics_section:overview", "Open diagnostics overview"],
  ["open_diagnostics_section:drafts", "Open diagnostics drafts"],
  ["open_diagnostics_section:subscriptions", "Open diagnostics subscriptions"],
  ["open_diagnostics_section:snoozed", "Open diagnostics snoozed"],
  ["open_diagnostics_section:semantic", "Open diagnostics semantic"],
  ["open_diagnostics_section:labels", "Open diagnostics labels"],
  ["open_diagnostics_section:saved-searches", "Open diagnostics saved searches"],
  ["open_diagnostics_section:settings", "Open diagnostics settings"],
]);

export function createEffectiveKeymap(
  overrides: DesktopKeymapBindings,
): EffectiveDesktopKeymap {
  const base = buildDefaultKeymap();

  return {
    mailList: applyOverrides(base.mailList, overrides.mailList),
    threadView: applyOverrides(base.threadView, overrides.threadView),
    messageView: applyOverrides(base.messageView, overrides.messageView),
    rules: applyOverrides(base.rules, overrides.rules),
    accounts: applyOverrides(base.accounts, overrides.accounts),
    diagnostics: applyOverrides(base.diagnostics, overrides.diagnostics),
  };
}

export function bindingsForContext(
  keymap: EffectiveDesktopKeymap,
  context: DesktopKeymapContext,
): readonly DesktopBinding[] {
  return keymap[context];
}

export function commandPaletteEntries(
  keymap: EffectiveDesktopKeymap,
): DesktopCommand[] {
  return tuiDesktopManifest.commands.map((command) => ({
    ...command,
    shortcut: shortcutForAction(keymap, command.action) ?? command.shortcut,
  }));
}

export function shortcutForAction(
  keymap: EffectiveDesktopKeymap,
  action: string,
  preferredContexts?: DesktopKeymapContext[],
) {
  const contexts = preferredContexts ?? [
    "mailList",
    "threadView",
    "messageView",
    "rules",
    "accounts",
    "diagnostics",
  ];

  for (const context of contexts) {
    const matches = keymap[context]
      .filter((binding) => binding.action === action)
      .map((binding) => binding.display);

    if (matches.length > 0) {
      return matches.join("/");
    }
  }

  return null;
}

export function resolveBindingAction(
  keymap: EffectiveDesktopKeymap,
  context: DesktopKeymapContext,
  token: string,
  pending: PendingBinding | null,
  now: number,
): { action?: DesktopAction; pending?: PendingBinding | null } | null {
  const bindings = bindingsForContext(keymap, context);
  const activeSequence = pending && pending.deadline >= now ? [...pending.tokens, token] : [token];

  const exact = bindings.find((binding) => matchesTokens(binding.tokens, activeSequence));
  if (exact) {
    return { action: exact.action, pending: null };
  }

  const partial = bindings.some((binding) => isPrefix(activeSequence, binding.tokens));
  if (partial) {
    return {
      pending: {
        tokens: activeSequence,
        deadline: now + 500,
      },
    };
  }

  return null;
}

export function serializeKeymapBindings(
  keymap: EffectiveDesktopKeymap,
): DesktopKeymapBindings {
  return {
    mailList: serializeContext(keymap.mailList),
    threadView: serializeContext(keymap.threadView),
    messageView: serializeContext(keymap.messageView),
    rules: serializeContext(keymap.rules),
    accounts: serializeContext(keymap.accounts),
    diagnostics: serializeContext(keymap.diagnostics),
  };
}

export function formatKeymapBindings(bindings: DesktopKeymapBindings) {
  return `${JSON.stringify(bindings, null, 2)}\n`;
}

export function parseKeymapBindings(input: string): DesktopKeymapBindings {
  const errors: ParseError[] = [];
  const parsed = parse(input, errors, {
    allowTrailingComma: true,
    disallowComments: false,
  });

  if (errors.length > 0) {
    const error = errors[0]!;
    throw new Error(
      `${printParseErrorCode(error.error)} at offset ${error.offset}`,
    );
  }

  return validateKeymapBindings(parsed);
}

function serializeContext(bindings: readonly DesktopBinding[]) {
  return Object.fromEntries(bindings.map((binding) => [binding.display, binding.action]));
}

function applyOverrides(
  defaults: readonly DesktopBinding[],
  overrideContext: Record<string, string> | undefined,
) {
  if (!overrideContext) {
    return [...defaults];
  }

  let next = [...defaults];
  for (const [display, action] of Object.entries(overrideContext)) {
    next = next.filter(
      (binding) => binding.action !== action && binding.display !== display,
    );
    next.push(createBinding(display, action));
  }

  return sortBindings(next);
}

function buildDefaultKeymap(): EffectiveDesktopKeymap {
  return {
    mailList: tuiDesktopManifest.bindings.mailList.map(cloneBinding),
    threadView: tuiDesktopManifest.bindings.threadView.map(cloneBinding),
    messageView: tuiDesktopManifest.bindings.messageView.map(cloneBinding),
    rules: buildBindingsFromMap(DEFAULT_EXTRA_BINDINGS.rules),
    accounts: buildBindingsFromMap(DEFAULT_EXTRA_BINDINGS.accounts),
    diagnostics: buildBindingsFromMap(DEFAULT_EXTRA_BINDINGS.diagnostics),
  };
}

function buildBindingsFromMap(bindings: Record<string, DesktopAction>) {
  return sortBindings(
    Object.entries(bindings).map(([display, action]) =>
      createBinding(display, action),
    ),
  );
}

function createBinding(display: string, action: DesktopAction): DesktopBinding {
  return {
    action,
    label: ACTION_LABELS.get(action) ?? action,
    display,
    tokens: parseBindingTokens(display),
  };
}

function sortBindings(bindings: readonly DesktopBinding[]) {
  return [...bindings].sort(
    (left, right) =>
      left.display.localeCompare(right.display) ||
      left.label.localeCompare(right.label),
  );
}

function parseBindingTokens(display: string) {
  if (display.startsWith("Ctrl-")) {
    return [display];
  }

  if (display === "Enter" || display === "Esc" || display === "Tab") {
    return [display];
  }

  if (display.includes(" ")) {
    return display.split(/\s+/).filter(Boolean);
  }

  return [...display];
}

function cloneBinding(binding: GeneratedBinding): DesktopBinding {
  return {
    action: binding.action,
    label: binding.label,
    display: binding.display,
    tokens: [...binding.tokens],
  };
}

function matchesTokens(left: readonly string[], right: readonly string[]) {
  return left.length === right.length && left.every((token, index) => token === right[index]);
}

function isPrefix(prefix: readonly string[], full: readonly string[]) {
  return prefix.length < full.length && prefix.every((token, index) => token === full[index]);
}

function validateKeymapBindings(value: unknown): DesktopKeymapBindings {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    throw new Error("Keymap JSON must be an object");
  }

  const result: DesktopKeymapBindings = {};

  for (const [context, bindings] of Object.entries(value)) {
    if (!DESKTOP_KEYMAP_CONTEXTS.has(context as DesktopKeymapContext)) {
      throw new Error(`Unknown keymap context: ${context}`);
    }
    if (!bindings || typeof bindings !== "object" || Array.isArray(bindings)) {
      throw new Error(`Context ${context} must map shortcut strings to actions`);
    }

    const validatedEntries = Object.entries(bindings).map(([shortcut, action]) => {
      if (!shortcut.trim()) {
        throw new Error(`Context ${context} contains an empty shortcut`);
      }
      if (typeof action !== "string" || !action.trim()) {
        throw new Error(`Context ${context} contains an invalid action`);
      }
      return [shortcut, action] as const;
    });

    result[context as DesktopKeymapContext] = Object.fromEntries(validatedEntries);
  }

  return result;
}
