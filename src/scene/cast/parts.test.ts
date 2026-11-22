import { existsSync, readdirSync, readFileSync, statSync } from "node:fs";
import { describe, expect, it } from "vitest";
import * as manifest from "./parts";
import { BUILDS } from "../../lib/modelTiers";
import { TOOL_WOOD } from "../../lib/woods";
import { TIER_BUILDS } from "./parts";

interface Part {
  readonly w: number;
  readonly h: number;
}

const isPart = (v: unknown): v is Part =>
  typeof v === "object" && v !== null && "w" in v && "h" in v && typeof v.w === "number" && typeof v.h === "number";

const isPartSet = (v: unknown): v is Readonly<Record<string, Part>> =>
  typeof v === "object" && v !== null && Object.values(v).every(isPart);

const RAW = "art/cast/raw";
const CAST = "src/assets/cast";
const PERSON = ["chair", "forearm-l", "forearm-r", "head", "head-closed", "leg-l", "leg-r", "pelvis", "torso", "upper-arm-l", "upper-arm-r"];
const FROM_MEDIUM = ["head", "head-closed", "chair"];
const WOODS = Object.values(TOOL_WOOD);

const sheets = readdirSync(RAW)
  .filter((f) => f.endsWith("-sheet.png"))
  .map((f) => f.slice(0, -"-sheet.png".length));
const ident = (name: string): string => name.replace(/-/g, "_").toUpperCase();
const sets = Object.entries(manifest).flatMap(([k, v]) => (k !== "TIER_BUILDS" && isPartSet(v) ? [[k, v] as const] : []));
const setOf = (name: string): Readonly<Record<string, Part>> | undefined => sets.find(([k]) => k === ident(name))?.[1];

describe("parts.ts and the cut art", () => {
  it("has an export for every raw sheet and a raw sheet for every export", () => {
    expect(sheets.map(ident).sort()).toEqual(sets.map(([k]) => k).sort());
    expect(Object.keys(manifest).filter((k) => k !== "TIER_BUILDS").sort()).toEqual(sets.map(([k]) => k).sort());
  });

  it("has every part's PNG at the manifest's size", () => {
    for (const name of sheets) {
      for (const [part, size] of Object.entries(setOf(name) ?? {})) {
        const png = readFileSync(`${CAST}/${name}/${part}.png`);
        expect([name, part, png.readUInt32BE(16), png.readUInt32BE(20)]).toEqual([name, part, size.w, size.h]);
      }
    }
  });

  it("keeps no folder or file the manifest does not name", () => {
    for (const folder of readdirSync(CAST)) {
      expect(sheets).toContain(folder);
      const named = Object.keys(setOf(folder) ?? {}).map((p) => `${p}.png`).sort();
      expect([folder, readdirSync(`${CAST}/${folder}`).sort()]).toEqual([folder, named]);
    }
  });

  it("builds every tier worker in six builds, the face and chair from medium", () => {
    expect(Object.keys(TIER_BUILDS).sort()).toEqual(["kit", "nell", "otto", "vera"]);
    for (const [worker, builds] of Object.entries(TIER_BUILDS)) {
      expect([worker, Object.keys(builds)]).toEqual([worker, BUILDS]);
      for (const [build, parts] of Object.entries(builds)) {
        const want = build === "medium" ? PERSON : PERSON.filter((p) => !FROM_MEDIUM.includes(p));
        expect([worker, build, Object.keys(parts).sort()]).toEqual([worker, build, want]);
      }
    }
  });

  it("ships a small tile beside every raw wood swatch", () => {
    for (const wood of WOODS) {
      expect(statSync(`src/assets/wood/${wood}.jpg`).size).toBeLessThan(30_000);
      expect(existsSync(`art/wood/raw/${wood}.png`)).toBe(true);
    }
  });
});
