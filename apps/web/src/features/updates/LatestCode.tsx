import { formatTime } from "@/lib/format";

import type { OneTimeCode } from "./api";
import { copyCode } from "./copyCode";

/** "Google (accounts.google.com)": a borrowed brand shows beside its host. */
export function codeSender(code: OneTimeCode): string {
  return code.sender_host && code.sender_host.toLowerCase() !== code.sender_name.toLowerCase()
    ? `${code.sender_name} (${code.sender_host})`
    : code.sender_name;
}

/**
 * The newest current one-time code, above the digest: it arrived between
 * cuts and expires in minutes, so it cannot wait for a line. Click or `y`.
 */
export function LatestCode({ code }: { code: OneTimeCode }) {
  const from = codeSender(code);
  return (
    <div
      data-testid="updates-latest-code"
      className="mx-5 mt-3 flex flex-wrap items-baseline gap-x-3 gap-y-1 rounded-md border border-border px-3 py-2"
    >
      <span className="type-meta text-text-2">Latest code</span>
      <button
        type="button"
        aria-label={`Copy code ${code.code} from ${from}`}
        title="Copy the code (y)"
        className="rounded border border-border px-1.5 font-mono tracking-wider text-text-1 hover:bg-muted"
        onClick={() => copyCode(code.code, from)}
      >
        {code.code}
      </button>
      <span className="type-meta text-text-3">
        from {from} · expires{" "}
        <time dateTime={code.expires_at}>{formatTime(new Date(code.expires_at))}</time>
      </span>
    </div>
  );
}
