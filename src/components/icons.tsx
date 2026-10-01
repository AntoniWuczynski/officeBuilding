// Line icons drawn for this app: each is the room object its panel lives in.
import type { Section } from "./floor/sections";

const common = {
  width: 22,
  height: 22,
  viewBox: "0 0 22 22",
  fill: "none",
  stroke: "currentColor",
  strokeWidth: 1.6,
  strokeLinejoin: "round" as const,
  strokeLinecap: "round" as const,
  "aria-hidden": true,
};

export function SectionIcon({ section }: { readonly section: Section }): React.ReactElement {
  switch (section) {
    case "info": // brass plaque
      return (
        <svg {...common}>
          <rect x="3" y="5" width="16" height="12" />
          <path d="M6.5 9h9M6.5 12.5h6" />
        </svg>
      );
    case "decisions": // memo pile
      return (
        <svg {...common}>
          <path d="M5 7h11v11H5z" />
          <path d="M8 4h11v11" />
          <path d="M7.5 11h6M7.5 14h4" />
        </svg>
      );
    case "human-todo": // cork board with a pinned note
      return (
        <svg {...common}>
          <rect x="3" y="4" width="16" height="14" />
          <path d="M8 8h6v6H8z" />
          <circle cx="11" cy="8" r="1" fill="currentColor" />
        </svg>
      );
    case "todo": // whiteboard on legs
      return (
        <svg {...common}>
          <rect x="3" y="3" width="16" height="11" />
          <path d="M6.5 8.5l2 2 4-4M7 14l-2 5M15 14l2 5" />
        </svg>
      );
    case "spend": // meter
      return (
        <svg {...common}>
          <path d="M4 15a7 7 0 1 1 14 0" />
          <path d="M11 15l3.5-4.5" />
          <path d="M3 18h16" />
        </svg>
      );
  }
}

/** Up arrow in a lift-call button: back to the building. */
export function LiftIcon(): React.ReactElement {
  return (
    <svg {...common} width={16} height={16}>
      <rect x="3" y="3" width="16" height="16" />
      <path d="M11 15V7M7.5 10.5L11 7l3.5 3.5" />
    </svg>
  );
}

export function CloseIcon(): React.ReactElement {
  return (
    <svg {...common} width={16} height={16}>
      <path d="M5 5l12 12M17 5L5 17" />
    </svg>
  );
}
