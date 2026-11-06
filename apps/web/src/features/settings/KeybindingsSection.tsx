/*
 * Every key, grouped by where it works, and the person's own keys. A change
 * goes to `keys.toml` through the daemon (`POST /api/v1/platform/keymap`),
 * so the TUI uses it from its next start. A new key is checked as it is
 * typed (a dry run that names any action already on it); Save stays off
 * until the check passes. Blueprint chapter 35, D170.
 */

import { useMutation, useQuery } from "@tanstack/react-query";
import { Pencil } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { toast } from "sonner";

import { ToggleSetting } from "./settingsParts";
import { KeyChip } from "@/components/KeyChip";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  buildActionContext,
  chordsOf,
  formatChord,
  getRegistry,
  shortcutSections,
  type ShortcutHint,
  type ShortcutSection,
} from "@/lib/actions";
import { applyServedKeymap } from "@/lib/actions/servedKeymap";
import { refuseWhileDaemonDown } from "@/lib/daemonAvailability";
import { isComposingKey } from "@/lib/keys/composition";
import { isCharacterKey, useKeymapStore } from "@/state/keymapStore";

import { updateKeys, type KeysChange, type KeysUpdate } from "./keysApi";

/** How long typing rests before the new keys are checked. */
const CHECK_DELAY_MS = 300;

/** "x, g x" → ["x", "g x"]: keys are comma-separated, since a sequence has spaces. */
export function parseKeyList(text: string): string[] {
  return text
    .split(",")
    .map((key) => key.trim().replace(/\s+/g, " "))
    .filter((key) => key.length > 0);
}

function useSaved(onSaved: (update: KeysUpdate) => void) {
  return useMutation({
    mutationFn: (change: KeysChange) => updateKeys(change, false),
    onSuccess: (update) => {
      if (!update.saved) return;
      applyServedKeymap(update.keymap);
      onSaved(update);
    },
    onError: (error) => toast.error("Couldn't save keys.toml", { description: String(error) }),
  });
}

export function KeybindingsSection() {
  const singleKeys = useKeymapStore((state) => state.singleKeys);
  const remapped = useKeymapStore((state) => state.remapped);
  const problems = useKeymapStore((state) => state.problems);
  // Re-render when the person's keys change; the registry is rebuilt then.
  useKeymapStore((state) => state.version);
  const [editing, setEditing] = useState<string | null>(null);
  // An action the person left without a key is in no key list, so it gets
  // its own section: that is where its keys come back.
  const keylessHints: ShortcutHint[] = remapped.flatMap((id) => {
    const action = getRegistry().get(id);
    return action && chordsOf(action).length === 0
      ? [
          {
            id,
            keys: [],
            label: action.label,
            shortLabel: action.shortLabel ?? action.label,
            live: true,
          },
        ]
      : [];
  });
  // Every section, whatever view is active: this page is the reference.
  const keySections: ShortcutSection[] = [
    ...(keylessHints.length > 0
      ? [{ id: "keyless", title: "Yours, without a key", hints: keylessHints }]
      : []),
    ...shortcutSections(
      buildActionContext({
        path: "/settings/keybindings",
        activePane: "mailbox",
        scopeStack: [],
        selectionCount: 0,
        accountCount: 0,
        openThreadId: null,
      }),
    ),
  ];
  const toggle = useSaved((update) =>
    toast.success(
      update.keymap.single_keys ? "Single-key shortcuts are on" : "Single-key shortcuts are off",
      { description: "Saved to keys.toml. The TUI uses it from its next start." },
    ),
  );

  return (
    <div className="space-y-6">
      <div>
        <ToggleSetting
          id="single-keys"
          label="Single-key shortcuts"
          description="Keys that are one character, such as e or g i. Turn them off if you use speech input or press keys by accident: keys with ⌘ or Ctrl, Esc, Enter and the arrows keep working, and ⌘K opens the palette."
          checked={singleKeys}
          onChange={(on) => {
            if (refuseWhileDaemonDown("change keys")) return;
            toggle.mutate({ kind: "single_keys", on });
          }}
        />
        {problems.length > 0 ? (
          <div className="mt-3 rounded-md border border-border p-3 text-xs">
            <p className="font-medium">mxr is ignoring part of keys.toml</p>
            <ul className="mt-1 list-disc pl-4 text-muted-foreground">
              {problems.map((problem) => (
                <li key={problem}>{problem}</li>
              ))}
            </ul>
          </div>
        ) : null}
      </div>
      <p className="text-sm text-muted-foreground">
        Keys follow the mxr TUI. Where the web differs, the note says why. Change a key with the
        pencil: it is saved to keys.toml and applies in the TUI and here. Press ? anywhere for the
        keys that apply to the current view.
      </p>
      <div className="space-y-6">
        {keySections.map((section) => (
          <section key={section.id}>
            <h3 className="mb-1.5 border-b border-border pb-1 font-mono text-2xs uppercase tracking-wider text-muted-foreground">
              {section.title}
            </h3>
            <ul className="grid gap-x-8 md:grid-cols-2">
              {section.hints.map((hint) => {
                const action = getRegistry().get(hint.id);
                const chords = action ? chordsOf(action) : [];
                const editingThis = editing === `${section.id}:${hint.id}`;
                return (
                  <li key={hint.id} className="py-1 text-sm">
                    <div className="flex items-start gap-3">
                      <span className="flex w-28 shrink-0 flex-wrap gap-1">
                        {chords.slice(0, 2).map((chord) => (
                          <KeyChip
                            key={chord}
                            className={
                              !singleKeys && isCharacterKey(chord) ? "line-through opacity-60" : ""
                            }
                          >
                            {formatChord(chord)}
                          </KeyChip>
                        ))}
                      </span>
                      <span className="min-w-0 flex-1">
                        {hint.label}
                        {remapped.includes(hint.id) ? (
                          <span className="ml-1.5 text-2xs text-muted-foreground">(yours)</span>
                        ) : null}
                        {hint.note ? (
                          <span className="block text-2xs text-muted-foreground">{hint.note}</span>
                        ) : null}
                      </span>
                      <Button
                        variant="ghost"
                        size="icon-xs"
                        aria-label={`Change keys for ${hint.label}`}
                        title={`Change keys for ${hint.label}`}
                        aria-expanded={editingThis}
                        data-keys-editor={`${section.id}:${hint.id}`}
                        onClick={() => setEditing(editingThis ? null : `${section.id}:${hint.id}`)}
                      >
                        <Pencil />
                      </Button>
                    </div>
                    {editingThis && action ? (
                      <KeyEditor
                        id={action.id}
                        label={action.label}
                        keys={chords}
                        remapped={remapped.includes(action.id)}
                        onClose={() => {
                          const opener = `${section.id}:${hint.id}`;
                          setEditing(null);
                          // Back to the pencil that opened the editor, not the top of
                          // the page: the next key to change is usually nearby.
                          requestAnimationFrame(() =>
                            document
                              .querySelector<HTMLElement>(
                                `[data-keys-editor="${CSS.escape(opener)}"]`,
                              )
                              ?.focus(),
                          );
                        }}
                      />
                    ) : null}
                  </li>
                );
              })}
            </ul>
          </section>
        ))}
      </div>
    </div>
  );
}

