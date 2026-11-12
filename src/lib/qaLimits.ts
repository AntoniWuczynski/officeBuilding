/** Minutes between GitHub checks (`QA_POLL_MINUTES` in settings.rs). */
export const QA_POLL_MINUTES = { min: 1, max: 60 } as const;
/** Reviews running at once (`QA_MAX_CONCURRENT` in settings.rs). */
export const QA_MAX_CONCURRENT = { min: 1, max: 4 } as const;
/** Reviews of one pull request in 24 hours (`QA_REVIEWS_PER_DAY` in settings.rs). */
export const QA_REVIEWS_PER_DAY = { min: 1, max: 20 } as const;

/** A range in words, as the backend words it: "between 1 and 60". */
export const between = (range: { readonly min: number; readonly max: number }): string => `between ${range.min} and ${range.max}`;

/** Whether `n` is a whole number inside `range`. */
export const inRange = (n: number, range: { readonly min: number; readonly max: number }): boolean => Number.isInteger(n) && n >= range.min && n <= range.max;
