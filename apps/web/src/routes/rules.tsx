import { createFileRoute } from "@tanstack/react-router";

import { RulesListRoute } from "@/features/rules/RulesListRoute";

export const Route = createFileRoute("/rules")({
  component: RulesListRoute,
});
