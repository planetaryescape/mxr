import { useParams } from "@tanstack/react-router";

import { ModeFrame } from "@/components/ModeFrame";
import { SectionHeading } from "@/components/SectionHeading";
import { Skeleton, SkeletonBar, SkeletonRow } from "@/components/Skeleton";
import { ResizablePanel, ResizablePanelGroup } from "@/components/ui/resizable";
import { SINGLE_PANE_QUERY, useMediaQuery } from "@/hooks/useMediaQuery";
import { useSplitPane } from "@/hooks/useSplitPane";

import { CARD_PANE_SIZE } from "./archiveLayout";

export function ArchiveSkeleton({ shown }: { shown: boolean }) {
  const singlePane = useMediaQuery(SINGLE_PANE_QUERY);
  const params = useParams({ strict: false });
  const split = !singlePane && !params.threadId;
  const pane = useSplitPane("archive-card", CARD_PANE_SIZE, { active: split });
  return (
    <Skeleton shown={shown} label="Loading Archive" className="flex min-h-0 flex-1 overflow-hidden">
      <ModeFrame width="wide" className="flex min-h-0 flex-1">
        <ResizablePanelGroup className="min-h-0 flex-1" {...pane.groupProps}>
          <ResizablePanel
            id="archive-ledger-loading"
            {...pane.otherPanelProps}
            className="min-h-0 overflow-hidden pb-6"
          >
            <div
              data-testid="archive-search-placeholder"
              className="mx-5 mt-3 flex h-10 items-center border border-rule px-3"
            >
              <SkeletonBar className="w-2/5" />
            </div>
            <section className="mx-5 mt-4">
              <SectionHeading>Coming up</SectionHeading>
              <div className="mt-1 grid gap-0.5">
                {Array.from({ length: 4 }, (_, index) => (
                  <span key={index} className="flex min-h-8 items-center">
                    <SkeletonBar className="w-3/4" />
                  </span>
                ))}
              </div>
            </section>
            <div
              data-testid="archive-filters-placeholder"
              className="mx-5 mt-4 flex h-7 items-center gap-3 overflow-hidden"
            >
              <SkeletonBar className="h-7 w-3/4" />
              <SkeletonBar className="ml-auto w-12" />
            </div>
            <div className="mt-2">
              <SectionHeading className="mx-5 mb-1 mt-4 flex justify-between">
                <SkeletonBar className="w-28" />
                <SkeletonBar className="w-20" />
              </SectionHeading>
              <ul className="grid grid-cols-[minmax(0,1fr)] gap-0.5">
                {Array.from({ length: 4 }, (_, index) => (
                  <SkeletonRow key={index} lead="amount" />
                ))}
              </ul>
            </div>
          </ResizablePanel>
          {split ? (
            <ResizablePanel
              id="archive-card-loading"
              {...pane.sidePanelProps}
              className="border-l border-rule"
            >
              <div data-testid="archive-card-placeholder" className="px-5 pt-4">
                <SkeletonBar className="w-20" />
                <div className="mt-3">
                  <SkeletonBar className="w-2/3" />
                </div>
                <div className="mt-2">
                  <SkeletonBar className="w-1/2 opacity-70" />
                </div>
                {Array.from({ length: 3 }, (_, index) => (
                  <div key={index} className="mt-5">
                    <SkeletonBar className="w-16 opacity-70" />
                    <div className="mt-2">
                      <SkeletonBar className="w-2/3" />
                    </div>
                  </div>
                ))}
              </div>
            </ResizablePanel>
          ) : null}
        </ResizablePanelGroup>
      </ModeFrame>
    </Skeleton>
  );
}
