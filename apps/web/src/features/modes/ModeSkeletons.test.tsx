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
  test("Now reserves its summary, four sections and different people/task leads", () => {
    const { container } = render(<NowSkeleton shown />);
    expect(screen.getByRole("status", { name: "Loading Now" })).toHaveAttribute(
      "aria-busy",
      "true",
    );
    expect([...container.querySelectorAll("h2")].map((heading) => heading.textContent)).toEqual([
      "People",
      "Due soon",
      "Updates",
      "For tonight",
    ]);
    expect(container.querySelector('[data-skeleton-row="avatar"]')).toBeInTheDocument();
    expect(container.querySelector('[data-skeleton-row="checkbox"]')).toBeInTheDocument();
    expect(container.querySelector("[data-split=true]")).toBeInTheDocument();
    expect(container.querySelector('[data-frame="wide"]')).toBeInTheDocument();
  });

  test("To do reserves task bands and checkbox leads rather than an email list", () => {
    const { container } = render(<TodoSkeleton shown />);
    expect([...container.querySelectorAll("h2")].map((heading) => heading.textContent)).toEqual([
      "Now",
      "Coming up",
    ]);
    expect(container.querySelector('[data-skeleton-row="checkbox"]')).toBeInTheDocument();
    expect(container.querySelector('[data-skeleton-row="avatar"]')).toBeNull();
    expect(container.querySelector('[role="checkbox"]')).toBeNull();
    for (const list of container.querySelectorAll("ul")) {
      expect(list).toHaveClass("gap-0.5");
    }
  });

  test("Updates uses grouped single-line source rows", () => {
    const { container } = render(<UpdatesSkeleton shown />);
    expect([...container.querySelectorAll("h2")].map((heading) => heading.textContent)).toEqual([
      "Needs a look",
      "Changed",
      "Routine",
    ]);
    const first = container.querySelector('[data-skeleton-row="source"]');
    // One primary line, unlike the two-line mail skeleton this replaces.
    expect(first?.querySelectorAll(".type-body")).toHaveLength(1);
    for (const list of container.querySelectorAll("ul")) {
      expect(list).toHaveClass("gap-0.5");
    }
  });

  test("Reading reserves its edition bands and source leads", () => {
    const { container } = render(<ReadingSkeleton shown />);
    expect(container.querySelector("h2")).toHaveTextContent("Since you were last here");
    expect(container.querySelector('[data-skeleton-row="source"]')).toBeInTheDocument();
    expect(
      container.querySelector('[data-skeleton-row="source"]')?.querySelectorAll(".type-body"),
    ).toHaveLength(2);
  });

  test("Later does not pretend to be an edition with visit bands", () => {
    const { container } = render(<ReadingSkeleton shown later />);
    expect(screen.getByRole("status", { name: "Loading Later" })).toBeInTheDocument();
    expect(container.querySelector("h2")).toBeNull();
    expect(container.querySelector('[data-skeleton-row="source"]')).toBeInTheDocument();
  });

  test("Messages does not put another frame around its already-framed list", () => {
    const { container } = render(<MessagesSkeleton shown />);
    expect(container.querySelector("[data-frame]")).toBeNull();
    expect(container.querySelector('[data-skeleton-row="avatar"]')).toBeInTheDocument();
  });

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
      expect(screen.getAllByRole("status")).toHaveLength(1);
      expect(screen.queryByRole("heading")).toBeNull();
      expect(container.querySelector("button, a, input, [tabindex]")).toBeNull();
      expect(container.querySelector("[data-lit=true]")).toBeNull();
    },
  );
});
