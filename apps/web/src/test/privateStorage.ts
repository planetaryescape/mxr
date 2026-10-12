/** Synthetic browser storage for runtimes whose Node Storage masks jsdom. */
export function privateStorage(): Storage {
  const items = new Map<string, string>();
  return { get length() { return items.size; }, clear: () => items.clear(),
    getItem: (key) => items.get(key) ?? null, key: (index) => [...items.keys()][index] ?? null,
    removeItem: (key) => { items.delete(key); }, setItem: (key, value) => { items.set(key, String(value)); } };
}
