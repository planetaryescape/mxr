import { act, fireEvent, render, screen } from "@testing-library/react";
import { useState } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { ContactInput } from "./autocomplete";

function deferred<T>() {
  let resolve!: (value: T | PromiseLike<T>) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((nextResolve, nextReject) => {
    resolve = nextResolve;
    reject = nextReject;
  });
  return { promise, resolve, reject };
}

function Harness(props: {
  fetchSuggestions: (query: string) => Promise<Array<{ label: string; value: string }>>;
}) {
  const [value, setValue] = useState("");
  return (
    <ContactInput
      label="To"
      value={value}
      onChange={setValue}
      fetchSuggestions={props.fetchSuggestions}
      placeholder="Type a name or email..."
    />
  );
}

describe("ContactInput", () => {
  afterEach(() => {
    vi.useRealTimers();
  });

  it("keeps only the newest suggestion results when older requests resolve later", async () => {
    vi.useFakeTimers();

    const first = deferred<Array<{ label: string; value: string }>>();
    const second = deferred<Array<{ label: string; value: string }>>();
    const fetchSuggestions = vi.fn((query: string) => {
      if (query === "al") {
        return first.promise;
      }
      if (query === "alex") {
        return second.promise;
      }
      return Promise.resolve([]);
    });

    render(<Harness fetchSuggestions={fetchSuggestions} />);
    const input = screen.getByPlaceholderText("Type a name or email...");

    fireEvent.change(input, { target: { value: "al" } });
    await act(async () => {
      vi.advanceTimersByTime(200);
    });

    fireEvent.change(input, { target: { value: "alex" } });
    await act(async () => {
      vi.advanceTimersByTime(200);
    });

    await act(async () => {
      second.resolve([{ label: "Alex", value: "alex@example.com" }]);
      await Promise.resolve();
    });

    await act(async () => {
      first.resolve([{ label: "Alice", value: "alice@example.com" }]);
      await Promise.resolve();
    });

    expect(screen.getByText("Alex")).toBeInTheDocument();
    expect(screen.queryByText("Alice")).not.toBeInTheDocument();
  });

  it("drops pending suggestion results after the query is cleared", async () => {
    vi.useFakeTimers();

    const first = deferred<Array<{ label: string; value: string }>>();
    const fetchSuggestions = vi.fn((query: string) => {
      if (query === "al") {
        return first.promise;
      }
      return Promise.resolve([]);
    });

    render(<Harness fetchSuggestions={fetchSuggestions} />);
    const input = screen.getByPlaceholderText("Type a name or email...");

    fireEvent.change(input, { target: { value: "al" } });
    await act(async () => {
      vi.advanceTimersByTime(200);
    });

    fireEvent.change(input, { target: { value: "" } });
    await act(async () => {
      first.resolve([{ label: "Alice", value: "alice@example.com" }]);
      await Promise.resolve();
    });

    expect(screen.queryByText("Alice")).not.toBeInTheDocument();
  });
});
