import { render, screen } from "@testing-library/react";
import { describe, expect, test } from "vitest";

import {
  MessagesSkeleton,
  NowSkeleton,
  ReadingSkeleton,
  TodoSkeleton,
  UpdatesSkeleton,
} from "./ModeSkeletons";

describe("mode loading layouts", () => {

  test.each([
    ["NowSkeleton", NowSkeleton],
    ["TodoSkeleton", TodoSkeleton],
    ["UpdatesSkeleton", UpdatesSkeleton],
    ["ReadingSkeleton", ReadingSkeleton],
    ["MessagesSkeleton", MessagesSkeleton],
  ] as const)(
    "%s keeps quiet-phase placeholders invisible and adds no fake controls or spoken content",
    (_name, Component) => {
      const { container, rerender } = render(<Component shown={false} />);
      expect(screen.queryByTestId("page-skeleton")).toBeNull();
      expect(container.querySelector('[aria-hidden="true"]')).toHaveClass("invisible");
      rerender(<Component shown />);
      expect(container.querySelector('[aria-hidden="true"]')).not.toHaveClass("invisible");
      expect(screen.getAllByRole("status")).toHaveLength(1);
      expect(screen.queryByRole("heading")).toBeNull();
      expect(container.querySelector("button, a, input, [tabindex]")).toBeNull();
      expect(container.querySelector("[data-lit=true]")).toBeNull();
    },
  );
});
