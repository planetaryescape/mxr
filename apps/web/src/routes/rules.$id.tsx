import { createFileRoute } from "@tanstack/react-router";

import { RuleEditorRoute } from "@/features/rules/RuleEditorRoute";

export const Route = createFileRoute("/rules/$id")({
  component: RuleEditorRoute,
});
