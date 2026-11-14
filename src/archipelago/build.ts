// Low-poly island meshes from plain three primitives. Cheap-looking on purpose.
import * as THREE from "three";
import type { IslandSpec, IslandTheme } from "./islands";

function rng(seed: number): () => number {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

const mat = (color: number): THREE.MeshStandardMaterial => new THREE.MeshStandardMaterial({ color, flatShading: true, roughness: 1 });
const M = {
  rock: mat(0x8a8f99),
  rockDark: mat(0x6b7079),
  sand: mat(0xf0d9a0),
  grass: mat(0x6cc04a),
  grassDark: mat(0x4fa03a),
  trunk: mat(0x8a5a33),
  leaf: mat(0x2f9e44),
  wood: mat(0xb9824a),
  roof: mat(0xd9534f),
  white: mat(0xf4f1ea),
  dish: mat(0xdfe6ee),
};

/** A wobbly closed outline: the island's coastline. */
function blob(radius: number, rand: () => number, steps = 28): THREE.Shape {
  const phase = rand() * 6.28;
  const lobes = 2 + Math.floor(rand() * 3);
  const s = new THREE.Shape();
  for (let i = 0; i <= steps; i++) {
    const a = (i / steps) * Math.PI * 2;
    const r = radius * (1 + 0.16 * Math.sin(a * lobes + phase) + 0.07 * Math.sin(a * 5 + phase * 2));
    const x = Math.cos(a) * r;
    const y = Math.sin(a) * r;
    if (i === 0) s.moveTo(x, y);
    else s.lineTo(x, y);
  }
  return s;
}

function layer(shape: THREE.Shape, depth: number, material: THREE.Material, y: number): THREE.Mesh {
  const geo = new THREE.ExtrudeGeometry(shape, { depth, bevelEnabled: false });
  geo.rotateX(-Math.PI / 2);
  const mesh = new THREE.Mesh(geo, material);
  mesh.position.y = y;
  return mesh;
}

/** Shapes every palm shares; the trunk is one unit tall and scaled per tree. Never disposed. */
const PALM = { trunk: new THREE.CylinderGeometry(0.12, 0.2, 1, 5), leaf: new THREE.ConeGeometry(0.28, 1.5, 4) };

function palm(rand: () => number): THREE.Group {
  const g = new THREE.Group();
  const h = 2.2 + rand() * 1.2;
  const trunk = new THREE.Mesh(PALM.trunk, M.trunk);
  trunk.scale.y = h;
  trunk.position.y = h / 2;
  trunk.rotation.z = (rand() - 0.5) * 0.3;
  g.add(trunk);
  for (let i = 0; i < 5; i++) {
    const leaf = new THREE.Mesh(PALM.leaf, M.leaf);
    leaf.position.set(Math.cos(i * 1.256) * 0.55, h, Math.sin(i * 1.256) * 0.55);
    leaf.rotation.set(Math.sin(i * 1.256) * 1.2, 0, -Math.cos(i * 1.256) * 1.2);
    g.add(leaf);
  }
  return g;
}

function rock(rand: () => number): THREE.Mesh {
  const m = new THREE.Mesh(new THREE.DodecahedronGeometry(0.5 + rand() * 0.9, 0), rand() > 0.5 ? M.rock : M.rockDark);
  m.scale.y = 0.7;
  return m;
}

const SHARED = new Set<THREE.BufferGeometry>([PALM.trunk, PALM.leaf]);

/**
 * Give a built object its own copies of the shared materials, so it can fade or dim on its own (rise, sink, search).
 * Returns them for `setOpacity` and `release`.
 */
export function ownMaterials(root: THREE.Object3D): THREE.Material[] {
  const copies = new Map<THREE.Material, THREE.Material>();
  root.traverse((o) => {
    if (!(o instanceof THREE.Mesh)) return;
    const mesh: THREE.Mesh = o;
    const original = mesh.material;
    if (Array.isArray(original)) return;
    let copy = copies.get(original);
    if (copy === undefined) {
      copy = original.clone();
      copies.set(original, copy);
    }
    o.material = copy;
  });
  return [...copies.values()];
}

export function setOpacity(materials: readonly THREE.Material[], opacity: number): void {
  const solid = opacity >= 0.999;
  for (const m of materials) {
    m.opacity = opacity;
    if (m.transparent === !solid) continue;
    // Switching between opaque and transparent changes the shader: without needsUpdate the material keeps the
    // program it was first drawn with, and an island first drawn opaque would never fade.
    m.transparent = !solid;
    m.depthWrite = solid;
    m.needsUpdate = true;
  }
}

/** Free what `ownMaterials` copied and the object's own geometry; shared shapes stay. */
export function release(root: THREE.Object3D, materials: readonly THREE.Material[]): void {
  root.traverse((o) => {
    if (!(o instanceof THREE.Mesh)) return;
    const mesh: THREE.Mesh = o;
    if (!SHARED.has(mesh.geometry)) mesh.geometry.dispose();
  });
  for (const m of materials) m.dispose();
}

/** Height of an agent hut's counter: the person's feet stand on it. */
export const COUNTER_H = 1.3;
const STALL = {
  counter: new THREE.BoxGeometry(3.4, COUNTER_H, 2.2),
  wall: new THREE.BoxGeometry(5.2, 8.8, 0.35),
  post: new THREE.CylinderGeometry(0.22, 0.22, 9.6, 5),
  roof: new THREE.ConeGeometry(3.7, 2.4, 4),
};
for (const g of Object.values(STALL)) SHARED.add(g);

/**
 * An agent's hut: a counter to sit behind, a back wall, two posts and a roof, sized for a 7.5-unit person.
 * Origin is the ground in front of the counter. An agent working in a git worktree gets a palm beside it.
 */
export function buildStall(worktree: boolean, seed: number): THREE.Group {
  const g = new THREE.Group();
  const counter = new THREE.Mesh(STALL.counter, M.wood);
  counter.position.set(0, COUNTER_H / 2, 0.3);
  const wall = new THREE.Mesh(STALL.wall, M.trunk);
  wall.position.set(0, COUNTER_H + 4.4, -1.7);
  const roof = new THREE.Mesh(STALL.roof, M.roof);
  roof.position.set(0, COUNTER_H + 10.8, -0.4);
  roof.rotation.y = Math.PI / 4;
  g.add(counter, wall, roof);
  for (const x of [-2.5, 2.5]) {
    const post = new THREE.Mesh(STALL.post, M.trunk);
    post.position.set(x, COUNTER_H + 4.8, 0.2);
    g.add(post);
  }
  if (worktree) {
    const tree = palm(rng(seed));
    tree.scale.setScalar(1.5);
    tree.position.set(4.2, 0, 0.4);
    g.add(tree);
  }
  return g;
}

function dish(): THREE.Group {
  const g = new THREE.Group();
  const pole = new THREE.Mesh(new THREE.CylinderGeometry(0.1, 0.1, 1.2, 5), M.rockDark);
  pole.position.y = 0.6;
  const bowl = new THREE.Mesh(new THREE.SphereGeometry(0.9, 8, 4, 0, Math.PI * 2, 0, Math.PI / 2), M.dish);
  bowl.position.y = 1.3;
  bowl.rotation.x = Math.PI * 0.8;
  g.add(pole, bowl);
  return g;
}

function lighthouse(): THREE.Group {
  const g = new THREE.Group();
  const tower = new THREE.Mesh(new THREE.CylinderGeometry(0.6, 0.9, 4, 8), M.white);
  tower.position.y = 2;
  const band = new THREE.Mesh(new THREE.CylinderGeometry(0.68, 0.76, 0.8, 8), M.roof);
  band.position.y = 2.2;
  const cap = new THREE.Mesh(new THREE.ConeGeometry(0.8, 0.9, 8), M.roof);
  cap.position.y = 4.4;
  g.add(tower, band, cap);
  return g;
}

function dock(): THREE.Group {
  const g = new THREE.Group();
  const planks = new THREE.Mesh(new THREE.BoxGeometry(4.5, 0.15, 1.1), M.wood);
  planks.position.y = 3.3;
  g.add(planks);
  const boat = new THREE.Mesh(new THREE.BoxGeometry(2, 0.5, 0.9), M.roof);
  boat.position.set(1.2, 2.2, 1.6);
  g.add(boat);
  return g;
}

/** No huts by default: the only huts on an island are its agents' (buildStall). Themes differ by equipment. */
const PROPS: Record<IslandTheme, readonly (() => THREE.Object3D)[]> = {
  workshop: [],
  observatory: [dish, dish],
  control: [dish],
  farm: [],
  lighthouse: [lighthouse],
};

/** One island: rock base, sand, grass, then props placed by the island's seed. Origin is sea level. */
export function buildIsland(spec: IslandSpec): THREE.Group {
  const rand = rng(spec.seed);
  const g = new THREE.Group();
  const r = spec.radius;
  g.add(layer(blob(r, rng(spec.seed), 28), 3.6, M.rock, -1.6));
  g.add(layer(blob(r * 0.94, rng(spec.seed), 28), 3.9, M.sand, -1.6));
  const grassShape = blob(r * 0.76, rng(spec.seed + 1), 24);
  g.add(layer(grassShape, 4.6, M.grass, -1.6));
  const hill = layer(blob(r * 0.4, rng(spec.seed + 2), 18), 5.2, M.grassDark, -1.6);
  hill.position.x = (rand() - 0.5) * r * 0.3;
  g.add(hill);

  const top = 3.1;
  PROPS[spec.theme].forEach((make, i) => {
    const o = make();
    o.scale.setScalar(Math.max(1, r / 14));
    const a = (i / PROPS[spec.theme].length) * Math.PI * 2 + rand();
    o.position.set(Math.cos(a) * r * 0.22, top + 0.2, Math.sin(a) * r * 0.22);
    o.rotation.y = rand() * 6.28;
    g.add(o);
  });
  for (let i = 0; i < Math.round(r * 0.6); i++) {
    const a = rand() * 6.28;
    const d = r * (0.45 + rand() * 0.4);
    const o = rand() > 0.35 ? palm(rand) : rock(rand);
    o.position.set(Math.cos(a) * d, o instanceof THREE.Mesh ? 1.4 : 2.0, Math.sin(a) * d);
    g.add(o);
  }
  const d = dock();
  d.position.set(r * 0.95, 0, 0);
  g.add(d);
  g.position.set(spec.x, 0, spec.z);
  g.userData["projectId"] = spec.projectId;
  return g;
}

/** Shared water: a big plane that gently heaves. Returned with its animate step. */
/** World units across the sea plane before scaling; the view scales it to cover its draw distance. */
export const OCEAN_SIZE = 1600;

export function buildOcean(): { readonly mesh: THREE.Mesh<THREE.PlaneGeometry, THREE.MeshStandardMaterial>; readonly tick: (t: number) => void } {
  const geo = new THREE.PlaneGeometry(OCEAN_SIZE, OCEAN_SIZE, 70, 70);
  geo.rotateX(-Math.PI / 2);
  const base = geo.getAttribute("position");
  const y0 = Float32Array.from({ length: base.count }, (_, i) => base.getX(i) * 0.37 + base.getZ(i) * 0.21);
  const mesh = new THREE.Mesh(geo, new THREE.MeshStandardMaterial({ color: 0x1f9be0, flatShading: true, roughness: 0.4, transparent: true, opacity: 0.93 }));
  mesh.position.y = 0.4;
  return {
    mesh,
    tick: (t) => {
      for (let i = 0; i < base.count; i++) base.setY(i, Math.sin(t * 0.8 + (y0[i] ?? 0)) * 0.18);
      base.needsUpdate = true;
    },
  };
}
