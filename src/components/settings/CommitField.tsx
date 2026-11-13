import { useEffect, useState } from "react";

interface CommitProps {
  readonly label: string;
  readonly testId?: string;
  readonly value: string;
  readonly placeholder?: string;
  readonly disabled?: boolean;
  /** Called with the text on blur and on Enter when it changed. Returns a sentence when the text is refused. */
  readonly onCommit: (text: string) => string | null;
}

/** A text field that saves on blur and on Enter, and shows why a text was refused until it is fixed. */
export function CommitField({ label, testId, value, placeholder, disabled, onCommit }: CommitProps): React.ReactElement {
  const [text, setText] = useState(value);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    setText(value);
    setError(null);
  }, [value]);
  const commit = (): void => {
    if (text === value) {
      setError(null);
      return;
    }
    setError(onCommit(text));
  };
  return (
    <>
      <label className="field">
        <span>{label}</span>
        <input
          type="text"
          data-testid={testId}
          value={text}
          placeholder={placeholder}
          disabled={disabled}
          spellCheck={false}
          onChange={(e) => setText(e.target.value)}
          onBlur={commit}
          onKeyDown={(e) => {
            if (e.key === "Enter") {
              e.preventDefault();
              commit();
            }
          }}
        />
      </label>
      {error === null ? null : (
        <p className="form-error" role="alert">
          {error}
        </p>
      )}
    </>
  );
}
