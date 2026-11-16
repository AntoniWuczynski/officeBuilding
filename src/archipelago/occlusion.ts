// Which people a hut or hill hides from the camera. People are page elements over the canvas, so the scene cannot hide them.
import * as THREE from "three";

/** Raycaster never looks at `visible`, and a mesh inside a hidden group still says visible: the whole chain has to. */
export function isShown(o: THREE.Object3D | null): boolean {
  for (let n = o; n !== null; n = n.parent) if (!n.visible) return false;
  return true;
}

/** The agent a hut belongs to (userData.family), found from any mesh inside it. */
function familyOf(o: THREE.Object3D | null): unknown {
  for (let n = o; n !== null; n = n.parent) if (n.userData["family"] !== undefined) return n.userData["family"];
  return undefined;
}

export interface Subject {
  readonly key: string;
  /** An agent's own hut and its subagents' small huts stand round it: they never hide it. */
  readonly family: string;
  /** The point the camera must be able to see. */
  readonly chest: THREE.Vector3;
}

/** Two moves closer than this (world units) are the same place. */
const SAME = 1e-4;

export class Occluder {
  private seen: number[] = [];
  private seenKeys = "";
  private hidden = new Map<string, boolean>();

  constructor(private readonly ray: THREE.Raycaster = new THREE.Raycaster()) {}

  /**
   * Who is hidden. A raycast per person against every hut costs people times huts, so the answer is kept until the
   * camera, a person, or a hut (its place or visibility) has moved.
   */
  update(eye: THREE.Vector3, subjects: readonly Subject[], blockers: THREE.Object3D[]): ReadonlyMap<string, boolean> {
    const now: number[] = [eye.x, eye.y, eye.z];
    for (const b of blockers) now.push(b.position.x, b.position.y, b.position.z, isShown(b) ? 1 : 0);
    for (const s of subjects) now.push(s.chest.x, s.chest.y, s.chest.z);
    const keys = `${blockers.length}|${subjects.map((s) => s.key).join("\u0000")}`;
    if (keys === this.seenKeys && now.length === this.seen.length && now.every((v, i) => Math.abs(v - (this.seen[i] ?? NaN)) < SAME)) return this.hidden;
    this.seen = now;
    this.seenKeys = keys;
    // The raycaster reads each mesh's world matrix, which the renderer refreshes only when it draws.
    for (const b of blockers) b.updateMatrixWorld(true);
    const result = new Map<string, boolean>();
    const dir = new THREE.Vector3();
    for (const s of subjects) {
      dir.copy(s.chest).sub(eye);
      const far = dir.length() - 0.5;
      this.ray.set(eye, dir.normalize());
      this.ray.far = far;
      result.set(s.key, this.ray.intersectObjects(blockers, true).some((hit) => isShown(hit.object) && familyOf(hit.object) !== s.family));
    }
    this.hidden = result;
    return result;
  }
}
