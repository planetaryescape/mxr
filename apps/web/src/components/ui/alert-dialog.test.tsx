/* @vitest-environment jsdom */

import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { useState } from "react";
import { describe, expect, test } from "vitest";

import {
  AlertDialog,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogTitle,
} from "./alert-dialog";

function Harness() {
  // Opened by a shortcut: no trigger for Radix to hand focus back to.
  const [open, setOpen] = useState(true);
  return (
    <>
      <main data-active-pane="true" tabIndex={-1} />
      <AlertDialog open={open} onOpenChange={setOpen}>
        <AlertDialogContent>
          <AlertDialogTitle>Discard draft?</AlertDialogTitle>
          <AlertDialogDescription>It can't be recovered.</AlertDialogDescription>
          <AlertDialogCancel>Keep</AlertDialogCancel>
        </AlertDialogContent>
      </AlertDialog>
    </>
  );
}

describe("AlertDialog", () => {
  test("returns focus to the active pane when it closes with no trigger", async () => {
    render(<Harness />);
    await waitFor(() => expect(screen.getByRole("button", { name: "Keep" })).toHaveFocus());

    fireEvent.click(screen.getByRole("button", { name: "Keep" }));

    await waitFor(() => expect(screen.getByRole("main")).toHaveFocus());
  });
});