function KeyEditor({
  id,
  label,
  keys,
  remapped,
  onClose,
}: {
  id: string;
  label: string;
  keys: string[];
  remapped: boolean;
  onClose: () => void;
}) {
  const [text, setText] = useState(keys.join(", "));
  const typed = useMemo(() => parseKeyList(text), [text]);
  const [checked, setChecked] = useState(typed);
  useEffect(() => {
    const timer = setTimeout(() => setChecked(typed), CHECK_DELAY_MS);
    return () => clearTimeout(timer);
  }, [typed]);
  const unchanged = checked.join("\n") === keys.join("\n");
  const preview = useQuery({
    queryKey: ["keymap", "preview", id, checked],
    queryFn: () => updateKeys({ kind: "set_action", id, keys: checked }, true),
    enabled: !unchanged,
    retry: false,
    staleTime: 0,
  });
  const done = (what: string) => () => {
    toast.success(what, {
      description: "Saved to keys.toml. The TUI uses it from its next start.",
    });
    onClose();
  };
  const save = useSaved(done(`${label} keys saved`));
  const reset = useSaved(done(`${label} has its default keys again`));
  const settled = checked.join("\n") === typed.join("\n");
  const problems = save.data?.problems.length
    ? save.data.problems
    : reset.data?.problems.length
      ? reset.data.problems
      : (preview.data?.problems ?? []);
  const canSave = settled && !unchanged && preview.isSuccess && preview.data.problems.length === 0;

  return (
    <form
      className="mt-2 space-y-2 rounded-md border border-border p-3"
      onKeyDown={(event) => {
        // Esc cancels, as everywhere else; nothing typed here is saved.
        if (event.key !== "Escape" || isComposingKey(event.nativeEvent)) return;
        event.preventDefault();
        onClose();
      }}
      onSubmit={(event) => {
        event.preventDefault();
        if (!canSave || refuseWhileDaemonDown("change keys")) return;
        save.mutate({ kind: "set_action", id, keys: checked });
      }}
    >
      <label className="block text-xs font-medium" htmlFor={`keys-${id}`}>
        Keys for {label}, primary first
      </label>
      <Input
        id={`keys-${id}`}
        value={text}
        onChange={(event) => setText(event.target.value)}
        placeholder="x, g x, Mod+Shift+k (empty: palette only)"
        className="h-8 font-mono text-xs"
        autoFocus
      />
      <p className="text-2xs text-muted-foreground" aria-live="polite">
        {unchanged
          ? "Separate keys with commas. Mod is ⌘ on a Mac and Ctrl elsewhere, and the TUI's Ctrl."
          : preview.isFetching || !settled
            ? "Checking…"
            : problems.length > 0
              ? null
              : checked.length === 0
                ? `${label} will have no key; the palette still runs it.`
                : `${label} will be ${checked.map((key) => formatChord(key)).join(", ")} in the web app and the TUI.`}
      </p>
      {problems.length > 0 ? (
        <ul className="list-disc pl-4 text-2xs text-destructive" aria-live="polite">
          {problems.map((problem) => (
            <li key={problem}>{problem}</li>
          ))}
        </ul>
      ) : null}
      {preview.isError ? (
        <p className="text-2xs text-destructive">Couldn't check: {String(preview.error)}</p>
      ) : null}
      <div className="flex flex-wrap gap-2">
        <Button type="submit" size="xs" disabled={!canSave || save.isPending}>
          Save
        </Button>
        {remapped ? (
          <Button
            type="button"
            size="xs"
            variant="outline"
            disabled={reset.isPending}
            onClick={() => {
              if (refuseWhileDaemonDown("change keys")) return;
              reset.mutate({ kind: "reset_action", id });
            }}
          >
            Use the default keys
          </Button>
        ) : null}
        <Button type="button" size="xs" variant="ghost" onClick={onClose}>
          Cancel
        </Button>
      </div>
    </form>
  );
}
