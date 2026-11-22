const ROW_ESTIMATE_PX = 44;

export function halfPageRows(): number {
  return Math.max(1, Math.floor(window.innerHeight / 2 / ROW_ESTIMATE_PX));
}

export function listMotions(move: (delta: number) => void) {
  return {
    top: () => move(-Number.MAX_SAFE_INTEGER),
    bottom: () => move(Number.MAX_SAFE_INTEGER),
    pageDown: () => move(halfPageRows()),
    pageUp: () => move(-halfPageRows()),
  };
}
