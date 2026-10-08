import { Link, useRouterState } from "@tanstack/react-router";
import { SearchX } from "lucide-react";

import { Page } from "@/components/Page";
import { PageEmpty } from "@/components/PageParts";
import { Button } from "@/components/ui/button";

/**
 * Rendered inside the shell for any URL no route matches (TanStack's own
 * default is an unstyled "Not Found" in the corner). Shows the path so a
 * stale bookmark is recognisable, and offers the two places people go.
 */
export function NotFoundPage() {
  const path = useRouterState({ select: (state) => state.location.pathname });
  return (
    <Page title="Not found">
      <PageEmpty
        icon={<SearchX className="size-5" />}
        title={`Nothing at ${path}`}
        body="The link may be from an older version of mxr."
        action={
          <div className="flex gap-2">
            <Button size="sm" asChild>
              <Link to="/now">Go to Now</Link>
            </Button>
            <Button size="sm" variant="ghost" asChild>
              <Link to="/m/$mailbox" params={{ mailbox: "inbox" }}>
                Open Inbox
              </Link>
            </Button>
          </div>
        }
      />
    </Page>
  );
}
