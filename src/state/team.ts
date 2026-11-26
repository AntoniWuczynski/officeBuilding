import type { ManagerFloor, Session } from "../types";

/** The seated desks split into A Human's own and the manager's team, the ones it hired (a note alone does not count), each in order. */
export function splitTeam(seated: readonly Session[], manager: ManagerFloor | undefined): { readonly own: readonly Session[]; readonly team: readonly Session[] } {
  const hired = new Set(manager?.hired ?? []);
  return { own: seated.filter((s) => !hired.has(s.id)), team: seated.filter((s) => hired.has(s.id)) };
}
