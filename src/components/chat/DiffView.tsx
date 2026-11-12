import type { FileDiff } from "../../types";
import { useCollapsed } from "./LongText";

function lineClass(line: string): string | undefined {
  if (line.startsWith("@@")) return "is-hunk";
  if (line.startsWith("+")) return "is-add";
  if (line.startsWith("-")) return "is-del";
  return undefined;
}

/** One file's change, with added and removed lines coloured. The text is never parsed as Markdown or HTML. */
export function DiffView({
  diff,
}: {
  readonly diff: FileDiff;
}): React.ReactElement {
  const { shown, toggle } = useCollapsed(diff.text);
  return (
    <figure className="chat-diff">
      <figcaption>{diff.path}</figcaption>
      <pre>
        {shown.split("\n").map((line, i) => (
          <span key={i} className={lineClass(line)}>
            {line}
            {"\n"}
          </span>
        ))}
      </pre>
      {toggle}
    </figure>
  );
}
