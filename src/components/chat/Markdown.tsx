import { memo } from "react";
import ReactMarkdown from "react-markdown";
import type { Components } from "react-markdown";
import remarkGfm from "remark-gfm";

// An agent's message is untrusted text. Raw HTML is skipped, only these elements are
// drawn, and a link or an image is drawn as text: nothing can navigate the webview or load a file.
// See https://github.com/remarkjs/react-markdown#security
const ALLOWED = [
  "p",
  "br",
  "strong",
  "em",
  "del",
  "h1",
  "h2",
  "h3",
  "h4",
  "h5",
  "h6",
  "ul",
  "ol",
  "li",
  "blockquote",
  "code",
  "pre",
  "hr",
  "a",
  "img",
  "table",
  "thead",
  "tbody",
  "tr",
  "th",
  "td",
  "input",
];

const COMPONENTS: Components = {
  a: ({ href, children }) => (
    <>
      <span className="chat-link">{children}</span>
      {typeof href === "string" && href !== "" && href !== children ? (
        <span className="chat-address">{` (${href})`}</span>
      ) : null}
    </>
  ),
  img: ({ alt, src }) => (
    <span className="chat-image">
      {`image${alt ? `: ${alt}` : ""}${typeof src === "string" && src !== "" ? ` (${src})` : ""}`}
    </span>
  ),
  input: ({ type, checked }) =>
    type === "checkbox" ? (
      <input type="checkbox" checked={checked === true} disabled readOnly />
    ) : null,
};

/** An agent's message as Markdown, with raw HTML off and links and images drawn as text. */
export const Markdown = memo(function Markdown({
  text,
}: {
  readonly text: string;
}): React.ReactElement {
  return (
    <div className="chat-md">
      <ReactMarkdown
        skipHtml
        unwrapDisallowed
        allowedElements={ALLOWED}
        remarkPlugins={[remarkGfm]}
        components={COMPONENTS}
      >
        {text}
      </ReactMarkdown>
    </div>
  );
});
