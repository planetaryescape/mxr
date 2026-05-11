import { Hammer } from "lucide-react";

import { EmptyState } from "@/components/EmptyState";

export function PageStub({ title, phase }: { title: string; phase: string }) {
  return (
    <EmptyState
      icon={Hammer}
      title={title}
      description={`Implemented in ${phase}. See docs/web-app/${phase}.md for the plan.`}
    />
  );
}
