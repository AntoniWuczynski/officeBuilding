import type { Settings } from "../types";

/** The settings of a fresh install (`Settings::default` in settings.rs). */
export function makeSettings(): Settings {
  return {
    notifications: {
      finished: true,
      needsYou: true,
      stuck: true,
      sound: true,
      dockBounce: true,
      systemNotification: true,
      cooldownSecs: 10,
    },
    editor: { preferred: null },
    connectors: { linearAgentLabel: "agent" },
  };
}
