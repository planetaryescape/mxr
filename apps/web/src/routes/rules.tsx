import { Outlet, createFileRoute, useMatch } from "@tanstack/react-router";

import { RulesListRoute } from "@/features/rules/RulesListRoute";

export const Route = createFileRoute("/rules")({
  component: RulesLayout,
});

function RulesLayout() {
  const editing = useMatch({ from: "/rules/$id", shouldThrow: false });
  return editing ? <Outlet /> : <RulesListRoute />;
}
