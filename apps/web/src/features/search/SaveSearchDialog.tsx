/* Name-and-save dialog for the current search query. */

import { useState } from "react";

import { Button } from "@/components/ui/button";
import {
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";

export function SaveSearchDialog({
  q,
  pending,
  onCancel,
  onSave,
}: {
  q: string;
  pending: boolean;
  onCancel: () => void;
  onSave: (name: string) => void;
}) {
  const [name, setName] = useState("");
  return (
    <DialogContent className="max-w-md">
      <DialogHeader>
        <DialogTitle>Save this search</DialogTitle>
        <DialogDescription className="font-mono text-2xs">{q}</DialogDescription>
      </DialogHeader>
      <form
        onSubmit={(event) => {
          event.preventDefault();
          if (name.trim()) onSave(name.trim());
        }}
      >
        <Input
          autoFocus
          aria-label="Name"
          value={name}
          onChange={(event) => setName(event.target.value)}
          placeholder="Invoices to file"
        />
        <DialogFooter className="mt-4">
          <Button type="button" variant="ghost" onClick={onCancel}>
            Cancel
          </Button>
          <Button type="submit" disabled={!name.trim() || pending}>
            Save search
          </Button>
        </DialogFooter>
      </form>
    </DialogContent>
  );
}
