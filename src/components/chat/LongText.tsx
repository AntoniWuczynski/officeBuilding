import { useState } from "react";

export const LONG_LINES = 40;
export const LONG_CHARS = 4000;

interface Collapsed {
  /** What to draw now. */
  readonly shown: string;
  /** The "Show all" or "Show less" button, or nothing for short text. */
  readonly toggle: React.ReactElement | null;
}

/** Cut long text to its first lines behind a "Show all" button. */
export function useCollapsed(text: string): Collapsed {
  const [open, setOpen] = useState(false);
  const lines = text.split("\n");
  if (lines.length <= LONG_LINES && text.length <= LONG_CHARS)
    return { shown: text, toggle: null };
  const cut =
    lines.length > LONG_LINES ? lines.slice(0, LONG_LINES).join("\n") : text;
  return {
    shown: open ? text : cut.slice(0, LONG_CHARS),
    toggle: (
      <button type="button" className="link" onClick={() => setOpen(!open)}>
        {open ? "Show less" : `Show all ${lines.length} lines`}
      </button>
    ),
  };
}

/** Plain text in a `<pre>`, collapsed when it is long. */
export function LongText({
  text,
  className,
}: {
  readonly text: string;
  readonly className?: string;
}): React.ReactElement {
  const { shown, toggle } = useCollapsed(text);
  return (
    <>
      <pre className={className}>{shown}</pre>
      {toggle}
    </>
  );
}
