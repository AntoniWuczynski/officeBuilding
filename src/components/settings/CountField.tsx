import { useEffect, useState } from "react";

export interface CountSpec {
  readonly label: string;
  readonly id: string;
  readonly unit: string;
  readonly range: { readonly min: number; readonly max: number };
  readonly error: string;
}

/** The whole number in `text` when it lies in `range`, else null. */
export function parseCount(text: string, range: { readonly min: number; readonly max: number }): number | null {
  if (!/^\d+$/.test(text.trim())) return null;
  const n = Number(text.trim());
  return n >= range.min && n <= range.max ? n : null;
}

interface CountFieldProps {
  readonly spec: CountSpec;
  readonly saved: number;
  readonly onSave: (n: number) => void;
}

/** A whole-number field that saves on blur or Enter, and shows its range error until it is fixed. */
export function CountField({ spec, saved, onSave }: CountFieldProps): React.ReactElement {
  const [text, setText] = useState(String(saved));
  const [bad, setBad] = useState(false);
  useEffect(() => {
    setText(String(saved));
    setBad(false);
  }, [saved]);

  const commit = (): void => {
    const n = parseCount(text, spec.range);
    setBad(n === null);
    if (n !== null && n !== saved) onSave(n);
  };

  return (
    <>
      <label className="field">
        <span>{spec.label}</span>
        <span className="settings-cooldown">
          <input
            type="number"
            data-testid={spec.id}
            min={spec.range.min}
            max={spec.range.max}
            step={1}
            inputMode="numeric"
            value={text}
            onChange={(e) => setText(e.target.value)}
            onBlur={commit}
            onKeyDown={(e) => {
              if (e.key === "Enter") {
                e.preventDefault();
                commit();
              }
            }}
          />
          {spec.unit}
        </span>
      </label>
      {bad ? (
        <p className="form-error" role="alert">
          {spec.error}
        </p>
      ) : null}
    </>
  );
}
