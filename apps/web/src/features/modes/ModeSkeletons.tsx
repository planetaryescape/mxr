import { ModeColumns, ModeFrame } from "@/components/ModeFrame";
import { SectionHeading } from "@/components/SectionHeading";
import { Skeleton, SkeletonBar, SkeletonRow, type SkeletonRowProps } from "@/components/Skeleton";

function Rows({ count, ...row }: { count: number } & SkeletonRowProps) {
  return (
    <ul className="grid grid-cols-[minmax(0,1fr)]">
      {Array.from({ length: count }, (_, index) => (
        <SkeletonRow key={index} {...row} />
      ))}
    </ul>
  );
}

function Summary({ headline = false }: { headline?: boolean }) {
  return (
    <div className="mx-5 mt-3">
      <span className="flex h-5 items-center">
        <SkeletonBar className={headline ? "w-3/5" : "w-2/5"} />
      </span>
      {headline ? (
        <span className="mt-2 flex h-5 items-center">
          <SkeletonBar className="w-4/5 opacity-70" />
        </span>
      ) : null}
    </div>
  );
}

export function NowSkeleton({ shown }: { shown: boolean }) {
  return (
    <Skeleton shown={shown} label="Loading Now" className="min-h-0 flex-1 overflow-hidden pb-6">
      <ModeFrame width="wide">
        <Summary headline />
        <ModeColumns split>
          <div className="min-w-0">
            <SectionHeading className="mx-5 mb-1 mt-4">People</SectionHeading>
            <Rows count={3} lead="avatar" />
            <SectionHeading className="mx-5 mb-1 mt-4">Due soon</SectionHeading>
            <Rows count={3} lead="checkbox" />
          </div>
          <div className="min-w-0">
            <SectionHeading className="mx-5 mb-1 mt-4">Updates</SectionHeading>
            <Rows count={1} lead="source" />
            <SectionHeading className="mx-5 mb-1 mt-4">For tonight</SectionHeading>
            <Rows count={1} lead="source" />
          </div>
        </ModeColumns>
      </ModeFrame>
    </Skeleton>
  );
}

export function TodoSkeleton({ shown }: { shown: boolean }) {
  return (
    <Skeleton shown={shown} label="Loading To do" className="min-h-0 flex-1 overflow-hidden pb-6">
      <ModeFrame>
        <Summary headline />
        <SectionHeading className="mx-5 mb-1 mt-4">Now</SectionHeading>
        <Rows count={3} lead="checkbox" />
        <SectionHeading className="mx-5 mb-1 mt-4">Coming up</SectionHeading>
        <div className="mx-5 mt-2">
          <SkeletonBar className="w-20" />
        </div>
        <Rows count={2} lead="checkbox" />
        <div className="mx-5 mt-5">
          <SkeletonBar className="w-36" />
        </div>
      </ModeFrame>
    </Skeleton>
  );
}

export function UpdatesSkeleton({ shown }: { shown: boolean }) {
  return (
    <Skeleton shown={shown} label="Loading Updates" className="min-h-0 flex-1 overflow-hidden pb-6">
      <ModeFrame>
        <Summary />
        <SectionHeading className="mx-5 mb-1 mt-4">Needs a look</SectionHeading>
        <Rows count={3} lead="source" secondary={false} />
        <SectionHeading className="mx-5 mb-1 mt-4">Changed</SectionHeading>
        <Rows count={4} lead="source" secondary={false} />
        <SectionHeading className="mx-5 mb-1 mt-4">Routine</SectionHeading>
      </ModeFrame>
    </Skeleton>
  );
}

export function ReadingSkeleton({ shown, later = false }: { shown: boolean; later?: boolean }) {
  return (
    <Skeleton
      shown={shown}
      label={later ? "Loading Later" : "Loading Reading"}
      className="min-h-0 flex-1 overflow-hidden pb-10"
    >
      <ModeFrame>
        {later ? null : (
          <SectionHeading className="mx-5 mb-1 mt-5">Since you were last here</SectionHeading>
        )}
        <Rows count={4} lead="source" metadataClassName="hidden sm:block" />
        {later ? null : (
          <>
            <SectionHeading className="mx-5 mb-1 mt-5">
              <SkeletonBar className="w-24" />
            </SectionHeading>
            <Rows count={3} lead="source" metadataClassName="hidden sm:block" />
          </>
        )}
      </ModeFrame>
    </Skeleton>
  );
}

/** Same band inset and heading line box as MessagesList, inside its existing panes. */
export function MessagesSkeleton({ shown }: { shown: boolean }) {
  return (
    <Skeleton shown={shown} label="Loading Messages" className="pt-2">
      <div className="mt-1">
        <SectionHeading className="px-5 pb-1 pt-2">
          <SkeletonBar className="h-3.5 w-20" />
        </SectionHeading>
        <Rows count={4} lead="avatar" />
      </div>
    </Skeleton>
  );
}
