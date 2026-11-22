import { Link, useRouterState } from "@tanstack/react-router";

import { Page } from "@/components/Page";
import { PageEmpty } from "@/components/PageParts";
import { Button } from "@/components/ui/button";

export function NotFoundPage() {
  const path = useRouterState({ select: (state) => state.location.pathname });
  return (
    <Page title="Not found" eyebrow="404">
      <PageEmpty
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
