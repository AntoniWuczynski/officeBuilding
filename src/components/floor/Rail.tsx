import { SectionIcon } from "../icons";
import type { Section } from "./sections";

interface RailProps {
  readonly sections: readonly { readonly id: Section; readonly label: string }[];
  readonly active: Section | null;
  /** Small count per section (open items, waiting decisions, spend). */
  readonly badges: Readonly<Partial<Record<Section, string | null>>>;
  readonly onSelect: (section: Section | null) => void;
}

/** The sketch's collapsed sidebar: always visible, click to enlarge a section. */
export function Rail({ sections, active, badges, onSelect }: RailProps): React.ReactElement {
  return (
    <nav className="rail" aria-label="Manager's office">
      {sections.map(({ id, label }) => {
        const badge = badges[id] ?? null;
        const on = active === id;
        return (
          <button
            key={id}
            type="button"
            className={on ? "rail-item is-on" : "rail-item"}
            data-testid={`rail-${id}`}
            aria-pressed={on}
            onClick={() => onSelect(on ? null : id)}
          >
            <SectionIcon section={id} />
            <span className="rail-label">{label}</span>
            {badge === null ? null : <span className={id === "decisions" ? "rail-badge is-waiting" : "rail-badge"}>{badge}</span>}
          </button>
        );
      })}
    </nav>
  );
}
