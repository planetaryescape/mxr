import { cn } from "@/lib/utils";

/** The tooltip: the model, and where it ran when that is known. */
export function modelMarkTitle(model: string, leavesMachine?: boolean): string {
  if (leavesMachine === undefined) return `Written by ${model}`;
  return leavesMachine
    ? `Written by ${model}, which runs off this machine`
    : `Written by ${model}, which runs on this machine`;
}

/**
 * `model · local` or `model · cloud` beside any model-written text (D136),
 * with the model and where it ran in its tooltip. When where it ran isn't
 * known (a reply from before the model changed) only the model shows.
 */
export function ModelMark({
  model,
  leavesMachine,
  className,
}: {
  model: string;
  leavesMachine?: boolean;
  className?: string;
}) {
  const where = leavesMachine === undefined ? null : leavesMachine ? "cloud" : "local";
  const title = modelMarkTitle(model, leavesMachine);
  return (
    <span data-testid="model-mark" title={title} className={cn("type-meta text-text-3", className)}>
      {model}
      {where ? ` · ${where}` : null}
    </span>
  );
}
