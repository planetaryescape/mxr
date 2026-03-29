import { useCallback, useEffect, useRef, useState } from "react";
import { cn } from "./cn";

export interface ContextMenuItem {
  label: string;
  shortcut?: string;
  danger?: boolean;
  disabled?: boolean;
  separator?: boolean;
  onClick: () => void;
}

interface ContextMenuState {
  x: number;
  y: number;
  items: ContextMenuItem[];
}

export function useContextMenu() {
  const [menu, setMenu] = useState<ContextMenuState | null>(null);

  const show = useCallback((e: React.MouseEvent, items: ContextMenuItem[]) => {
    e.preventDefault();
    setMenu({ x: e.clientX, y: e.clientY, items });
  }, []);

  const close = useCallback(() => setMenu(null), []);

  return { menu, show, close } as const;
}

export function ContextMenuOverlay(props: {
  menu: ContextMenuState | null;
  onClose: () => void;
}) {
  const menuRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!props.menu) return;
    const handleClick = () => props.onClose();
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") props.onClose();
    };
    document.addEventListener("click", handleClick);
    document.addEventListener("keydown", handleKeyDown);
    return () => {
      document.removeEventListener("click", handleClick);
      document.removeEventListener("keydown", handleKeyDown);
    };
  }, [props.menu, props.onClose]);

  if (!props.menu) return null;

  // Position adjustment to stay in viewport
  const { x, y, items } = props.menu;

  return (
    <div
      ref={menuRef}
      className="fixed z-50 min-w-48 border border-outline bg-panel py-1 shadow-xl"
      style={{
        left: `${x}px`,
        top: `${y}px`,
        borderRadius: "var(--radius-md)",
        animation: "scaleIn var(--duration-fast) var(--ease-out-expo)",
        transformOrigin: "top left",
      }}
    >
      {items.map((item, i) =>
        item.separator ? (
          <div key={`sep-${i}`} className="my-1 border-t border-outline" />
        ) : (
          <button
            key={`${item.label}-${i}`}
            type="button"
            disabled={item.disabled}
            className={cn(
              "flex w-full items-center justify-between px-3 py-1.5 text-left text-[length:var(--text-sm)] transition-colors",
              item.disabled
                ? "text-foreground-subtle"
                : item.danger
                  ? "text-danger hover:bg-danger/10"
                  : "text-foreground-muted hover:bg-panel-elevated hover:text-foreground",
            )}
            onClick={(e) => {
              e.stopPropagation();
              item.onClick();
              props.onClose();
            }}
          >
            <span>{item.label}</span>
            {item.shortcut ? (
              <kbd className="ml-4 font-mono text-[length:var(--text-xs)] text-foreground-subtle">
                {item.shortcut}
              </kbd>
            ) : null}
          </button>
        ),
      )}
    </div>
  );
}
