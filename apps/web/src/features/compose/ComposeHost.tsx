/*
 * The one compose surface (inline / overlay / fullscreen). Every entry
 * point opens it: `c`, the compose launcher, reply/forward, drafts, and the
 * /compose/* deep links. Mounted once in AppShell so the compose session
 * survives surface switches and route changes. The inline surface portals the
 * editor into the thread reader's slot (#inline-composer-slot); when no
 * slot exists (user navigated away) it falls back to the overlay so the
 * draft is never hidden.
 */

import * as DialogPrimitive from "@radix-ui/react-dialog";
import { Maximize2, Minimize2, PictureInPicture2, X } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";

import { PageError } from "@/components/PageParts";
import { Button } from "@/components/ui/button";
import { overlayMotion, scrimMotion, sheetMotion } from "@/components/ui/motion";
import { PHONE_QUERY, useMediaQuery } from "@/hooks/useMediaQuery";
import { focusActivePane } from "@/lib/keys/focusPane";
import { cn } from "@/lib/utils";
import { ComposeEditorPanel } from "./ComposeEditorPanel";
import { htmlDraftRefusal, HtmlDraftNotice } from "./HtmlDraftNotice";
import { useComposeUi, type ComposeSurface } from "./composeUiStore";
import { isComposeActionChord } from "./session/composeShortcuts";
import { useComposeSession, type ComposeIntent } from "./useComposeSession";

export function ComposeHost() {
  const intent = useComposeUi((s) => s.intent);
  if (!intent) return null;
  // Key by intent so a new reply target starts a fresh session while the
  // host itself stays mounted.
  return <ComposeHostInner key={intent.key} intent={intent} />;
}

