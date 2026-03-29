export function SkeletonReaderHeader() {
  return (
    <div className="space-y-3 px-3 py-3">
      <div className="h-4 w-48 skeleton" />
      <div className="h-5 w-3/4 skeleton" />
      <div className="flex items-center gap-2">
        <div className="h-3 w-32 skeleton" />
        <div className="h-3 w-20 skeleton" />
      </div>
    </div>
  );
}

export function SkeletonReaderBody() {
  return (
    <div className="space-y-4 px-3 py-4">
      <div className="space-y-2">
        <div className="h-3 w-full skeleton" />
        <div className="h-3 w-11/12 skeleton" />
        <div className="h-3 w-4/5 skeleton" />
      </div>
      <div className="space-y-2">
        <div className="h-3 w-full skeleton" />
        <div className="h-3 w-3/4 skeleton" />
        <div className="h-3 w-5/6 skeleton" />
        <div className="h-3 w-2/3 skeleton" />
      </div>
      <div className="space-y-2">
        <div className="h-3 w-full skeleton" />
        <div className="h-3 w-1/2 skeleton" />
      </div>
    </div>
  );
}

export function SkeletonMailList(props: { count?: number }) {
  const count = props.count ?? 8;
  return (
    <div>
      {Array.from({ length: count }, (_, i) => (
        <div
          key={i}
          className="flex h-[var(--row-height)] min-h-[var(--row-height)] items-start gap-2.5 border-l-2 border-l-transparent px-2.5 py-2"
        >
          <span className="mt-[7px] size-2 shrink-0 rounded-full skeleton" />
          <div className="min-w-0 flex-1 space-y-2">
            <div className="flex items-center justify-between gap-4">
              <div className="h-3 skeleton" style={{ width: `${60 + (i % 3) * 20}px` }} />
              <div className="h-3 w-12 skeleton" />
            </div>
            <div className="h-3 skeleton" style={{ width: `${50 + (i % 4) * 10}%` }} />
            <div className="h-3 skeleton" style={{ width: `${30 + (i % 5) * 8}%` }} />
          </div>
        </div>
      ))}
    </div>
  );
}
