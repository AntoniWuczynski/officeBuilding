/** What a desk's take-overs have to say, in its panel, and the button that starts a new session beside it. */
export function TakeOverStrip({ lines, onTakeOver }: { readonly lines: readonly string[]; readonly onTakeOver?: (() => void) | undefined }): React.ReactElement | null {
  if (lines.length === 0 && onTakeOver === undefined) return null;
  return (
    <div className="route-strip handover-strip" data-testid="take-over-strip">
      {lines.length === 0 ? null : <span className="route-lamp" data-tone="ok" aria-hidden="true" />}
      {lines.map((line) => (
        <span key={line}>{line}</span>
      ))}
      {onTakeOver === undefined ? null : (
        <button type="button" className="btn dark-btn" data-testid="take-over-button" onClick={onTakeOver}>
          Take over with a new session
        </button>
      )}
    </div>
  );
}
