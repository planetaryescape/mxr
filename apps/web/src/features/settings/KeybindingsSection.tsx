import { KeyChip } from "@/components/KeyChip";
import { buildActionContext, shortcutSections } from "@/lib/actions";

export function KeybindingsSection() {
  // Every section, whatever view is active: this page is the reference.
  const keySections = shortcutSections(
    buildActionContext({
      path: "/settings/keybindings",
      activePane: "mailbox",
      scopeStack: [],
      selectionCount: 0,
      accountCount: 0,
      openThreadId: null,
    }),
  );
  return (
    <div className="space-y-6">
      <p className="mb-4 text-[13px] text-muted-foreground">
        Keys follow the mxr TUI. Where the web differs, the note says why. Press ? anywhere for the
        keys that apply to the current view.
      </p>
      <div className="space-y-6">
        {keySections.map((section) => (
          <section key={section.id}>
            <h3 className="mb-1.5 border-b border-border pb-1 font-mono text-2xs uppercase tracking-wider text-muted-foreground">
              {section.title}
            </h3>
            <ul className="grid gap-x-8 md:grid-cols-2">
              {section.hints.map((hint) => (
                <li key={hint.id} className="flex items-start gap-3 py-1 text-[13px]">
                  <span className="flex w-28 shrink-0 flex-wrap gap-1">
                    {hint.keys.slice(0, 2).map((key) => (
                      <KeyChip key={key}>{key}</KeyChip>
                    ))}
                  </span>
                  <span>
                    {hint.label}
                    {hint.note ? (
                      <span className="block text-2xs text-muted-foreground">{hint.note}</span>
                    ) : null}
                  </span>
                </li>
              ))}
            </ul>
          </section>
        ))}
      </div>
    </div>
  );
}
