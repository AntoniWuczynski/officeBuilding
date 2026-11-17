import { useState } from "react";
import "./boards.css";

interface Props {
  /** "island" or "floor". */
  readonly noun: string;
  readonly tags: readonly string[];
  readonly onChange: (tags: readonly string[]) => void;
}

/** The shared-board tags linked to this place, besides its folder name and its own name: each can be unlinked, a new one is typed in. */
export function PlaceTags({ noun, tags, onChange }: Props): React.ReactElement {
  const [draft, setDraft] = useState("");
  return (
    <section className="place-tags" aria-label={`Tags shown on this ${noun}`}>
      <h3>Tags shown on this {noun}</h3>
      {tags.length === 0 ? null : (
        <div className="place-tag-list">
          {tags.map((t) => (
            <span key={t} className="place-tag" data-testid="place-tag">
              {t}
              <button type="button" aria-label={`Unlink the tag ${t}`} onClick={() => onChange(tags.filter((x) => x !== t))}>
                ×
              </button>
            </span>
          ))}
        </div>
      )}
      <form
        onSubmit={(e) => {
          e.preventDefault();
          if (draft.trim() === "") return;
          onChange([...tags, draft]);
          setDraft("");
        }}
      >
        <input aria-label={`Link a board tag to this ${noun}`} value={draft} placeholder="Link a tag" spellCheck={false} onChange={(e) => setDraft(e.target.value)}
          onKeyDown={(e) => {
            // A first Escape drops the draft; handled here, so the panel's Escape waits for the next one.
            if (e.key !== "Escape" || draft === "") return;
            e.preventDefault();
            setDraft("");
          }}
        />
        <button type="submit" className="btn" disabled={draft.trim() === ""}>
          Link
        </button>
      </form>
    </section>
  );
}
