import { useState } from "react";
import { MAX_BUILDING_NAME } from "../../state/useBuildingName";

interface Props {
  readonly name: string;
  readonly onRename: (name: string) => void;
  /** What the sign names, for its tooltip and field label. */
  readonly noun?: string;
}

/** The roof sign: click it to rename the building. Enter or leaving the field saves, Escape cancels. */
export function EditableSign({ name, onRename, noun = "building" }: Props): React.ReactElement {
  const [draft, setDraft] = useState<string | null>(null);
  const finish = (save: boolean): void => {
    if (save && draft !== null) onRename(draft);
    setDraft(null);
  };

  if (draft === null) {
    return (
      <button type="button" className="roof-sign" data-testid="roof-sign" title={`Rename the ${noun}`} onClick={() => setDraft(name)}>
        {name}
      </button>
    );
  }
  return (
    <input
      className="roof-sign roof-sign-input"
      data-testid="roof-sign-input"
      aria-label={`${noun.charAt(0).toUpperCase()}${noun.slice(1)} name`}
      value={draft}
      maxLength={MAX_BUILDING_NAME}
      size={Math.max(draft.length, 8)}
      autoFocus
      onFocus={(e) => e.currentTarget.select()}
      onChange={(e) => setDraft(e.target.value)}
      onBlur={() => finish(true)}
      onKeyDown={(e) => {
        if (e.key === "Enter") finish(true);
        else if (e.key === "Escape") finish(false);
      }}
    />
  );
}
