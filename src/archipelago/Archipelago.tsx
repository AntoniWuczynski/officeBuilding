// The island map: the building view's floors drawn as islands on one ocean. Same data, same actions.
import { useEffect, useMemo, useRef, useState } from "react";
import * as THREE from "three";
import { MapControls } from "three/examples/jsm/controls/MapControls.js";
import type { OfficeSnapshot, Project, Session } from "../types";
import { poseForState } from "../lib/animation";
import { hashString, lookFor } from "../lib/looks";
import { Person } from "../scene/Person";
import { questionsForProject, tripTag } from "../state/selectors";
import { useNewMessages } from "../state/useNewMessages";
import type { Present } from "../state/usePresence";
import { PlaceNameplate } from "../components/building/PlaceNameplate";
import { PLACE_PANEL_ID, PlacePanel } from "../components/building/PlacePanel";
import type { FloorModel } from "../components/building/Storey";
import { PLACE } from "../state/placeWords";
import { COUNTER_H, OCEAN_SIZE, buildBoat, buildIsland, buildOcean, buildStall, ownMaterials, release, setOpacity } from "./build";
import { agentSlots, assignSeats, clampToBounds, layoutIslands, MAX_SLOTS, mergeOrder, shortName, worldBounds } from "./islands";
import type { IslandSpec } from "./islands";
import { Occluder } from "./occlusion";
import type { Subject } from "./occlusion";
import { boxAt, roomFor } from "./pick";
import type { Box, Rect } from "./pick";
import { bezierAt, sailRoute } from "./routes";
import type { Route } from "./routes";
import "./archipelago.css";

const LIGHT = { green: "var(--ok)", yellow: "var(--wait)", red: "var(--fault)", off: "var(--off)" } as const;
/** Grass surface height in world units (build.ts: layer at -1.6, depth 4.6). */
const GRASS_Y = 3.0;
/** A subagent is a smaller person in a smaller hut. */
const HELPER_SCALE = 0.55;
/** A person is 30 css px tall; they stand this many world units tall (an island is 22+ units across). */
const PERSON_UNITS = 7.5;
const PERSON_PX = 30;
/** Below this on-screen scale a figure is mush; the label's counts say it instead. */
const MIN_SCALE = 0.7;
/** Names show from this on-screen scale: the figure is then as big as it is on a zoomed island (measured in the harness). */
const TAG_SCALE = 1;
/** How much of a label's opacity is left while a boat sails behind it. */
const SEE_THROUGH = 0.3;
/** How close a selected island's camera comes, in world units from its centre; it backs off if the island does not fit the free room. */
const FOCUS_DISTANCE = 52;
/** The list, signs and panel float 16 px off the edges; framed islands keep the same clear space round them. */
const FRAME_GAP = 16;
/** The coastline swings up to 23% past an island's radius (islands.ts closestPair, build.ts blob). */
const COAST_REACH = 1.23;
/** How high a label hangs above its island's centre, as the render loop places it. */
const LABEL_Y = 26;
/** North up with the opening tilt: the camera looks at the middle of the map from this side. */
const OPENING = new THREE.Vector3(0, 1, 1).normalize();
/** Open sea kept around the islands: the camera target cannot leave this box. */
const SEA_PAD = 40;
/** A parrot flies a message between two huts on one island. */
const NOTE_MS = 2600;
/** A boat carries a message between islands at this many world units a second, but never faster than a parrot's trip. */
const BOAT_SPEED = 38;
const BOAT_MIN_MS = 3200;
const BOAT_Y = 1.3;
/** Drawn at 5.6 units a boat is about 11 px at the overview zoom, too small to read as a boat; huts are drawn oversize the same way. */
const BOAT_SCALE = 3;
/** A drawn parrot: deep blue body and tail, red head and wing, yellow beak. Its green body vanished on the grass; this one is 3:1 or more on both greens. */
const PARROT_SVG =
  '<svg viewBox="0 0 34 24" width="34" height="24" aria-hidden="true"><path class="wing" d="M14 12 L6 2 L20 8 Z" fill="#e0453a" stroke="#1d2433" stroke-width="1.2" stroke-linejoin="round"/><ellipse cx="17" cy="14" rx="9" ry="5" fill="#15216e" stroke="#1d2433" stroke-width="1.2"/><circle cx="26" cy="10" r="4.2" fill="#e0453a" stroke="#1d2433" stroke-width="1.2"/><path d="M29 9 L34 11 L29.5 13 Z" fill="#f4c542" stroke="#1d2433" stroke-width="1"/><circle cx="26.8" cy="9" r="0.9" fill="#1d2433"/><path d="M8 14 L1 19 L9 17 Z" fill="#15216e" stroke="#1d2433" stroke-width="1" stroke-linejoin="round"/></svg>';
/** Islands rise from the sea one after another on open (as a storey is built), and sink when removed. */
const RISE_MS = 1100;
const RISE_STAGGER_MS = 110;
const RISE_DEPTH = 14;
const SINK_MS = 1400;
const SINK_DEPTH = 16;
/** How far a hovered island lifts, like a storey sliding out under the pointer. */
const HOVER_LIFT = 1.8;
/** A camera move to an island or back to north, inside DESIGN.md's 450 to 600 ms. */
const FLY_MS = 520;
/** How far search dims a non-matching island, its labels, huts and people. */
const DIM = 0.35;

interface Placed {
  readonly key: string;
  readonly island: string;
  readonly x: number;
  readonly y: number;
  readonly z: number;
  readonly session: Session;
  /** The agent a click opens: the session itself, or a subagent's parent. */
  readonly owner: Session;
  /** A subagent: smaller, beside its parent. */
  readonly helper: boolean;
  readonly dimmed: boolean;
}

