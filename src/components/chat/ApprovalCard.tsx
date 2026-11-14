import { useState } from "react";
import type { Approval, ApprovalField, ApprovalMode } from "../../types";

/**
 * The agent's request. Inline it has its fields and one button per answer. In a
 * terminal the answer is given there, so the card only offers to flip to it.
 * Where the session runs elsewhere, it says so.
 */
export function ApprovalCard({
  approval,
  mode,
  onAnswer,
  onFlip,
}: {
  readonly approval: Approval;
  readonly mode: ApprovalMode;
  readonly onAnswer: (choiceId: string, values: Readonly<Record<string, string>>) => void;
  readonly onFlip?: (() => void) | undefined;
}): React.ReactElement {
  const [values, setValues] = useState<Readonly<Record<string, string>>>({});
  const set = (id: string, value: string): void => setValues((v) => ({ ...v, [id]: value }));
  const missing = approval.fields.some((f) => f.required && (values[f.id] ?? "").trim() === "");
  return (
    <section className="conversation-approval" role="group" aria-label={`The agent asks: ${approval.title}`} data-testid="conversation-approval">
      <b>{approval.title}</b>
      {approval.detail === "" ? null : <pre>{approval.detail}</pre>}
      {mode === "none" ? <p className="chat-note">Answer it where the session runs.</p> : null}
      {mode === "terminal" && onFlip !== undefined ? (
        <div className="conversation-choices">
          <button type="button" className="btn btn-primary" onClick={onFlip}>
            Answer in the terminal
          </button>
        </div>
      ) : null}
      {mode === "inline" ? approval.fields.map((f) => <FieldInput key={f.id} field={f} value={values[f.id] ?? ""} onChange={(v) => set(f.id, v)} />) : null}
      {mode === "inline" ? (
        <div className="conversation-choices">
          {approval.choices.map((c) => (
            <button
              key={c.id}
              type="button"
              className={c.allow ? "btn btn-primary" : "btn"}
              disabled={c.allow && missing}
              title={c.allow && missing ? "Fill in every field marked as needed first" : undefined}
              onClick={() => onAnswer(c.id, values)}
            >
              {c.label}
            </button>
          ))}
        </div>
      ) : null}
    </section>
  );
}

/** One field: text (hidden for a secret), a number, yes or no, or a pick-list with an optional "other". */
function FieldInput({
  field,
  value,
  onChange,
}: {
  readonly field: ApprovalField;
  readonly value: string;
  readonly onChange: (value: string) => void;
}): React.ReactElement {
  const label = `${field.label}${field.required ? " (needed)" : ""}`;
  const { kind } = field;
  if (kind.kind === "boolean") {
    return (
      <label className="conversation-field is-check">
        <input type="checkbox" checked={value === "true"} onChange={(e) => onChange(e.target.checked ? "true" : "false")} />
        {label}
      </label>
    );
  }
  if (kind.kind === "pick") {
    const picked = kind.options.includes(value);
    const otherOn = kind.other && value !== "" && !picked;
    return (
      <label className="conversation-field">
        <span>{label}</span>
        <select value={otherOn ? OTHER : value} onChange={(e) => onChange(e.target.value === OTHER ? " " : e.target.value)}>
          <option value="">Choose…</option>
          {kind.options.map((o) => (
            <option key={o} value={o}>
              {o}
            </option>
          ))}
          {kind.other ? <option value={OTHER}>Something else…</option> : null}
        </select>
        {otherOn ? (
          <input
            aria-label={`${field.label}: your own answer`}
            value={value.trimStart()}
            onChange={(e) => onChange(e.target.value === "" ? " " : e.target.value)}
          />
        ) : null}
      </label>
    );
  }
  return (
    <label className="conversation-field">
      <span>{label}</span>
      <input type={kind.kind === "secret" ? "password" : kind.kind === "number" ? "number" : "text"} value={value} onChange={(e) => onChange(e.target.value)} />
    </label>
  );
}

/** The pick-list entry that opens a text box for an answer of the human's own. */
const OTHER = "\u0000other";
