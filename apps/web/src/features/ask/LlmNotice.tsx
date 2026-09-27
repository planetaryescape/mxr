import { Link } from "@tanstack/react-router";
import { Info } from "lucide-react";

/**
 * Shown when the daemon has no language model: what is on screen is the
 * archive's raw evidence, and this says how to get the written version.
 */
export function LlmNotice({ children }: { children: string }) {
  return (
    <p className="flex gap-1.5 text-[12.5px] text-muted-foreground">
      <Info className="mt-0.5 size-3.5 shrink-0 text-warning" aria-hidden="true" />
      <span>
        {children}{" "}
        <Link
          to="/settings/$section"
          params={{ section: "llm" }}
          className="text-primary hover:underline"
        >
          Set up a language model
        </Link>
        .
      </span>
    </p>
  );
}
