import { When } from "@/components/When";
import { formatLongDate } from "@/lib/format";

/** Server location is not completion in a mode. Keep the wording explicit. */
export function SourceFiled({ value }: { value?: string | null }) {
  if (!value) return null;
  return (
    <span className="type-meta shrink-0 text-text-3" data-testid="source-filed">
      left the inbox · observed{" "}
      <time dateTime={value} title={`Observed outside the inbox ${formatLongDate(value)}; the archive may have happened earlier`}>
        <When value={value} />
      </time>
    </span>
  );
}
