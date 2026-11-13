import { useState } from "react";
import { useLoad } from "../../state/useLoad";
import { message } from "../../lib/errors";

interface Props<S> {
  readonly load: () => Promise<S>;
  readonly save: (key: string | null) => Promise<S>;
  readonly sentence: (status: S) => string;
  /** Whether "Remove key" is offered. */
  readonly removable: (status: S) => boolean;
  /** Shown while a save runs. */
  readonly busy: string;
  readonly placeholder: string;
  /** Prefix of the test ids: `<testId>`, `-status`, `-save`, `-remove`. */
  readonly testId: string;
  readonly hint: React.ReactNode;
}

/** A password field with Save and Remove for a key kept in the Keychain. The typed key leaves the field when Save is pressed. */
export function KeyField<S>({ load, save, sentence, removable, busy, placeholder, testId, hint }: Props<S>): React.ReactElement {
  const { state, set } = useLoad(load);
  const [text, setText] = useState("");
  const [saving, setSaving] = useState(false);
  const [saveError, setSaveError] = useState<string | null>(null);

  if (state.phase === "reading") {
    return (
      <p className="panel-hint" role="status">
        Reading the key’s status…
      </p>
    );
  }
  if (state.phase === "failed") {
    return (
      <p className="form-error" role="alert">
        Could not read the key’s status: {state.message}.
      </p>
    );
  }
  const status = state.value;
  const run = (key: string | null): void => {
    setSaving(true);
    setSaveError(null);
    save(key).then(
      (next) => {
        set(next);
        setSaving(false);
      },
      (err: unknown) => {
        setSaveError(message(err));
        setSaving(false);
      },
    );
  };
  const submit = (): void => {
    const key = text;
    setText("");
    run(key);
  };
  return (
    <>
      <p className="panel-hint" role="status" data-testid={`${testId}-status`}>
        {saving ? busy : sentence(status)}
      </p>
      {saveError === null ? null : (
        <p className="form-error" role="alert">
          Could not save the key: {saveError}.
        </p>
      )}
      <label className="field">
        <span>API key</span>
        <input
          type="password"
          autoComplete="off"
          spellCheck={false}
          data-testid={testId}
          placeholder={placeholder}
          value={text}
          onChange={(e) => setText(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && text.trim() !== "" && !saving) {
              e.preventDefault();
              submit();
            }
          }}
        />
      </label>
      <div className="settings-test">
        <button type="button" className="btn" data-testid={`${testId}-save`} disabled={text.trim() === "" || saving} onClick={submit}>
          Save key
        </button>
        {removable(status) ? (
          <button type="button" className="btn" data-testid={`${testId}-remove`} disabled={saving} onClick={() => run(null)}>
            Remove key
          </button>
        ) : null}
      </div>
      <p className="panel-hint">{hint}</p>
    </>
  );
}
