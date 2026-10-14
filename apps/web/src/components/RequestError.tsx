import { BridgeRequestError, DaemonUnavailableError, UnauthorizedError } from "@/api/client";

export function describeRequestError(error: unknown): { summary: string; technical: string } {
  const technical = error instanceof Error ? error.message : "No additional detail was provided.";
  if (error instanceof DaemonUnavailableError) {
    return {
      summary: "Could not reach mxr. Check the connection, or run mxr restart on the computer running mxr.",
      technical,
    };
  }
  if (error instanceof UnauthorizedError) {
    return {
      summary: "mxr could not verify local access. Check the Bridge token in Settings.",
      technical,
    };
  }
  if (error instanceof BridgeRequestError && error.status >= 500) {
    return {
      summary: "mxr could not complete this request because of a service error.",
      technical,
    };
  }
  if (error instanceof BridgeRequestError && error.status === 403) {
    return { summary: "mxr does not have permission to complete this request.", technical };
  }
  return { summary: "mxr could not complete this request.", technical };
}

export function RequestErrorDetails({ error }: { error: unknown }) {
  const description = describeRequestError(error);
  return (
    <div className="max-w-md break-words text-sm text-muted-foreground">
      <p>{description.summary}</p>
      <details className="mt-2 text-xs">
        <summary className="w-fit cursor-pointer underline underline-offset-4">
          Technical details
        </summary>
        <p className="mt-1 font-mono">{description.technical}</p>
      </details>
    </div>
  );
}
