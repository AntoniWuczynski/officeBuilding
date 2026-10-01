import type { FloorLight, SessionState } from "../types";

/** The pose an employee holds at their desk for a given session state. */
export type Pose =
  | "typing" //    running
  | "thinking" //  thinking
  | "hand-up" //   waiting on a human
  | "stuck" //     error
  | "idle" //      idle
  | "finished"; // done

const POSES: Record<SessionState, Pose> = {
  running: "typing",
  thinking: "thinking",
  "waiting-human": "hand-up",
  error: "stuck",
  idle: "idle",
  done: "finished",
};

export function poseForState(state: SessionState): Pose {
  return POSES[state];
}

/** Whether a state should draw the eye (needs attention). */
export function needsAttention(state: SessionState): boolean {
  return state === "waiting-human" || state === "error";
}

const LIGHT_VAR: Record<FloorLight, string> = {
  green: "var(--ok)",
  yellow: "var(--wait)",
  red: "var(--fault)",
  off: "var(--off)",
};

/** CSS colour (a DESIGN.md status token) for a floor light. */
export function lightColour(light: FloorLight): string {
  return LIGHT_VAR[light];
}

const LIGHT_WORDS: Record<FloorLight, string> = {
  red: "has a stuck agent",
  yellow: "needs you",
  green: "agents working",
  off: "nobody working",
};

/** Plain-words status for screen readers and tooltips. */
export function lightWords(light: FloorLight): string {
  return LIGHT_WORDS[light];
}
