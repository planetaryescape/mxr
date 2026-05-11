import { createFileRoute } from "@tanstack/react-router";

import { OnboardingRoute } from "@/features/onboarding/OnboardingRoute";

export const Route = createFileRoute("/onboarding")({
  component: OnboardingRoute,
});
