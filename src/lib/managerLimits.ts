/** Desks a manager may have hired at once (`MANAGER_MAX_DESKS` in settings.rs). */
export const MANAGER_MAX_DESKS = { min: 1, max: 10 } as const;
/** Ceiling on the summed spend of a manager's desks, in whole US dollars (`MANAGER_SPEND_CEILING_USD` in settings.rs). */
export const MANAGER_SPEND_CEILING_USD = { min: 1, max: 1000 } as const;