function ComposeHostInner({ intent }: { intent: ComposeIntent }) {
  const surfaceTitle = intent.contextTitle
    ? `${intent.title} · ${intent.contextTitle}`
    : intent.title;
  const surface = useComposeUi((s) => s.surface);
  const setSurface = useComposeUi((s) => s.setSurface);
  const closeCompose = useComposeUi((s) => s.closeCompose);
  const [slot, setSlot] = useState<HTMLElement | null>(null);

  // A send finishes after its undo window, possibly once the user has moved
  // on to another composer; only close the surface if it still shows this
  // session.
  const closeThisSession = () => {
    if (useComposeUi.getState().intent?.key === intent.key) closeCompose();
  };
  const controller = useComposeSession(intent, {
    onSent: closeThisSession,
    onDiscarded: closeThisSession,
    onClose: closeThisSession,
  });

  // Commands for a page that hosts this session inline (focus mode). The
  // latest controller is read at call time.
  const controllerRef = useRef(controller);
  controllerRef.current = controller;
  // Registered once the session has loaded, so "the reply is ready" and
  // "its commands exist" are the same fact for a host page.
  const sessionReady = !controller.sessionLoading && controller.draft !== null;
  useEffect(() => {
    if (!sessionReady) return;
    const commands = {
      intentKey: intent.key,
      send: () => controllerRef.current.requestSend(),
      sendAndRemind: () => controllerRef.current.setRemindDialogOpen(true),
      // Opens the help line for the user to ask; nothing is sent until they submit.
      draftForMe: () => controllerRef.current.openHelp(),
    };
    useComposeUi.getState().setCommands(commands);
    return () => {
      if (useComposeUi.getState().commands === commands) useComposeUi.getState().setCommands(null);
    };
  }, [intent.key, sessionReady]);

  // The inline slot lives at the bottom of the thread reader. Re-resolve it
  // whenever the DOM changes, not only the route: a pane can unmount and
  // remount on the same path (Messages on a phone goes back to the people
  // and reopens the person), and a portal into a detached slot would hide
  // the draft for good. Without a slot the draft falls back to the overlay.
  useEffect(() => {
    if (surface !== "inline") {
      setSlot(null);
      return;
    }
    // Typing in the editor mutates the DOM too: only a new slot re-renders.
    let current: HTMLElement | null | undefined;
    const resolve = () => {
      const slotId = intent.contextThreadId
        ? `inline-composer-slot-${intent.contextThreadId}`
        : "inline-composer-slot";
      const next = document.getElementById(slotId);
      if (next !== current) {
        current = next;
        setSlot(next);
      }
    };
    resolve();
    const observer = new MutationObserver(resolve);
    observer.observe(document.body, { childList: true, subtree: true });
    return () => observer.disconnect();
  }, [intent.contextThreadId, surface]);

  // Bring the inline composer into view once it has content: the slot is
  // hidden while empty, so scrolling to it any earlier does nothing.
  const sessionLoading = controller.sessionLoading;
  useEffect(() => {
    if (!slot || sessionLoading) return;
    const frame = requestAnimationFrame(() => slot.scrollIntoView({ block: "nearest" }));
    return () => cancelAnimationFrame(frame);
  }, [slot, sessionLoading]);

  // An HTML-bodied draft is a permanent refusal, not a transient failure:
  // retrying can only fail again. Show the document instead.
  const htmlDraft = htmlDraftRefusal(controller.sessionError);
  const body = controller.sessionLoading ? (
    <div role="status" className="type-body flex h-40 items-center justify-center text-text-2">
      Opening {surfaceTitle.toLowerCase()}…
    </div>
  ) : htmlDraft ? (
    <HtmlDraftNotice refusal={htmlDraft} />
  ) : controller.sessionError ? (
    <div className="flex flex-col items-start gap-3 p-5">
      <PageError
        title={`Couldn't open ${surfaceTitle.toLowerCase()}`}
        error={controller.sessionError}
        onRetry={controller.retrySession}
      />
      {controller.reopenSavedDraft && (
        <Button size="sm" variant="outline" onClick={controller.reopenSavedDraft}>
          Open saved draft for review
        </Button>
      )}
    </div>
  ) : controller.draft ? (
    <ComposeEditorPanel controller={controller} />
  ) : null;

  // A sheet is something the user asked for (popout, fullscreen) and gets a scrim. An
  // inline draft whose thread is gone docks in the corner instead, so going back to the
  // list never throws a modal over it.
  const sheet = surface !== "inline";
  const inThread = surface === "inline" && slot !== null;
  // On a phone the corner window would cover most of the list, so the docked
  // draft is its title bar alone; reopening the conversation brings it back
  // inline, and Popout or Fullscreen open it in place.
  const phone = useMediaQuery(PHONE_QUERY);
  const dockedBar = phone && surface === "inline" && slot === null;
  const chrome = (
    <div
      data-compose-surface
      className={cn(
        "flex min-h-0 flex-col overflow-hidden border border-rule",
        inThread
          ? "max-h-[70vh] min-h-[320px] bg-background"
          : cn("bg-ink-overlay shadow-2xl", !dockedBar && "h-full"),
      )}
      onKeyDown={(event) => {
        if (isComposeActionChord(event, "compose.fullscreen")) {
          event.preventDefault();
          setSurface(surface === "fullscreen" ? "inline" : "fullscreen");
        }
      }}
    >
      <div className="flex h-12 shrink-0 items-center justify-between gap-3 border-b border-rule px-5">
        <h2 className="type-title min-w-0 truncate text-text-1">{surfaceTitle}</h2>
        <span className="-mr-2 flex items-center gap-1">
          <SurfaceButton
            label="Inline"
            active={surface === "inline"}
            onClick={() => setSurface("inline")}
            icon={<Minimize2 className="size-3.5" />}
          />
          <SurfaceButton
            label="Popout"
            active={surface === "overlay"}
            onClick={() => setSurface("overlay")}
            icon={<PictureInPicture2 className="size-3.5" />}
          />
          <SurfaceButton
            label="Fullscreen (⇧⌘F)"
            active={surface === "fullscreen"}
            onClick={() => setSurface("fullscreen")}
            icon={<Maximize2 className="size-3.5" />}
          />
          <Button
            variant="ghost"
            size="icon-sm"
            aria-label="Close composer (saves draft)"
            title="Close composer (saves draft)"
            className="text-text-2"
            onClick={() => void controller.requestClose()}
          >
            <X className="size-4" />
          </Button>
        </span>
      </div>
      {dockedBar ? null : <div className="flex min-h-0 flex-1 flex-col">{body}</div>}
    </div>
  );

  if (surface === "inline" && slot) {
    return createPortal(chrome, slot);
  }

  const fullscreen = surface === "fullscreen";
  if (!sheet) {
    // An inline draft whose thread is gone docks in the corner: not modal, so the list stays usable.
    return (
      <div
        role="dialog"
        aria-label={surfaceTitle}
        className={
          dockedBar
            ? "fixed inset-x-4 bottom-4 z-40 flex flex-col"
            : "fixed bottom-4 right-4 z-40 flex h-[min(640px,calc(100vh-6rem))] w-[min(680px,calc(100vw-3rem))] flex-col"
        }
      >
        {chrome}
      </div>
    );
  }
  // A popout or fullscreen sheet is modal: focus stays inside it, Esc leaves (the draft is
  // saved first), and focus returns to where it was. The scrim and the sheet fade and rise
  // like every overlay (docs/blueprint/26 "Motion").
  return (
    <DialogPrimitive.Root
      open
      onOpenChange={(open) => {
        if (!open) void controller.requestClose();
      }}
    >
      <DialogPrimitive.Portal>
        <DialogPrimitive.Overlay
          data-testid="compose-scrim"
          className={cn(
            "fixed inset-0 z-40 bg-scrim data-[state=open]:animate-in data-[state=open]:fade-in-0",
            scrimMotion,
          )}
        />
        <DialogPrimitive.Content
          aria-label={surfaceTitle}
          aria-describedby={undefined}
          // The editor focuses itself (useComposeAutofocus); the trap only has to hold it.
          onOpenAutoFocus={(event) => event.preventDefault()}
          onCloseAutoFocus={() => {
            // Radix puts focus back where it was; when that is gone (a key opened the sheet
            // from the page), the pane gets it.
            requestAnimationFrame(() => {
              if (document.activeElement === document.body) focusActivePane();
            });
          }}
          onInteractOutside={(event) => event.preventDefault()}
          onEscapeKeyDown={(event) => {
            // Esc belongs to vim inside the Markdown editor, and to an open suggestion list.
            const target = event.target instanceof Element ? event.target : null;
            if (target?.closest(".cm-editor, [aria-expanded='true']")) {
              event.preventDefault();
              return;
            }
            // Esc never destroys: on a proof it keeps the user's text, on the help
            // line it closes the line. Radix runs this in capture, before either
            // sees the key, so it has to be handled here.
            if (controller.proofOpen) {
              event.preventDefault();
              controller.keepMine();
            } else if (controller.helpOpen) {
              event.preventDefault();
              controller.closeHelp();
            }
          }}
          className={cn(
            "fixed z-40 flex flex-col outline-none data-[state=open]:animate-in data-[state=open]:fade-in-0",
            overlayMotion,
            sheetMotion,
            fullscreen
              ? "inset-0 sm:inset-4"
              : "inset-0 sm:inset-x-auto sm:bottom-4 sm:right-4 sm:top-14 sm:w-[min(720px,calc(100vw-2rem))]",
          )}
        >
          <DialogPrimitive.Title asChild>
            {/* The visible heading is in the chrome; this names the dialog without a second heading. */}
            <span className="sr-only">{surfaceTitle}</span>
          </DialogPrimitive.Title>
          {chrome}
        </DialogPrimitive.Content>
      </DialogPrimitive.Portal>
    </DialogPrimitive.Root>
  );
}

function SurfaceButton({
  label,
  active,
  onClick,
  icon,
}: {
  label: string;
  active: boolean;
  onClick: () => void;
  icon: React.ReactNode;
}) {
  return (
    <Button
      variant="ghost"
      size="icon-sm"
      aria-label={label}
      aria-pressed={active}
      title={label}
      className={active ? "bg-signal-soft text-text-1" : "text-text-2"}
      onClick={onClick}
    >
      {icon}
    </Button>
  );
}

/** Surface switch helper for ComposeSurface validation at call sites. */
export type { ComposeSurface };
