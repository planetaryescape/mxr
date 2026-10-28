import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { toast } from "sonner";

import { SWATCH_CLASS } from "@/components/AccountMark";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import type { RuntimeAccount } from "@/features/compose/api";
import { cn } from "@/lib/utils";
import { ACCOUNT_SLOTS, type AccountSlot } from "@/styles/accountSlots";

import type { AccountIdentities } from "./useAccountIdentities";
import { setAccountIdentity } from "./api";

const TAG_PATTERN = /^[A-Z0-9]{1,3}$/;

/**
 * Sets an account's colour and tag through the daemon, which validates both and
 * refuses a tag another account holds. A refusal stays in the dialog so the
 * user can pick another tag without losing the colour they chose.
 */
export function AccountIdentityDialog({
  account,
  identities,
  onClose,
}: {
  account: RuntimeAccount | null;
  identities: AccountIdentities;
  onClose: () => void;
}) {
  return (
    <Dialog open={account !== null} onOpenChange={(open) => !open && onClose()}>
      {account ? (
        // Remounted per account so the fields start from that account's values.
        <IdentityForm
          key={account.account_id}
          account={account}
          identities={identities}
          onClose={onClose}
        />
      ) : null}
    </Dialog>
  );
}

function IdentityForm({
  account,
  identities,
  onClose,
}: {
  account: RuntimeAccount;
  identities: AccountIdentities;
  onClose: () => void;
}) {
  const qc = useQueryClient();
  const current = identities.byId.get(account.account_id);
  const [color, setColor] = useState<AccountSlot>(current?.slot ?? "slate");
  const [tag, setTag] = useState(current?.tag ?? "");
  const [refusal, setRefusal] = useState<string | null>(null);
  const tagValid = TAG_PATTERN.test(tag);
  const usedBy = (slot: AccountSlot) =>
    [...identities.byId.values()]
      .filter((other) => other.accountId !== account.account_id && other.slot === slot)
      .map((other) => other.name);

  const save = useMutation({
    mutationFn: () => setAccountIdentity(account.key ?? "", { color, tag }),
    onSuccess: ({ result }) => {
      // The daemon answers a refused change as a normal response, so `ok` decides.
      if (!result.ok) {
        setRefusal(result.save?.detail || result.summary);
        return;
      }
      toast.success("Colour and tag saved");
      void qc.invalidateQueries({ queryKey: ["accounts"] });
      onClose();
    },
    onError: (error) => setRefusal(error.message),
  });

  const sharing = usedBy(color);
  return (
    <DialogContent>
      <DialogHeader>
        <DialogTitle>Colour and tag</DialogTitle>
        <DialogDescription>
          The tag marks {account.email} on every email from it when mail from several accounts is
          shown together. Colours repeat after the eighth account, so the tag tells those apart.
        </DialogDescription>
      </DialogHeader>
      <form
        className="grid gap-4"
        onSubmit={(event) => {
          event.preventDefault();
          setRefusal(null);
          if (tagValid && account.key) save.mutate();
        }}
      >
        <fieldset className="grid gap-2">
          <legend className="text-xs text-muted-foreground">Colour</legend>
          <div role="radiogroup" aria-label="Account colour" className="flex flex-wrap gap-2">
            {ACCOUNT_SLOTS.map((slot) => (
              <button
                key={slot}
                type="button"
                role="radio"
                aria-checked={color === slot}
                aria-label={`${slot}${usedBy(slot).length ? `, used by ${usedBy(slot).join(", ")}` : ""}`}
                title={usedBy(slot).length ? `Also used by ${usedBy(slot).join(", ")}` : slot}
                onClick={() => setColor(slot)}
                className={cn(
                  "size-7 rounded-md border-2",
                  SWATCH_CLASS[slot],
                  color === slot ? "border-foreground" : "border-transparent",
                )}
              />
            ))}
          </div>
          {sharing.length > 0 ? (
            <p className="text-2xs text-muted-foreground">
              {color} is also used by {sharing.join(", ")}. The tag tells them apart.
            </p>
          ) : null}
        </fieldset>
        <div className="grid gap-1.5">
          <Label htmlFor={`identity-tag-${account.account_id}`}>Tag</Label>
          <Input
            id={`identity-tag-${account.account_id}`}
            value={tag}
            maxLength={3}
            autoComplete="off"
            spellCheck={false}
            aria-invalid={!tagValid}
            aria-describedby={`identity-tag-help-${account.account_id}`}
            onChange={(event) => setTag(event.target.value.toUpperCase())}
            className="w-24 font-mono"
          />
          <p
            id={`identity-tag-help-${account.account_id}`}
            className="text-2xs text-muted-foreground"
          >
            One to three letters or digits.
          </p>
        </div>
        {refusal ? (
          <p role="alert" className="text-xs text-destructive">
            {refusal}
          </p>
        ) : null}
        <DialogFooter>
          <Button type="button" variant="ghost" onClick={onClose}>
            Cancel
          </Button>
          <Button type="submit" disabled={!tagValid || !account.key || save.isPending}>
            Save
          </Button>
        </DialogFooter>
      </form>
    </DialogContent>
  );
}
