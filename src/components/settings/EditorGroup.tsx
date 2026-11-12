import type { EditorList, EditorSettings } from "../../types";
import { editorLabel, joinNames } from "../../state/selectors";

interface Props {
  readonly value: EditorSettings;
  readonly editors: EditorList | null;
  readonly onChange: (next: EditorSettings) => void;
}

/** What the picker means right now, in one sentence. */
function hint(value: EditorSettings, list: EditorList): string {
  const fallback = editorLabel(list, list.defaultEditor);
  if (fallback === undefined) {
    return `No editor was found on this Mac. The building looks for ${joinNames(list.editors.map((o) => o.label))}.`;
  }
  const preferred = list.editors.find((o) => o.editor === value.preferred);
  if (preferred !== undefined && !preferred.installed) {
    return `${preferred.label} is not installed, so ${fallback} is used until it is.`;
  }
  return `“Open in” on a desk opens its code in ${fallback}. The arrow beside it offers the other editors found on this Mac.`;
}

/** Which editor "Open in" uses. */
export function EditorGroup({ value, editors, onChange }: Props): React.ReactElement {
  return (
    <>
      <label className="field">
        <span>Default editor</span>
        <select
          data-testid="settings-editor"
          disabled={editors === null}
          value={value.preferred ?? ""}
          onChange={(e) => onChange({ preferred: editors?.editors.find((o) => o.editor === e.target.value)?.editor ?? null })}
        >
          {editors === null ? (
            <option value="">Looking for editors…</option>
          ) : (
            <>
              <option value="">First one found</option>
              {editors.editors.map((o) => (
                <option key={o.editor} value={o.editor} disabled={!o.installed && o.editor !== value.preferred}>
                  {o.installed ? o.label : `${o.label} (not installed)`}
                </option>
              ))}
            </>
          )}
        </select>
      </label>
      {editors === null ? null : <p className="panel-hint">{hint(value, editors)}</p>}
    </>
  );
}
