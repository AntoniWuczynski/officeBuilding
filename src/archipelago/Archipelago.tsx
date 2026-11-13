// The island map: the building view's floors drawn as islands on one ocean. Same data, same actions.
import { useEffect, useMemo, useRef, useState } from "react";
import * as THREE from "three";
import { MapControls } from "three/examples/jsm/controls/MapControls.js";
import type { OfficeSnapshot, Project, Session } from "../types";
import { lightWords, poseForState } from "../lib/animation";
import { hashString, lookFor } from "../lib/looks";
import { Person } from "../scene/Person";
import { STATE_LABEL, TOOL_LABEL, helpersLine } from "../state/selectors";
import type { Present } from "../state/usePresence";
import { EditableSign } from "../components/building/EditableSign";
import type { FloorModel } from "../components/building/Storey";
import { SinkIcon } from "../components/icons";
import { PLACE } from "../state/placeWords";
import { COUNTER_H, OCEAN_SIZE, buildIsland, buildOcean, buildStall, ownMaterials, release, setOpacity } from "./build";
import { agentSlots, clampToBounds, layoutIslands, MAX_SLOTS, mergeOrder, worldBounds } from "./islands";
import type { IslandSpec } from "./islands";
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
/** Where a selected island's camera ends up, in world units from its centre. */
const FOCUS_DISTANCE = 52;
/** Open sea kept around the islands: the camera target cannot leave this box. */
const SEA_PAD = 40;
const NOTE_MS = 2600;
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
  /** A subagent: smaller, beside its parent. */
  readonly helper: boolean;
  readonly dimmed: boolean;
}

