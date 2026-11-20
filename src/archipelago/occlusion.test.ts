import * as THREE from "three";
import { describe, expect, it, vi } from "vitest";
import { Occluder, isShown } from "./occlusion";
import type { Subject } from "./occlusion";

/** A wall between the camera at z=10 and a person at z=-10. */
function wall(family?: string): THREE.Group {
  const g = new THREE.Group();
  g.add(new THREE.Mesh(new THREE.BoxGeometry(4, 4, 1), new THREE.MeshBasicMaterial()));
  if (family !== undefined) g.userData["family"] = family;
  return g;
}
const eye = new THREE.Vector3(0, 0, 10);
const person = (family = "p"): Subject => ({ key: family, family, chest: new THREE.Vector3(0, 0, -10) });

describe("isShown", () => {
  it("is false for a mesh whose group is hidden, though the mesh itself says visible", () => {
    const g = wall();
    const mesh = g.children[0] as THREE.Mesh;
    g.visible = false;
    expect(mesh.visible).toBe(true);
    expect(isShown(mesh)).toBe(false);
    g.visible = true;
    expect(isShown(mesh)).toBe(true);
  });
});

describe("Occluder", () => {
  it("is hidden behind a shown hut, and not behind a hidden one or its own", () => {
    const g = wall();
    const occ = new Occluder();
    expect(occ.update(eye, [person()], [g]).get("p")).toBe(true);
    g.visible = false;
    expect(occ.update(eye, [person()], [g]).get("p")).toBe(false);
    g.visible = true;
    g.userData["family"] = "p";
    expect(occ.update(eye, [person("p")], [g]).get("p")).toBe(false);
  });

  it("is not hidden by a hut that is only inside a hidden group (a sinking island)", () => {
    const island = new THREE.Group();
    island.add(wall());
    island.visible = false;
    expect(new Occluder().update(eye, [person()], [island]).get("p")).toBe(false);
  });

  it("casts no rays at all while the camera, the people and the huts stand still", () => {
    const ray = new THREE.Raycaster();
    const spy = vi.spyOn(ray, "intersectObjects");
    const occ = new Occluder(ray);
    const g = wall();
    const people = [person("a"), person("b")];
    occ.update(eye, people, [g]);
    expect(spy).toHaveBeenCalledTimes(2);
    spy.mockClear();
    for (let frame = 0; frame < 60; frame++) occ.update(eye.clone(), [person("a"), person("b")], [g]);
    expect(spy).toHaveBeenCalledTimes(0);
  });

  it("casts again when the camera moves, a hut moves, or a hut is hidden", () => {
    const ray = new THREE.Raycaster();
    const spy = vi.spyOn(ray, "intersectObjects");
    const occ = new Occluder(ray);
    const g = wall();
    occ.update(eye, [person()], [g]);
    spy.mockClear();
    expect(occ.update(new THREE.Vector3(3, 0, 10), [person()], [g]).get("p")).toBe(true);
    expect(spy).toHaveBeenCalledTimes(1);
    g.position.x = 50;
    expect(occ.update(new THREE.Vector3(3, 0, 10), [person()], [g]).get("p")).toBe(false);
    expect(spy).toHaveBeenCalledTimes(2);
    g.position.x = 0;
    g.visible = false;
    occ.update(new THREE.Vector3(3, 0, 10), [person()], [g]);
    expect(spy).toHaveBeenCalledTimes(3);
  });
});
