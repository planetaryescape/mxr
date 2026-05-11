import { createFileRoute } from "@tanstack/react-router";

import { SettingsRoute } from "@/features/settings/SettingsRoute";

export const Route = createFileRoute("/settings/$section")({
  component: SettingsRoute,
});