/** Real sessions on their seats, each session's subagents standing beside it. */
function placePeople(rows: readonly { readonly spec: IslandSpec; readonly floor: FloorModel }[]): Placed[] {
  const out: Placed[] = [];
  for (const { spec, floor } of rows) {
    const slots = agentSlots(spec);
    floor.sessions.slice(0, MAX_SLOTS).forEach((session, i) => {
      const slot = slots[i];
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
  readonly name: string;
  /** False under prefers-reduced-motion. */
  readonly animate: boolean;
  readonly onRename: (name: string) => void;
  /** Go inside an island: the floor view, with Hire, the terminal, decisions and the rest. */
  readonly onEnter: (projectId: string) => void;
  readonly onOpenSession: (session: Session) => void;
  readonly onAddFloor: () => void;
  readonly onDemolish: (projectId: string) => void;
}

const clamp01 = (n: number): number => Math.min(1, Math.max(0, n));

export function Archipelago({ snapshot, entries, onGone, name, animate, onRename, onEnter, onOpenSession, onAddFloor, onDemolish }: Props): React.ReactElement {
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
  const [noWebGl, setNoWebGl] = useState(false);
  // Reduced motion: still sea, instant camera moves, no flying notes (as the building's walks and collapses).
  const animateRef = useRef(animate);
  animateRef.current = animate;
  const onGoneRef = useRef(onGone);
  onGoneRef.current = onGone;
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
    // The layout depends on which projects exist, not on their live counts.
  }, [signature, snapshot.setupProjectId]);
  /** Where the camera was, so rebuilding the scene for a new island does not snap the view back home. */
  const view = useRef<{ readonly position: THREE.Vector3; readonly target: THREE.Vector3 } | null>(null);
  /** When each sinking island started to sink, so a rebuild mid-sink does not restart it. */
  const sinkStart = useRef(new Map<string, number>());
  const rows = islands.flatMap((spec) => {
    const entry = entries.find((e) => e.key === spec.projectId);
    return entry === undefined ? [] : [{ spec, floor: entry.item, phase: entry.phase }];
  });
  const people = placePeople(rows);
  // Read by the render loop, which must not restart (and rebuild the scene) on every snapshot push.
  const dimmed = new Set(rows.filter((r) => r.floor.dimmed).map((r) => r.spec.projectId));
  const live = useRef({ people, messages: snapshot.messages, dimmed });
  live.current = { people, messages: snapshot.messages, dimmed };

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
    const dropHut = (key: string, hut: { readonly group: THREE.Group; readonly mats: THREE.Material[] }): void => {
      scene.remove(hut.group);
      release(hut.group, hut.mats);
      huts.delete(key);
    };

    // Frame every island; the camera may not back off past that (the zoom-out cap).
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
    controls.maxDistance = camera.position.distanceTo(controls.target);
    // Fog, draw distance and sea all follow the fitted distance: from the opening view the farthest island is about
    // 1.6 times as far as the centre (half the spread across, 1.13 spread away), so fog starts past that.
    const reach = controls.maxDistance;
    scene.fog = new THREE.Fog(0x8fd3f4, reach * 1.8, reach * 3.2);
    camera.far = reach * 4;
    ocean.mesh.scale.setScalar(Math.max(1, (camera.far * 2) / OCEAN_SIZE));
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
    flyTo.current = (spec) => {
      const target = new THREE.Vector3(spec.x, 0, spec.z);
      const dir = camera.position.clone().sub(controls.target).normalize();
      moveTo(target, target.clone().addScaledVector(dir, FOCUS_DISTANCE));
      setSelected(spec.projectId);
    };
    // Compass: north up and the opening tilt again, keeping the current zoom and place.
    const opening = new THREE.Vector3(0, 1, 1).normalize();
    resetView.current = () => {
      const target = controls.target.clone();
      moveTo(target, target.clone().addScaledVector(opening, camera.position.distanceTo(controls.target)));
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
    const onUp = (e: PointerEvent): void => {
      if (Math.hypot(e.clientX - down.x, e.clientY - down.y) > 4) return;
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

    // The list and the usage signs float over the sea: shift the view so the islands sit in the open space between them.
    const resize = (): void => {
      const w = el.clientWidth;
      const h = el.clientHeight;
      renderer.setSize(w, h);
      camera.aspect = w / Math.max(h, 1);
      const inset = (selector: string): number => {
        const side = el.parentElement?.querySelector<HTMLElement>(selector);
        return side === null || side === undefined ? 0 : side.offsetWidth + 32;
      };
      const left = inset(".arch-list");
      const right = inset(".usage-signs");
      camera.setViewOffset(w, h, -(left - right) / 2, 0, w, h);
      camera.updateProjectionMatrix();
    };
    resize();
    const ro = new ResizeObserver(resize);
    ro.observe(el);

    // Notes: a message between two agents flies from sender to receiver, arcing over the sea.
    const seenMsgs = new Set(live.current.messages.map((m) => m.id));
    const notes: { readonly el: HTMLElement; readonly from: THREE.Vector3; readonly to: THREE.Vector3; readonly t0: number }[] = [];

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
      renderer.domElement.style.cursor = pointer.inside && hover.current !== null ? "pointer" : "";

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
        }
      }
      const byKey = new Map<string, Placed>();
      for (const p of live.current.people) {
        byKey.set(p.key, p);
        const a = anim.get(p.island);
        const worktree = p.session.worktree;
        let hut = huts.get(p.key);
        if (hut !== undefined && hut.worktree !== worktree) {
          dropHut(p.key, hut);
          hut = undefined;
        }
        if (hut === undefined) {
          const group = buildStall(worktree, hashString(p.key));
          if (p.helper) group.scale.setScalar(HELPER_SCALE);
          scene.add(group);
          hut = { group, worktree, mats: ownMaterials(group) };
          huts.set(p.key, hut);
        }
        const op = a?.op ?? 1;
        const shade = op * (p.dimmed ? DIM : 1);
        hut.group.position.set(p.x, GRASS_Y + (a?.off ?? 0), p.z);
        hut.group.visible = op > 0.02;
        setOpacity(hut.mats, shade);
        const pe = personEls.current.get(p.key);
        if (pe === undefined) continue;
        const s = screen(new THREE.Vector3(p.x, p.y + (a?.off ?? 0), p.z));
        const scale = ((s.unit * PERSON_UNITS) / PERSON_PX) * (p.helper ? HELPER_SCALE : 1);
        pe.style.display = s.behind || scale < MIN_SCALE || op <= 0.02 ? "none" : "block";
        pe.style.transform = `translate(${s.px}px, ${s.py}px) scale(${scale})`;
        pe.style.zIndex = String(Math.round(s.py));
        pe.style.opacity = String(shade);
      }
      for (const [key, hut] of huts) if (!byKey.has(key)) dropHut(key, hut);

      for (const m of live.current.messages) {
        if (seenMsgs.has(m.id)) continue;
        seenMsgs.add(m.id);
        const a = byKey.get(m.fromSessionId);
        const b = byKey.get(m.toSessionId);
        if (a === undefined || b === undefined || !moving) continue;
        const note = document.createElement("div");
        note.className = "arch-note";
        el.appendChild(note);
        notes.push({ el: note, from: new THREE.Vector3(a.x, a.y + PERSON_UNITS, a.z), to: new THREE.Vector3(b.x, b.y + PERSON_UNITS, b.z), t0: t });
      }
      for (let i = notes.length - 1; i >= 0; i--) {
        const n = notes[i];
        if (n === undefined) continue;
        const k = (t - n.t0) / NOTE_MS;
        if (k >= 1) {
          n.el.remove();
          notes.splice(i, 1);
          continue;
        }
        const at = n.from.clone().lerp(n.to, k);
        at.y += Math.sin(Math.PI * k) * (4 + n.from.distanceTo(n.to) * 0.25);
        const s = screen(at);
        n.el.style.display = s.behind ? "none" : "block";
        n.el.style.transform = `translate(${s.px}px, ${s.py}px) scale(${Math.max(0.5, s.unit / 11)}) rotate(${Math.sin(k * 12) * 14}deg)`;
      }
      renderer.render(scene, camera);
    };
    raf = requestAnimationFrame(loop);
    return () => {
      cancelAnimationFrame(raf);
      ro.disconnect();
      controls.dispose();
      for (const n of notes) n.el.remove();
      renderer.domElement.removeEventListener("pointerdown", onDown);
      renderer.domElement.removeEventListener("pointerup", onUp);
      renderer.domElement.removeEventListener("pointermove", onMove);
      renderer.domElement.removeEventListener("pointerleave", onLeave);
      for (const [key, hut] of [...huts]) dropHut(key, hut);
      for (const [id, g] of groups) release(g, islandMats.get(id) ?? []);
      (ocean.mesh.geometry as THREE.BufferGeometry).dispose();
      (ocean.mesh.material as THREE.Material).dispose();
      renderer.dispose();
      // Browsers keep only a handful of live WebGL contexts; give this one back now rather than on garbage collection.
      renderer.forceContextLoss();
      renderer.domElement.remove();
    };
  }, [islands]);

  const picked = rows.find((r) => r.spec.projectId === selected);
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
          <aside className="arch-panel" aria-label={`${picked.spec.name} island`}>
            <h2>{picked.spec.name}</h2>
            <p>
              {picked.floor.counts.total} {picked.floor.counts.total === 1 ? "agent" : "agents"}, {picked.floor.counts.active} working
              {picked.floor.pending > 0 ? `, ${picked.floor.pending} waiting on you` : ""}
            </p>
            <ul>
              {picked.floor.sessions.map((s) => (
                <li key={s.id}>
                  <button type="button" onClick={() => onOpenSession(s)}>
                    <b>{s.title}</b>
                    <span>
                      {STATE_LABEL[s.state]} · {TOOL_LABEL[s.tool]}
                      {helpersLine(s) === null ? "" : ` · ${helpersLine(s)}`}
                      {s.worktree ? " · in a worktree" : ""}
                    </span>
                  </button>
                </li>
              ))}
            </ul>
            <button type="button" className="btn btn-primary arch-open" disabled={picked.phase === "leaving"} onClick={() => onEnter(picked.spec.projectId)}>
              Open island
            </button>
          </aside>
        )}
      </div>
      <nav className="arch-list" aria-label="Islands">
        <EditableSign name={name} onRename={onRename} noun="map" />
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
            <button
              type="button"
              className="nameplate"
              data-testid="island-nameplate"
              data-project={spec.projectId}
              aria-label={`${spec.name}: ${floor.counts.active} of ${floor.counts.total} active, ${lightWords(floor.light)}${floor.pending > 0 ? `, ${floor.pending} waiting on you` : ""}. Show island.`}
              onClick={() => {
                setSelected(spec.projectId);
                flyTo.current(spec);
              }}
            >
              <i className="lamp" aria-hidden />
              <span className="nameplate-name">{spec.name}</span>
              {floor.plant ? <span className="nameplate-kind">Plant room</span> : null}
              <span className="nameplate-count">
                {floor.counts.active}/{floor.counts.total}
              </span>
              {floor.pending > 0 ? (
                <span className="arch-pending" aria-label={`${floor.pending} waiting on you`}>
                  {floor.pending}
                </span>
              ) : null}
            </button>
            {floor.plant ? null : (
              <button
                type="button"
                className="demolish-btn"
                aria-label={PLACE.islands.removeLabel(spec.name)}
                title="Sink this island"
                disabled={phase === "leaving"}
                onClick={() => onDemolish(spec.projectId)}
              >
                <SinkIcon />
              </button>
            )}
          </div>
        ))}
        <button type="button" className="arch-add" data-testid="add-island" onClick={onAddFloor}>
          + Add an island
        </button>
      </nav>
    </>
  );
}