/** Real sessions on their seats, each session's subagents standing beside it. `seats` remembers who sits where between calls. */
export function placePeople(rows: readonly { readonly spec: IslandSpec; readonly floor: Pick<FloorModel, "sessions" | "dimmed"> }[], seats: Map<string, ReadonlyMap<string, number>>): Placed[] {
  const out: Placed[] = [];
  // A sunk island's seats would otherwise stay for as long as this view is open.
  const standing = new Set(rows.map((r) => r.spec.projectId));
  for (const id of seats.keys()) if (!standing.has(id)) seats.delete(id);
  for (const { spec, floor } of rows) {
    const slots = agentSlots(spec);
    const own = assignSeats(seats.get(spec.projectId) ?? new Map(), floor.sessions.map((s) => s.id));
    seats.set(spec.projectId, own);
    floor.sessions.forEach((session) => {
      const slot = slots[own.get(session.id) ?? -1];
      if (slot === undefined) return;
      // A stuck agent lies on the grass in front of its hut, as it lies in front of its desk in the office.
      const stuck = session.state === "error";
      out.push({
        key: session.id,
        island: spec.projectId,
        x: slot.x,
        y: stuck ? GRASS_Y : GRASS_Y + COUNTER_H,
        z: stuck ? slot.z + 4 : slot.z,
        session,
        owner: session,
        helper: false,
        dimmed: floor.dimmed,
      });
      // Subagents stand in front of their parent, each behind a small hut of their own.
      session.helpers.slice(0, 3).forEach((h, j) => {
        out.push({
          key: `${session.id}/${h.id}`,
          island: spec.projectId,
          x: slot.x + (j - 1) * 3.6,
          y: GRASS_Y + COUNTER_H * HELPER_SCALE,
          z: slot.z + 5.2,
          session: { ...session, id: h.id, state: h.state, helpers: [] },
          owner: session,
          helper: true,
          dimmed: floor.dimmed,
        });
      });
    });
  }
  return out;
}

interface Props {
  readonly snapshot: OfficeSnapshot;
  /** The building view's floors with their arrive and leave phases, one island each. */
  readonly entries: readonly Present<FloorModel>[];
  /** An island that has finished sinking. */
  readonly onGone: (projectId: string) => void;
  /** False under prefers-reduced-motion. */
  readonly animate: boolean;
  /** Go inside an island: the floor view, with Hire, the terminal, decisions and the rest. */
  readonly onEnter: (projectId: string) => void;
  readonly onOpenSession: (session: Session) => void;
  /** Give an island a name of your own; a blank name restores the folder's. */
  readonly onRenameIsland: (projectId: string, name: string) => void;
  /** Put an agent on another island. */
  readonly onMoveSession: (sessionId: string, projectId: string) => void;
  readonly onAddFloor: () => void;
  readonly onDemolish: (projectId: string) => void;
}

const clamp01 = (n: number): number => Math.min(1, Math.max(0, n));

