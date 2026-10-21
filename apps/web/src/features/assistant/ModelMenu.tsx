/*
 * The model mark under the message box, as a menu: which model the
 * assistant uses now and where it runs (D136), and the choices. A choice
 * that can't be used yet stays in the list, disabled, with what to do about
 * it; the daemon decides, so a client never offers what it would refuse.
 * The choice applies to every conversation, from the next message.
 */

import { Check, ChevronDown } from "lucide-react";

import { assistantToast } from "./assistantToast";
import { useAssistantActions, useAvailability, useModels } from "./useAssistant";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";

export function ModelMenu() {
  const availability = useAvailability().data;
  const models = useModels().data;
  const { pickModel } = useAssistantActions();
  // An older daemon has no picker: the plain mark stays.
  if (!models) return <PlainMark />;

  const current = models.choices.find((choice) => choice.id === models.current);
  const name = current?.label ?? availability?.model ?? models.current ?? null;
  const where = availability?.model ? (availability.leaves_machine ? "cloud" : "local") : null;
  return (
    <DropdownMenu>
      <DropdownMenuTrigger
        aria-label={name ? `Model: ${name}. Change it` : "Choose a model"}
        title={
          availability?.model
            ? availability.leaves_machine
              ? `${availability.model} runs off this machine: your messages and the mail it reads go to its provider`
              : `${availability.model} runs on this machine: nothing leaves it`
            : undefined
        }
        className="type-meta -mx-1 inline-flex max-w-full items-center gap-0.5 rounded-sm px-1 text-text-3 outline-none hover:text-text-1 focus-visible:text-text-1 focus-visible:ring-1 focus-visible:ring-signal"
      >
        <span className="truncate">
          {name ?? "Choose a model"}
          {name && where ? ` · ${where}` : null}
        </span>
        <ChevronDown className="size-3 shrink-0" strokeWidth={1.75} aria-hidden="true" />
      </DropdownMenuTrigger>
      <DropdownMenuContent side="top" align="start" className="w-72">
        <p className="type-meta px-2 py-1.5 text-text-3">
          Every conversation uses this model from its next message. A cloud model receives your
          messages and the mail the assistant reads.
        </p>
        {models.choices.map((choice) => (
          <DropdownMenuItem
            key={choice.id}
            disabled={!choice.available}
            onSelect={() =>
              pickModel.mutate(choice.id, {
                onError: (error) =>
                  assistantToast.error(error instanceof Error ? error.message : String(error)),
              })
            }
            className="flex-col items-stretch gap-0.5 data-[disabled]:opacity-100"
          >
            <span className="flex items-center gap-2">
              <span
                className={choice.available ? "type-body text-text-1" : "type-body text-text-3"}
              >
                {choice.label}
              </span>
              <span className="type-meta text-text-3">
                {choice.cloud ? "cloud" : "on this machine"}
              </span>
              {choice.id === models.current ? (
                <Check
                  className="ml-auto size-3.5 text-text-1"
                  strokeWidth={1.75}
                  aria-label="In use"
                />
              ) : null}
            </span>
            {choice.unavailable_reason ? (
              <span className="type-meta break-words text-text-3">{choice.unavailable_reason}</span>
            ) : null}
          </DropdownMenuItem>
        ))}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

/** The model and where it runs, said once, when there is no picker. */
function PlainMark() {
  const data = useAvailability().data;
  if (!data?.available || !data.model) return null;
  return (
    <span
      title={
        data.leaves_machine
          ? `${data.model} runs off this machine: your messages and the mail it reads go to its provider`
          : `${data.model} runs on this machine: nothing leaves it`
      }
    >
      {data.model} · {data.leaves_machine ? "cloud" : "local"}
    </span>
  );
}
