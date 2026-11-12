import type { Verdict } from "../../types";
import { STAMP_WORDS } from "../../state/selectors";

/** The rubber stamp a QA desk leaves: drawn lettering, the words differ as well as the colour. */
export function QaStamp({ verdict }: { readonly verdict: Verdict }): React.ReactElement {
  return (
    <span className={`qa-stamp is-${verdict}`} data-testid="qa-stamp" title={STAMP_WORDS[verdict].spoken}>
      {STAMP_WORDS[verdict].label}
    </span>
  );
}
