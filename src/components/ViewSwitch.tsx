import type { ViewMode } from "../state/useViewMode";

const MODES: readonly (readonly [ViewMode, string])[] = [
  ["office", "Office"],
  ["islands", "Islands"],
];

/** Office building or island map: the same sessions, two overviews. */
export function ViewSwitch({ mode, onChange }: { readonly mode: ViewMode; readonly onChange: (mode: ViewMode) => void }): React.ReactElement {
  return (
    <div className="overview-switch" role="group" aria-label="Overview">
      {MODES.map(([m, label]) => (
        <button key={m} type="button" aria-pressed={mode === m} data-testid={`view-${m}`} onClick={() => onChange(m)}>
          {label}
        </button>
      ))}
    </div>
  );
}
