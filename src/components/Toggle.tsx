export const classes = (...names: readonly (string | false)[]): string | undefined =>
  names.filter(Boolean).join(" ") || undefined;

/** A labelled on/off switch, drawn as a choice. */
export function Toggle({
  label,
  title,
  checked,
  disabled,
  onChange,
}: {
  readonly label: string;
  readonly title?: string;
  readonly checked: boolean;
  readonly disabled: boolean;
  readonly onChange: (on: boolean) => void;
}): React.ReactElement {
  return (
    <label className={classes("choice-toggle", checked && "is-on")} title={title}>
      <input type="checkbox" role="switch" checked={checked} disabled={disabled} onChange={(e) => onChange(e.target.checked)} />
      {label}
    </label>
  );
}
