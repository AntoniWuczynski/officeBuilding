/** `items[index]`, or a failed test when it is not there. */
export function nth<T>(items: readonly T[], index: number): T {
  const item = items.at(index);
  if (item === undefined) throw new Error(`no item at ${index}`);
  return item;
}
