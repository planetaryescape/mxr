import * as AlertDialogPrimitive from "@radix-ui/react-alert-dialog";
import * as React from "react";

import { buttonVariants } from "@/components/ui/button";
import { focusActivePane } from "@/lib/keys/focusPane";
import { pressedOverlayKey } from "@/lib/keys/overlayKeys";
import { cn } from "@/lib/utils";

import { overlayMotion, scrimMotion, sheetMotion } from "./motion";

const AlertDialog = AlertDialogPrimitive.Root;
const AlertDialogTrigger = AlertDialogPrimitive.Trigger;
const AlertDialogPortal = AlertDialogPrimitive.Portal;

const AlertDialogOverlay = React.forwardRef<
  React.ElementRef<typeof AlertDialogPrimitive.Overlay>,
  React.ComponentPropsWithoutRef<typeof AlertDialogPrimitive.Overlay>
>(({ className, ...props }, ref) => (
  <AlertDialogPrimitive.Overlay
    ref={ref}
    className={cn(
      "fixed inset-0 z-50 bg-background/70 backdrop-blur-sm data-[state=open]:animate-in data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=open]:fade-in-0",
      scrimMotion,
      className,
    )}
    {...props}
  />
));
AlertDialogOverlay.displayName = AlertDialogPrimitive.Overlay.displayName;

/**
 * A yes-or-no confirm's keys (blueprint 35's overlay grammar): `y` does what
 * the action button names and `n` is Esc. Enter and Tab stay the buttons'
 * own, so a confirm that opens on its cancel button destroys nothing on a
 * bare Enter.
 */
function answerYesOrNo(event: React.KeyboardEvent<HTMLDivElement>) {
  const target = event.target;
  if (
    target instanceof HTMLElement &&
    (target.isContentEditable || target.closest("input, textarea, select"))
  ) {
    return;
  }
  const yes = pressedOverlayKey(event, "overlay.yes");
  if (!yes && !pressedOverlayKey(event, "overlay.no")) return;
  const buttons = event.currentTarget.querySelectorAll<HTMLButtonElement>(
    yes ? "[data-alert-action]:not(:disabled)" : "[data-alert-cancel]",
  );
  // Two actions would make `y` ambiguous.
  if (buttons.length !== 1) return;
  event.preventDefault();
  buttons[0]?.click();
}

const AlertDialogContent = React.forwardRef<
  React.ElementRef<typeof AlertDialogPrimitive.Content>,
  React.ComponentPropsWithoutRef<typeof AlertDialogPrimitive.Content> & {
    /** Answer to `y` and `n` (a yes-or-no confirm). */
    yesNo?: boolean;
  }
>(({ className, onCloseAutoFocus, onKeyDown, yesNo, ...props }, ref) => (
  <AlertDialogPortal>
    <AlertDialogOverlay />
    <AlertDialogPrimitive.Content
      ref={ref}
      onKeyDown={(event) => {
        onKeyDown?.(event);
        if (yesNo && !event.defaultPrevented) answerYesOrNo(event);
      }}
      onCloseAutoFocus={(event) => {
        onCloseAutoFocus?.(event);
        if (event.defaultPrevented) return;
        // Opened from a shortcut (send confirm, discard) there is no trigger to return to.
        requestAnimationFrame(() => {
          if (document.activeElement === document.body) focusActivePane();
        });
      }}
      className={cn(
        "fixed left-1/2 top-1/2 z-50 grid w-full max-w-lg -translate-x-1/2 -translate-y-1/2 gap-4 border border-border bg-popover p-6 text-popover-foreground shadow-2xl data-[state=open]:animate-in data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=open]:fade-in-0 sm:rounded-none",
        overlayMotion,
        sheetMotion,
        className,
      )}
      {...props}
    />
  </AlertDialogPortal>
));
AlertDialogContent.displayName = AlertDialogPrimitive.Content.displayName;

const AlertDialogHeader = ({ className, ...props }: React.HTMLAttributes<HTMLDivElement>) => (
  <div className={cn("flex flex-col gap-1.5 text-left", className)} {...props} />
);
AlertDialogHeader.displayName = "AlertDialogHeader";

const AlertDialogFooter = ({ className, ...props }: React.HTMLAttributes<HTMLDivElement>) => (
  <div
    className={cn("flex flex-col-reverse gap-2 sm:flex-row sm:justify-end", className)}
    {...props}
  />
);
AlertDialogFooter.displayName = "AlertDialogFooter";

const AlertDialogTitle = React.forwardRef<
  React.ElementRef<typeof AlertDialogPrimitive.Title>,
  React.ComponentPropsWithoutRef<typeof AlertDialogPrimitive.Title>
>(({ className, ...props }, ref) => (
  <AlertDialogPrimitive.Title
    ref={ref}
    className={cn("text-md font-semibold leading-none tracking-tight", className)}
    {...props}
  />
));
AlertDialogTitle.displayName = AlertDialogPrimitive.Title.displayName;

const AlertDialogDescription = React.forwardRef<
  React.ElementRef<typeof AlertDialogPrimitive.Description>,
  React.ComponentPropsWithoutRef<typeof AlertDialogPrimitive.Description>
>(({ className, ...props }, ref) => (
  <AlertDialogPrimitive.Description
    ref={ref}
    className={cn("text-xs text-muted-foreground", className)}
    {...props}
  />
));
AlertDialogDescription.displayName = AlertDialogPrimitive.Description.displayName;

const AlertDialogAction = React.forwardRef<
  React.ElementRef<typeof AlertDialogPrimitive.Action>,
  React.ComponentPropsWithoutRef<typeof AlertDialogPrimitive.Action> &
    Parameters<typeof buttonVariants>[0]
>(({ className, variant, size, ...props }, ref) => (
  <AlertDialogPrimitive.Action
    ref={ref}
    data-alert-action=""
    className={cn(buttonVariants({ variant, size, className }))}
    {...props}
  />
));
AlertDialogAction.displayName = AlertDialogPrimitive.Action.displayName;

const AlertDialogCancel = React.forwardRef<
  React.ElementRef<typeof AlertDialogPrimitive.Cancel>,
  React.ComponentPropsWithoutRef<typeof AlertDialogPrimitive.Cancel> &
    Parameters<typeof buttonVariants>[0]
>(({ className, variant = "outline", size, ...props }, ref) => (
  <AlertDialogPrimitive.Cancel
    ref={ref}
    data-alert-cancel=""
    className={cn(buttonVariants({ variant, size, className }))}
    {...props}
  />
));
AlertDialogCancel.displayName = AlertDialogPrimitive.Cancel.displayName;

export {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogOverlay,
  AlertDialogPortal,
  AlertDialogTitle,
  AlertDialogTrigger,
};