export function Archipelago({ snapshot, entries, onGone, animate, onEnter, onOpenSession, onRenameIsland, onMoveSession, onAddFloor, onDemolish }: Props): React.ReactElement {
  const host = useRef<HTMLDivElement>(null);
  const labels = useRef(new Map<string, HTMLButtonElement>());
  const personEls = useRef(new Map<string, HTMLDivElement>());
  const flyTo = useRef<(spec: IslandSpec) => void>(() => undefined);
  const resetView = useRef<() => void>(() => undefined);
  const needle = useRef<HTMLSpanElement>(null);
  const hover = useRef<string | null>(null);
  /** Islands that have already risen, so a rebuild for a new project does not make the rest rise again. */
  const risen = useRef(new Set<string>());
  const [selected, setSelected] = useState<string | null>(null);
  const selectedRef = useRef<string | null>(null);
  selectedRef.current = selected;
  /** True until the user moves the camera: while it is, the view re-frames itself as the list and signs change size. */
  const homed = useRef(true);
  const focus = useRef<(projectId: string) => void>(() => undefined);
  const panelHeading = useRef<HTMLHeadingElement>(null);
  const [noWebGl, setNoWebGl] = useState(false);
  // Reduced motion: still sea, instant camera moves, no flying notes (as the building's walks and collapses).
  const animateRef = useRef(animate);
  animateRef.current = animate;
  const onGoneRef = useRef(onGone);
  onGoneRef.current = onGone;
  // Read by the canvas click handler, which is set up once per scene.
  const openRef = useRef(onOpenSession);
  openRef.current = onOpenSession;
  const phases = useRef(new Map<string, Present<FloorModel>["phase"]>());
  phases.current = new Map(entries.map((e) => [e.key, e.phase]));

  // Layout order: islands keep their place for as long as this view is open; a removed one leaves a hole of sea.
  const order = useRef<string[]>([]);
  const known = useRef(new Map<string, Project>());
  for (const e of entries) known.current.set(e.key, e.item.project);
  order.current = mergeOrder(order.current, entries.map((e) => e.key));
  const signature = entries.map((e) => e.key).join("\u0000");
  const islands = useMemo(() => {
    const present = new Set(signature.split("\u0000"));
    const placed = layoutIslands(order.current.flatMap((id) => known.current.get(id) ?? []), snapshot.setupProjectId);
    return placed.filter((i) => present.has(i.projectId));
    // The layout depends on which projects exist, not on their live counts or names: a rename reaches the labels through `rows`.
  }, [signature, snapshot.setupProjectId]);
  /** Where the camera was, so rebuilding the scene for a new island does not snap the view back home. */
  const view = useRef<{ readonly position: THREE.Vector3; readonly target: THREE.Vector3 } | null>(null);
  /** When each sinking island started to sink, so a rebuild mid-sink does not restart it. */
  const sinkStart = useRef(new Map<string, number>());
  const rows = islands.flatMap((spec) => {
    const entry = entries.find((e) => e.key === spec.projectId);
    return entry === undefined ? [] : [{ spec: { ...spec, name: entry.item.project.name }, floor: entry.item, phase: entry.phase }];
  });
  const seats = useRef(new Map<string, ReadonlyMap<string, number>>());
  const people = placePeople(rows, seats.current);
  // Read by the render loop, which must not restart (and rebuild the scene) on every snapshot push.
  const dimmed = new Set(rows.filter((r) => r.floor.dimmed).map((r) => r.spec.projectId));
  const live = useRef({ people, dimmed });
  live.current = { people, dimmed };
  const takeNewMessages = useNewMessages(snapshot.messages);

  // Choosing an island opens its panel: take focus there, so its agents and Open island come next in Tab order.
  useEffect(() => {
    if (selected === null) return;
    panelHeading.current?.focus();
    // The panel is in the page now, so the camera can leave room for it.
    focus.current(selected);
  }, [selected]);

  // Without WebGL there is no render loop to finish a sink: let a removed island go at once.
  useEffect(() => {
    if (!noWebGl) return;
    for (const e of entries) if (e.phase === "leaving") onGone(e.key);
  }, [noWebGl, entries, onGone]);

  useEffect(() => {
    const el = host.current;
    if (el === null) return;
    let renderer: THREE.WebGLRenderer;
    try {
      renderer = new THREE.WebGLRenderer({ antialias: true });
    } catch {
      setNoWebGl(true);
      return;
    }
    renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
    renderer.setClearColor(0x8fd3f4);
    el.prepend(renderer.domElement);
    const scene = new THREE.Scene();
    scene.add(new THREE.HemisphereLight(0xffffff, 0x4a90b8, 1.1));
    const sun = new THREE.DirectionalLight(0xfff3d6, 2.2);
    sun.position.set(40, 70, 30);
    scene.add(sun);

    const ocean = buildOcean();
    scene.add(ocean.mesh);
    const group = new THREE.Group();
    const groups = new Map<string, THREE.Object3D>();
    // born: when this island starts to rise (stagger by build order); lift: current hover lift.
    const anim = new Map<string, { born: number | null; order: number; lift: number; gone: boolean; off: number; op: number }>();
    const islandMats = new Map<string, THREE.Material[]>();
    let order = 0;
    for (const spec of islands) {
      const g = buildIsland(spec);
      group.add(g);
      groups.set(spec.projectId, g);
      islandMats.set(spec.projectId, ownMaterials(g));
      // Marked risen by the first frame that animates it, so a mount StrictMode discards does not use up the rise.
      const fresh = !risen.current.has(spec.projectId);
      anim.set(spec.projectId, { born: fresh ? null : -1e9, order: fresh ? order++ : 0, lift: 0, gone: false, off: 0, op: 1 });
    }
    const leave = (id: string): void => {
      risen.current.delete(id);
      sinkStart.current.delete(id);
      onGoneRef.current(id);
    };
    scene.add(group);

    // One hut per person, subagents included, made when they appear and removed when they go.
    const huts = new Map<string, { readonly group: THREE.Group; readonly worktree: boolean; readonly mats: THREE.Material[] }>();
    // What can hide a person: the islands and every hut. Rebuilt only when a hut comes or goes.
    let blockers: THREE.Object3D[] = [group];
    let blockersStale = false;
    const dropHut = (key: string, hut: { readonly group: THREE.Group; readonly mats: THREE.Material[] }): void => {
      scene.remove(hut.group);
      release(hut.group, hut.mats);
      huts.delete(key);
      blockersStale = true;
    };
    const occluder = new Occluder();
    /** A name tag over each agent's head, and its twin on a boat or parrot; page elements, so text stays flat and upright. */
    const tags = new Map<string, HTMLElement>();
    const makeTag = (className: string): HTMLElement => {
      const t = document.createElement("div");
      t.className = className;
      el.appendChild(t);
      return t;
    };

    const bounds = worldBounds(islands, SEA_PAD);
    const cx = (bounds.minX + bounds.maxX) / 2;
    const cz = (bounds.minZ + bounds.maxZ) / 2;
    const spread = Math.max(bounds.maxX - bounds.minX, bounds.maxZ - bounds.minZ);
    const camera = new THREE.PerspectiveCamera(42, 1, 1, 1000);
    camera.position.set(cx, spread * 0.8, cz + spread * 0.8);
    const controls = new MapControls(camera, renderer.domElement);
    controls.target.set(cx, 0, cz);
    controls.enableDamping = true;
    // Tilt from straight down to just above the waterline, as in Google Maps. Shift- or Cmd-drag
    // turns and tilts (OrbitControls treats a modified left-drag as rotate); right-drag does too.
    controls.minPolarAngle = 0;
    controls.maxPolarAngle = Math.PI * 0.47;
    controls.minDistance = 14;
    controls.addEventListener("start", () => {
      homed.current = false;
    });
    // Fog, draw distance and sea all follow the fitted distance (the zoom-out cap): the farthest island is about
    // 1.6 times as far as the centre, so fog starts past that.
    const setReach = (reach: number): void => {
      controls.maxDistance = reach;
      scene.fog = new THREE.Fog(0x8fd3f4, reach * 1.8, reach * 3.2);
      camera.far = reach * 4;
      camera.updateProjectionMatrix();
      ocean.mesh.scale.setScalar(Math.max(1, (camera.far * 2) / OCEAN_SIZE));
    };
    setReach(camera.position.distanceTo(controls.target));
    if (view.current !== null) {
      camera.position.copy(view.current.position);
      controls.target.copy(view.current.target);
    } else {
      // Filled now, not by the first frame: StrictMode unmounts before any frame and the remount reads this.
      view.current = { position: camera.position.clone(), target: controls.target.clone() };
    }
    const saved = view.current;

    // One camera move per action, eased out over FLY_MS (DESIGN.md, Motion: navigation).
    let goal: { target: THREE.Vector3; cam: THREE.Vector3; fromTarget: THREE.Vector3; fromCam: THREE.Vector3; t0: number | null } | null = null;
    const moveTo = (target: THREE.Vector3, cam: THREE.Vector3): void => {
      goal = { target, cam, fromTarget: controls.target.clone(), fromCam: camera.position.clone(), t0: null };
    };
    // Frame `specs` in the room the floating chrome leaves: the smallest distance (at least `want`) from which they
    // fit, with the centre of the view moved so they sit in the middle of that room. The sums are done on a spare
    // camera, so the live one never jumps.
    const probe = camera.clone();
    const labelSize = new Map<string, { readonly w: number; readonly h: number }>();
    const spots = (specs: readonly IslandSpec[]): { at: THREE.Vector3; halfW: number; up: number }[] =>
      specs.flatMap((spec) => {
        // A label that has not risen yet is display: none and measures 0, so keep the last size it had.
        const label = labels.current.get(spec.projectId);
        if (label !== undefined && label.offsetWidth > 0) labelSize.set(spec.projectId, { w: label.offsetWidth, h: label.offsetHeight });
        const size = labelSize.get(spec.projectId) ?? { w: 0, h: 0 };
        const ring = Array.from({ length: 8 }, (_, i) => {
          const a = (i / 8) * Math.PI * 2;
          return { at: new THREE.Vector3(spec.x + Math.cos(a) * spec.radius * COAST_REACH, 0, spec.z + Math.sin(a) * spec.radius * COAST_REACH), halfW: 0, up: 0 };
        });
        return [...ring, { at: new THREE.Vector3(spec.x, LABEL_Y, spec.z), halfW: size.w / 2, up: size.h }];
      });
    const frame = (specs: readonly IslandSpec[], covers: readonly Rect[], dir: THREE.Vector3, from: THREE.Vector3, lo: number, hi: number, want: number, ref: number): { target: THREE.Vector3; cam: THREE.Vector3; d: number } => {
      const w = el.clientWidth;
      const h = el.clientHeight;
      const pts = spots(specs);
      probe.aspect = camera.aspect;
      probe.updateProjectionMatrix();
      const centre = from.clone();
      const seen = (d: number): Rect => {
        probe.position.copy(centre).addScaledVector(dir, d);
        probe.lookAt(centre);
        probe.updateMatrixWorld();
        const box = { left: Infinity, top: Infinity, right: -Infinity, bottom: -Infinity };
        for (const p of pts) {
          const n = p.at.clone().project(probe);
          const x = ((n.x + 1) / 2) * w;
          const y = ((1 - n.y) / 2) * h;
          box.left = Math.min(box.left, x - p.halfW);
          box.right = Math.max(box.right, x + p.halfW);
          box.top = Math.min(box.top, y - p.up);
          box.bottom = Math.max(box.bottom, y);
        }
        return box;
      };
      const first = seen(ref);
      const room = roomFor({ left: 0, top: 0, right: w, bottom: h }, covers, FRAME_GAP, first.right - first.left, first.bottom - first.top) ?? { left: 0, top: 0, right: w, bottom: h };
      // At distance d, slide the view until the islands' box is centred in the room, then report where the box ended up.
      // Sliding changes the perspective a little, so the distance is searched on the slid view, not the unslid one.
      const ground = new THREE.Raycaster();
      const settle = (d: number): Rect => {
        centre.copy(from);
        for (let pass = 0; pass < 3; pass++) {
          const box = seen(d);
          const dx = (room.left + room.right) / 2 - (box.left + box.right) / 2;
          const dy = (room.top + room.bottom) / 2 - (box.top + box.bottom) / 2;
          ground.setFromCamera(new THREE.Vector2(((w / 2 - dx) / w) * 2 - 1, -(((h / 2 - dy) / h) * 2 - 1)), probe);
          centre.copy(ground.ray.origin).addScaledVector(ground.ray.direction, -ground.ray.origin.y / ground.ray.direction.y);
          centre.y = 0;
        }
        return seen(d);
      };
      let near = lo;
      let far = hi;
      for (let i = 0; i < 24; i++) {
        const mid = (near + far) / 2;
        const box = settle(mid);
        if (box.right - box.left <= room.right - room.left && box.bottom - box.top <= room.bottom - room.top) far = mid;
        else near = mid;
      }
      const d = Math.max(far, want);
      settle(d);
      return { target: centre.clone(), cam: centre.clone().addScaledVector(dir, d), d };
    };
    /** Rects of the floating chrome in the view's own pixels, for what is on the page now. */
    const covering = (selectors: readonly string[]): Rect[] => {
      const host = el.getBoundingClientRect();
      return selectors.flatMap((selector) => {
        const side = el.parentElement?.querySelector<HTMLElement>(selector);
        if (side === null || side === undefined) return [];
        const r = side.getBoundingClientRect();
        return [{ left: r.left - host.left, top: r.top - host.top, right: r.right - host.left, bottom: r.bottom - host.top }];
      });
    };
    // The panel is not on the page yet when the click lands; the effect on `selected` flies again once it is.
    const flyToIsland = (spec: IslandSpec): void => {
      homed.current = false;
      const dir = camera.position.clone().sub(controls.target).normalize();
      const f = frame([spec], covering([".arch-list", ".usage-signs", ".arch-panel"]), dir, new THREE.Vector3(spec.x, 0, spec.z), controls.minDistance, controls.maxDistance, FOCUS_DISTANCE, FOCUS_DISTANCE);
      moveTo(f.target, f.cam);
    };
    flyTo.current = (spec) => {
      if (selectedRef.current === spec.projectId) flyToIsland(spec);
      else setSelected(spec.projectId);
    };
    focus.current = (projectId) => {
      const spec = islands.find((i) => i.projectId === projectId);
      if (spec !== undefined) flyToIsland(spec);
    };
    // Compass: north up and the opening tilt again, keeping the current zoom and place.
    resetView.current = () => {
      const target = controls.target.clone();
      moveTo(target, target.clone().addScaledVector(OPENING, camera.position.distanceTo(controls.target)));
    };

    const ray = new THREE.Raycaster();
    const ndc = new THREE.Vector2();
    const down = { x: 0, y: 0 };
    const pointer = { x: 0, y: 0, inside: false };
    const toNdc = (clientX: number, clientY: number): THREE.Vector2 => {
      const r = renderer.domElement.getBoundingClientRect();
      return ndc.set(((clientX - r.left) / r.width) * 2 - 1, -((clientY - r.top) / r.height) * 2 + 1);
    };
    const islandAt = (clientX: number, clientY: number): IslandSpec | undefined => {
      ray.setFromCamera(toNdc(clientX, clientY), camera);
      const hit = ray.intersectObjects(group.children, true)[0];
      let o: THREE.Object3D | null = hit?.object ?? null;
      while (o !== null && o.userData["projectId"] === undefined) o = o.parent;
      return islands.find((i) => i.projectId === o?.userData["projectId"]);
    };
    const onDown = (e: PointerEvent): void => {
      down.x = e.clientX;
      down.y = e.clientY;
    };
    /** The person drawn under a point: figures are page elements over the canvas, so they are hit-tested by their boxes. */
    const personAt = (clientX: number, clientY: number): Placed | undefined => {
      const boxes: Box[] = [];
      for (const p of live.current.people) {
        const pe = personEls.current.get(p.key);
        const figure = pe?.firstElementChild;
        if (pe === undefined || figure === null || figure === undefined || pe.style.display === "none") continue;
        const r = figure.getBoundingClientRect();
        boxes.push({ key: p.key, left: r.left, top: r.top, right: r.right, bottom: r.bottom, z: Number(pe.style.zIndex) || 0 });
      }
      const key = boxAt(boxes, clientX, clientY);
      return live.current.people.find((p) => p.key === key);
    };
    const onUp = (e: PointerEvent): void => {
      if (Math.hypot(e.clientX - down.x, e.clientY - down.y) > 4) return;
      const person = personAt(e.clientX, e.clientY);
      if (person !== undefined) {
        openRef.current(person.owner);
        return;
      }
      const spec = islandAt(e.clientX, e.clientY);
      if (spec !== undefined) flyTo.current(spec);
    };
    const onMove = (e: PointerEvent): void => {
      pointer.x = e.clientX;
      pointer.y = e.clientY;
      pointer.inside = true;
    };
    const onLeave = (): void => {
      pointer.inside = false;
    };
    renderer.domElement.addEventListener("pointerdown", onDown);
    renderer.domElement.addEventListener("pointerup", onUp);
    renderer.domElement.addEventListener("pointermove", onMove);
    renderer.domElement.addEventListener("pointerleave", onLeave);

    // The list and the usage signs float over the sea: frame every island in the open room they leave, and let the
    // camera back off no further than that.
    const resize = (): void => {
      const w = el.clientWidth;
      const h = el.clientHeight;
      renderer.setSize(w, h);
      camera.aspect = w / Math.max(h, 1);
      camera.updateProjectionMatrix();
      const home = frame(islands, covering([".arch-list", ".usage-signs"]), OPENING, new THREE.Vector3(cx, 0, cz), controls.minDistance, spread * 3, 0, spread * 1.13);
      setReach(home.d);
      if (!homed.current) return;
      camera.position.copy(home.cam);
      controls.target.copy(home.target);
      saved.position.copy(home.cam);
      saved.target.copy(home.target);
    };
    resize();
    const ro = new ResizeObserver(resize);
    ro.observe(el);
    // The signs appear once their numbers load, and the list and signs change size: re-frame when they do.
    const watch = (): void => {
      for (const selector of [".arch-list", ".usage-signs"]) {
        const side = el.parentElement?.querySelector<HTMLElement>(selector);
        if (side !== null && side !== undefined) ro.observe(side);
      }
    };
    watch();
    const mo = new MutationObserver(() => {
      watch();
      resize();
    });
    if (el.parentElement !== null) mo.observe(el.parentElement, { childList: true });

    // A message between two agents on one island is a parrot flying from sender to receiver; between islands it is a
    // boat sailing from one shore to the other.
    const notes: { readonly el: HTMLElement; readonly tag: HTMLElement; readonly wing: SVGElement | null; readonly from: THREE.Vector3; readonly to: THREE.Vector3; readonly t0: number }[] = [];
    const voyages: { readonly boat: THREE.Group; readonly tag: HTMLElement; readonly route: Route; readonly t0: number; readonly ms: number }[] = [];
    /** Where each island's label stands on screen this frame, so a boat passing under it can be seen through it. */
    const labelAt = new Map<string, { px: number; py: number }>();
    /** Screen boxes of the name and trip tags shown this frame. */
    const tagsAt: { x0: number; x1: number; y0: number; y1: number }[] = [];
    /** A tag centred on x whose top edge is at y. */
    const tagBelow = (tag: HTMLElement, x: number, y: number): void => {
      tagsAt.push({ x0: x - tag.offsetWidth / 2, x1: x + tag.offsetWidth / 2, y0: y, y1: y + tag.offsetHeight });
    };

    const v = new THREE.Vector3();
    const head = new THREE.Vector3();
    /** Screen position of a world point, and how many css px one world unit is tall there. */
    const screen = (p: THREE.Vector3): { px: number; py: number; unit: number; behind: boolean } => {
      const w = el.clientWidth;
      const h = el.clientHeight;
      v.copy(p).project(camera);
      head.set(p.x, p.y + 1, p.z).project(camera);
      return { px: ((v.x + 1) / 2) * w, py: ((1 - v.y) / 2) * h, unit: (Math.abs(v.y - head.y) / 2) * h, behind: v.z > 1 };
    };

    let raf = 0;
    const loop = (t: number): void => {
      raf = requestAnimationFrame(loop);
      tagsAt.length = 0;
      const moving = animateRef.current;
      if (moving) ocean.tick(t / 1000);
      ocean.mesh.position.set(controls.target.x, 0.4, controls.target.z);
      if (goal !== null) {
        goal.t0 ??= t;
        const k = moving ? clamp01((t - goal.t0) / FLY_MS) : 1;
        const e = 1 - (1 - k) ** 3;
        controls.target.lerpVectors(goal.fromTarget, goal.target, e);
        camera.position.lerpVectors(goal.fromCam, goal.cam, e);
        if (k >= 1) goal = null;
      }
      controls.update();
      const c = clampToBounds(controls.target.x, controls.target.z, bounds);
      const dx = c.x - controls.target.x;
      const dz = c.z - controls.target.z;
      if (dx !== 0 || dz !== 0) {
        controls.target.x += dx;
        controls.target.z += dz;
        camera.position.x += dx;
        camera.position.z += dz;
      }
      saved.position.copy(camera.position);
      saved.target.copy(controls.target);
      if (needle.current !== null) needle.current.style.transform = `rotate(${controls.getAzimuthalAngle()}rad)`;

      // Hover: the island under the pointer (or its list row) lifts a little.
      if (pointer.inside) hover.current = islandAt(pointer.x, pointer.y)?.projectId ?? null;
      renderer.domElement.style.cursor = pointer.inside && (hover.current !== null || personAt(pointer.x, pointer.y) !== undefined) ? "pointer" : "";

      // Rise on open, lift on hover, sink on removal.
      for (const spec of islands) {
        const a = anim.get(spec.projectId);
        const g = groups.get(spec.projectId);
        if (a === undefined || g === undefined) continue;
        if (a.born === null) {
          a.born = t + a.order * RISE_STAGGER_MS;
          risen.current.add(spec.projectId);
        }
        let off = 0;
        let op = 1;
        const leaving = phases.current.get(spec.projectId) === "leaving";
        if (moving) {
          const k = clamp01((t - a.born) / RISE_MS);
          off = -RISE_DEPTH * (1 - k) ** 3;
          op = clamp01(k * 1.6 - 0.4);
          a.lift += ((hover.current === spec.projectId && !leaving ? HOVER_LIFT : 0) - a.lift) * 0.18;
          if (leaving) {
            const started = sinkStart.current.get(spec.projectId) ?? t;
            sinkStart.current.set(spec.projectId, started);
            const s = clamp01((t - started) / SINK_MS);
            off -= SINK_DEPTH * s * s;
            op *= 1 - s;
            if (s >= 1 && !a.gone) {
              a.gone = true;
              leave(spec.projectId);
            }
          }
        } else if (leaving && !a.gone) {
          a.gone = true;
          leave(spec.projectId);
        }
        a.off = off + a.lift;
        a.op = op;
        // Search dims a non-matching island as the office dims a storey; the fade of a rise or sink multiplies in.
        const shade = op * (live.current.dimmed.has(spec.projectId) ? DIM : 1);
        g.position.y = a.off;
        g.visible = op > 0.02;
        setOpacity(islandMats.get(spec.projectId) ?? [], shade);
        const b = labels.current.get(spec.projectId);
        if (b !== undefined) {
          const s = screen(new THREE.Vector3(spec.x, 26 + a.off, spec.z));
          b.style.transform = `translate(-50%, -100%) translate(${s.px}px, ${s.py}px)`;
          b.style.display = s.behind || op <= 0.02 ? "none" : "flex";
          b.style.opacity = String(shade);
          labelAt.set(spec.projectId, { px: s.px, py: s.py });
        }
      }
      const byKey = new Map<string, Placed>();
      const subjects: Subject[] = [];
      for (const p of live.current.people) {
        byKey.set(p.key, p);
        const a = anim.get(p.island);
        const worktree = p.session.worktree;
        let hut = huts.get(p.key);
        if (hut !== undefined && hut.worktree !== worktree) {
          dropHut(p.key, hut);
          hut = undefined;
        }
        // An agent's own hut and its subagents' small huts (keys `<session>/<helper>`) stand round it: they never hide it.
        const family = p.key.split("/")[0] ?? p.key;
        if (hut === undefined) {
          const group = buildStall(worktree, hashString(p.key));
          if (p.helper) group.scale.setScalar(HELPER_SCALE);
          group.userData["family"] = family;
          scene.add(group);
          hut = { group, worktree, mats: ownMaterials(group) };
          huts.set(p.key, hut);
          blockersStale = true;
        }
        const op = a?.op ?? 1;
        const shade = op * (p.dimmed ? DIM : 1);
        hut.group.position.set(p.x, GRASS_Y + (a?.off ?? 0), p.z);
        hut.group.visible = op > 0.02;
        setOpacity(hut.mats, shade);
        subjects.push({ key: p.key, family, chest: new THREE.Vector3(p.x, p.y + (a?.off ?? 0) + PERSON_UNITS * (p.helper ? HELPER_SCALE : 1) * 0.5, p.z) });
      }
      for (const [key, hut] of huts) if (!byKey.has(key)) dropHut(key, hut);
      for (const [key, tag] of tags)
        if (!byKey.has(key)) {
          tag.remove();
          tags.delete(key);
        }
      if (blockersStale) {
        blockers = [group, ...[...huts.values()].map((h) => h.group)];
        blockersStale = false;
      }
      // People are page elements painted over the scene, so one behind another hut (or the hill) would show
      // through it: hide a person whose chest the camera cannot see past other huts and the islands.
      const hiddenBy = occluder.update(camera.position, subjects, blockers);
      for (const p of live.current.people) {
        const pe = personEls.current.get(p.key);
        if (pe === undefined) continue;
        const a = anim.get(p.island);
        const op = a?.op ?? 1;
        const shade = op * (p.dimmed ? DIM : 1);
        const s = screen(new THREE.Vector3(p.x, p.y + (a?.off ?? 0), p.z));
        const scale = ((s.unit * PERSON_UNITS) / PERSON_PX) * (p.helper ? HELPER_SCALE : 1);
        const shown = !(s.behind || hiddenBy.get(p.key) === true || scale < MIN_SCALE || op <= 0.02);
        pe.style.display = shown ? "block" : "none";
        pe.style.transform = `translate(${s.px}px, ${s.py}px) scale(${scale})`;
        pe.style.zIndex = String(Math.round(s.py));
        pe.style.opacity = String(shade);
        // Who is who, once the island is close enough to read; the overview stays clean. Subagents are not named.
        if (p.helper) continue;
        let tag = tags.get(p.key);
        if (tag === undefined) {
          tag = makeTag("arch-tag");
          tags.set(p.key, tag);
        }
        const name = shortName(p.session.title);
        if (tag.textContent !== name) tag.textContent = name;
        tag.style.display = shown && scale >= TAG_SCALE && s.px >= 0 && s.px <= el.clientWidth && s.py >= 0 && s.py <= el.clientHeight ? "block" : "none";
        if (tag.style.display === "block") {
          const bottom = s.py - PERSON_PX * scale;
          tagsAt.push({ x0: s.px - tag.offsetWidth / 2, x1: s.px + tag.offsetWidth / 2, y0: bottom - tag.offsetHeight, y1: bottom });
        }
        tag.style.transform = `translate(${s.px}px, ${s.py - PERSON_PX * scale}px) translate(-50%, -100%)`;
        tag.style.zIndex = pe.style.zIndex;
        tag.style.opacity = String(shade);
      }

      for (const m of takeNewMessages()) {
        const a = byKey.get(m.fromSessionId);
        const b = byKey.get(m.toSessionId);
        if (a === undefined || b === undefined || !moving) continue;
        if (a.island !== b.island) {
          const from = islands.find((i) => i.projectId === a.island);
          const to = islands.find((i) => i.projectId === b.island);
          if (from === undefined || to === undefined) continue;
          const route = sailRoute(from, to, islands);
          const boat = buildBoat();
          boat.scale.setScalar(BOAT_SCALE);
          scene.add(boat);
          const tag = makeTag("arch-trip");
          tag.textContent = tripTag(a.owner, b.owner);
          voyages.push({ boat, tag, route, t0: t, ms: Math.max(BOAT_MIN_MS, (Math.hypot(route.to.x - route.from.x, route.to.z - route.from.z) / BOAT_SPEED) * 1000) });
          continue;
        }
        const note = document.createElement("div");
        note.className = "arch-note";
        note.innerHTML = PARROT_SVG;
        el.appendChild(note);
        const tag = makeTag("arch-trip");
        tag.textContent = tripTag(a.owner, b.owner);
        notes.push({ el: note, tag, wing: note.querySelector<SVGElement>(".wing"), from: new THREE.Vector3(a.x, a.y + PERSON_UNITS, a.z), to: new THREE.Vector3(b.x, b.y + PERSON_UNITS, b.z), t0: t });
      }
      for (let i = notes.length - 1; i >= 0; i--) {
        const n = notes[i];
        if (n === undefined) continue;
        const k = (t - n.t0) / NOTE_MS;
        if (k >= 1) {
          n.el.remove();
          n.tag.remove();
          notes.splice(i, 1);
          continue;
        }
        const at = n.from.clone().lerp(n.to, k);
        at.y += Math.sin(Math.PI * k) * (4 + n.from.distanceTo(n.to) * 0.25);
        const s = screen(at);
        n.el.style.display = s.behind ? "none" : "block";
        n.tag.style.display = s.behind ? "none" : "block";
        // The wing beats from here, not a CSS keyframe: archipelago.css.test.ts allows only rules scoped to .arch.
        n.wing?.style.setProperty("transform", `scaleY(${Math.sin(t / 55) * 0.8})`);
        // The parrot faces the way it is flying on screen.
        const goingLeft = screen(n.to).px < screen(n.from).px;
        const size = Math.max(0.8, s.unit / 11);
        // The parrot is 24 px tall and centred on the point; the tag hangs just below its belly.
        n.tag.style.transform = `translate(${s.px}px, ${s.py}px) translate(-50%, ${12 * size + 3}px)`;
        if (!s.behind) tagBelow(n.tag, s.px, s.py + 12 * size + 3);
        n.el.style.transform = `translate(${s.px}px, ${s.py}px) scale(${goingLeft ? -size : size}, ${size}) rotate(${Math.sin(k * 12) * 6}deg)`;
      }

      // Boats: along the course, heading with it, bobbing on the swell; one that has arrived is taken out of the sea.
      const boatsAt: { px: number; py: number }[] = [];
      for (let i = voyages.length - 1; i >= 0; i--) {
        const vy = voyages[i];
        if (vy === undefined) continue;
        const k = clamp01((t - vy.t0) / vy.ms);
        if (k >= 1) {
          scene.remove(vy.boat);
          vy.tag.remove();
          voyages.splice(i, 1);
          continue;
        }
        const eased = k * k * (3 - 2 * k);
        const at = bezierAt(vy.route, eased);
        const ahead = bezierAt(vy.route, Math.min(1, eased + 0.02));
        vy.boat.position.set(at.x, BOAT_Y + Math.sin(t / 260 + i) * 0.25, at.z);
        vy.boat.rotation.y = -Math.atan2(ahead.z - at.z, ahead.x - at.x);
        vy.boat.rotation.z = Math.sin(t / 340 + i) * 0.06;
        const bs = screen(vy.boat.position);
        vy.tag.style.display = bs.behind ? "none" : "block";
        vy.tag.style.transform = `translate(${bs.px}px, ${bs.py}px) translate(-50%, 8px)`;
        if (!bs.behind) tagBelow(vy.tag, bs.px, bs.py + 8);
        boatsAt.push(bs);
      }
      // Labels are page elements over the canvas, so a boat sailing behind one would vanish, and a name tag would
      // collide with it: let them show through.
      if (boatsAt.length > 0 || tagsAt.length > 0)
        for (const [id, b] of labels.current) {
          const at = labelAt.get(id);
          if (at === undefined || b.style.display === "none") continue;
          const w = b.offsetWidth;
          const h = b.offsetHeight;
          const x0 = at.px - w / 2;
          const x1 = at.px + w / 2;
          const y0 = at.py - h;
          const boat = boatsAt.some((q) => q.px > x0 && q.px < x1 && q.py > y0 && q.py < at.py);
          const tag = tagsAt.some((r) => r.x0 < x1 && x0 < r.x1 && r.y0 < at.py && y0 < r.y1);
          if (boat || tag) b.style.opacity = String(Math.min(Number(b.style.opacity), SEE_THROUGH));
        }
      renderer.render(scene, camera);
    };
    raf = requestAnimationFrame(loop);
    return () => {
      cancelAnimationFrame(raf);
      ro.disconnect();
      mo.disconnect();
      controls.dispose();
      for (const n of notes) {
        n.el.remove();
        n.tag.remove();
      }
      for (const vy of voyages) {
        scene.remove(vy.boat);
        vy.tag.remove();
      }
      for (const tag of tags.values()) tag.remove();
      renderer.domElement.removeEventListener("pointerdown", onDown);
      renderer.domElement.removeEventListener("pointerup", onUp);
      renderer.domElement.removeEventListener("pointermove", onMove);
      renderer.domElement.removeEventListener("pointerleave", onLeave);
      for (const [key, hut] of [...huts]) dropHut(key, hut);
      for (const [id, g] of groups) release(g, islandMats.get(id) ?? []);
      ocean.mesh.geometry.dispose();
      ocean.mesh.material.dispose();
      renderer.dispose();
      // Browsers keep only a handful of live WebGL contexts; give this one back now rather than on garbage collection.
      renderer.forceContextLoss();
      renderer.domElement.remove();
    };
  }, [islands]);

  const picked = rows.find((r) => r.spec.projectId === selected);
  // The Setup island is not a place to work, and a sinking one is going: neither takes an agent.
  const moveTargets = rows.filter((r) => r.spec.projectId !== picked?.spec.projectId && !r.floor.plant && r.phase !== "leaving");
  return (
    <>
      <div className="arch" ref={host} data-testid="archipelago">
        {noWebGl ? (
          <p className="scene-note" role="status">
            The island map needs WebGL, which this window does not have. Switch to Office to carry on.
          </p>
        ) : null}
        {noWebGl ? null : (
          <>
            <button type="button" className="arch-compass" aria-label="North up and reset tilt" title="North up" onClick={() => resetView.current()}>
              <span ref={needle} className="arch-needle" />
            </button>
            <p className="arch-hint">Drag to move · Shift-drag to turn and tilt · Scroll or pinch to zoom</p>
          </>
        )}
        {(noWebGl ? [] : people).map((p) => (
          <div
            key={p.key}
            className="arch-person"
            ref={(pe) => {
              if (pe === null) personEls.current.delete(p.key);
              else personEls.current.set(p.key, pe);
            }}
          >
            <Person look={lookFor(p.session.id)} pose={poseForState(p.session.state)} />
          </div>
        ))}
        {(noWebGl ? [] : rows).map(({ spec, floor }) => (
          <button
            key={spec.projectId}
            type="button"
            className={floor.dimmed ? "arch-label is-dim" : "arch-label"}
            data-selected={spec.projectId === selected ? "" : undefined}
            ref={(b) => {
              if (b === null) labels.current.delete(spec.projectId);
              else labels.current.set(spec.projectId, b);
            }}
            onClick={() => flyTo.current(spec)}
          >
            <i style={{ background: LIGHT[floor.light] }} />
            <b>{spec.name}</b>
            <span>
              {floor.counts.active}/{floor.counts.total}
              {floor.sessions.length > MAX_SLOTS ? ` · +${floor.sessions.length - MAX_SLOTS} more` : ""}
            </span>
          </button>
        ))}
        {picked === undefined ? null : (
          <PlacePanel
            id={PLACE_PANEL_ID}
            className="arch-panel"
            projectId={picked.spec.projectId}
            name={picked.spec.name}
            noun={PLACE.islands.noun}
            enterLabel="Open island"
            sessions={picked.floor.sessions}
            counts={picked.floor.counts}
            asks={questionsForProject(snapshot, picked.spec.projectId)}
            moveTargets={moveTargets.map((r) => ({ id: r.spec.projectId, name: r.spec.name }))}
            leaving={picked.phase === "leaving"}
            headingRef={panelHeading}
            onRename={onRenameIsland}
            onMove={onMoveSession}
            onOpenSession={onOpenSession}
            onEnter={onEnter}
            onRemove={picked.floor.plant ? undefined : () => onDemolish(picked.spec.projectId)}
            onClose={() => setSelected(null)}
          />
        )}
      </div>
      <nav className="arch-list" aria-label="Islands">
        {rows.map(({ spec, floor, phase }) => (
          <div
            key={spec.projectId}
            className={`arch-list-row light-${floor.light}${floor.dimmed ? " is-dim" : ""}`}
            data-selected={spec.projectId === selected ? "" : undefined}
            onMouseEnter={() => {
              hover.current = spec.projectId;
            }}
            onMouseLeave={() => {
              if (hover.current === spec.projectId) hover.current = null;
            }}
          >
            <PlaceNameplate
              mode="islands"
              projectId={spec.projectId}
              name={spec.name}
              light={floor.light}
              counts={floor.counts}
              pending={floor.pending}
              overflow={Math.max(0, floor.sessions.length - MAX_SLOTS)}
              plant={floor.plant}
              stamp={floor.stamp}
              leaving={phase === "leaving"}
              expanded={spec.projectId === selected}
              panelId={PLACE_PANEL_ID}
              onShow={() => {
                setSelected(spec.projectId);
                flyTo.current(spec);
              }}
              onRemove={() => onDemolish(spec.projectId)}
            />
          </div>
        ))}
        <button type="button" className="arch-add" data-testid="add-island" onClick={onAddFloor}>
          + Add an island
        </button>
      </nav>
    </>
  );
}
