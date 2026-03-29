import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { cn } from "./cn";

type ToastVariant = "info" | "success" | "error" | "shortcut";

interface Toast {
  id: number;
  message: string;
  variant: ToastVariant;
  shortcutKey?: string;
  exiting?: boolean;
}

interface ToastContextValue {
  toast: (message: string, variant?: ToastVariant) => void;
  shortcutHint: (action: string, key: string) => void;
}

const ToastContext = createContext<ToastContextValue>({
  toast: () => {},
  shortcutHint: () => {},
});

export function useToast() {
  return useContext(ToastContext);
}

let nextId = 0;

export function ToastProvider(props: { children: ReactNode }) {
  const [toasts, setToasts] = useState<Toast[]>([]);
  const timers = useRef<Map<number, ReturnType<typeof setTimeout>>>(new Map());

  const dismiss = useCallback((id: number) => {
    setToasts((prev) =>
      prev.map((t) => (t.id === id ? { ...t, exiting: true } : t)),
    );
    setTimeout(() => {
      setToasts((prev) => prev.filter((t) => t.id !== id));
    }, 200);
  }, []);

  const addToast = useCallback(
    (message: string, variant: ToastVariant = "info", shortcutKey?: string) => {
      const id = nextId++;
      setToasts((prev) => [...prev.slice(-4), { id, message, variant, shortcutKey }]);
      const timer = setTimeout(() => dismiss(id), 3000);
      timers.current.set(id, timer);
    },
    [dismiss],
  );

  const toast = useCallback(
    (message: string, variant: ToastVariant = "info") => {
      addToast(message, variant);
    },
    [addToast],
  );

  const shortcutHint = useCallback(
    (action: string, key: string) => {
      addToast(action, "shortcut", key);
    },
    [addToast],
  );

  useEffect(() => {
    return () => {
      for (const timer of timers.current.values()) clearTimeout(timer);
    };
  }, []);

  return (
    <ToastContext value={{ toast, shortcutHint }}>
      {props.children}
      <div className="fixed bottom-4 right-4 z-50 flex flex-col-reverse gap-2">
        {toasts.map((t) => (
          <div
            key={t.id}
            className={cn(
              "flex items-center gap-3 border px-3 py-2 text-[length:var(--text-sm)] shadow-lg",
              "transition-all duration-200",
              t.exiting
                ? "translate-x-full opacity-0"
                : "translate-x-0 opacity-100",
              t.variant === "success" && "border-success/30 bg-success/10 text-success",
              t.variant === "error" && "border-danger/30 bg-danger/10 text-danger",
              t.variant === "info" && "border-outline bg-panel-elevated text-foreground-muted",
              t.variant === "shortcut" && "border-accent/30 bg-accent/8 text-foreground-muted",
            )}
            style={{ borderRadius: "var(--radius-md)" }}
          >
            <span className="min-w-0 flex-1">{t.message}</span>
            {t.variant === "shortcut" && t.shortcutKey ? (
              <span className="flex items-center gap-1.5 text-[length:var(--text-xs)] text-foreground-subtle">
                Next time, try
                <kbd className="inline-flex h-5 min-w-5 items-center justify-center border border-outline bg-canvas-elevated px-1.5 font-mono text-[length:var(--text-xs)] uppercase text-accent"
                  style={{ borderRadius: "var(--radius-sm)" }}
                >
                  {t.shortcutKey}
                </kbd>
              </span>
            ) : null}
          </div>
        ))}
      </div>
    </ToastContext>
  );
}
