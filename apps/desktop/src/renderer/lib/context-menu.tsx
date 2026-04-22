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
  const [activeIndex, setActiveIndex] = useState<number | null>(null);
  const { menu, onClose } = props;

  useEffect(() => {
    if (!menu) {
      setActiveIndex(null);
      return;
    }

    setActiveIndex(findNextInteractiveIndex(menu.items, -1, 1));
  }, [menu]);

  useEffect(() => {
    if (!menu) return;
    const handleClick = () => onClose();
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        onClose();
        return;
      }

      if (e.key === "ArrowDown" || e.key === "j") {
        e.preventDefault();
        setActiveIndex((current) =>
          findNextInteractiveIndex(
            menu.items,
            current ?? -1,
            1,
          ),
        );
        return;
      }

      if (e.key === "ArrowUp" || e.key === "k") {
        e.preventDefault();
        setActiveIndex((current) =>
          findNextInteractiveIndex(
            menu.items,
            current ?? menu.items.length,
            -1,
          ),
        );
        return;
      }

      if (e.key === "Enter" || e.key === " ") {
        const nextIndex = activeIndex;
        if (nextIndex == null) {
          return;
        }
        const item = menu.items[nextIndex];
        if (!item || item.separator || item.disabled) {
          return;
        }
        e.preventDefault();
        item.onClick();
        onClose();
      }
    };
    document.addEventListener("click", handleClick);
    window.addEventListener("keydown", handleKeyDown, true);
    return () => {
      document.removeEventListener("click", handleClick);
      window.removeEventListener("keydown", handleKeyDown, true);
    };
  }, [activeIndex, menu, onClose]);

  if (!menu) return null;

  // Position adjustment to stay in viewport
  const { x, y, items } = menu;

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
            data-active={activeIndex === i ? "true" : "false"}
            className={cn(
              "flex w-full items-center justify-between px-3 py-1.5 text-left text-[length:var(--text-sm)] transition-colors",
              activeIndex === i && !item.disabled && !item.separator
                ? "bg-panel-elevated text-foreground"
                : "",
              item.disabled
                ? "text-foreground-subtle"
                : item.danger
                  ? "text-danger hover:bg-danger/10"
                  : "text-foreground-muted hover:bg-panel-elevated hover:text-foreground",
            )}
            onMouseEnter={() => setActiveIndex(i)}
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

function findNextInteractiveIndex(
  items: ContextMenuItem[],
  start: number,
  direction: 1 | -1,
) {
  if (items.length === 0) {
    return null;
  }

  let index = start;
  for (let count = 0; count < items.length; count += 1) {
    index = (index + direction + items.length) % items.length;
    const item = items[index];
    if (item && !item.separator && !item.disabled) {
      return index;
    }
  }

  return null;
}
