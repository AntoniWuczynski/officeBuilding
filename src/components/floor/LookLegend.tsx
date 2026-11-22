import { BUILDS, BUILD_LABEL, TIER_FAMILIES, TIER_WORKERS } from "../../lib/modelTiers";
import { lookOf } from "../../lib/looks";
import { TOOL_WOOD, WOODS, deskFaces } from "../../lib/woods";
import type { ToolKind } from "../../types";
import { TOOL_LABEL, capital } from "../../state/selectors";
import "./legend.css";

const isTool = (key: string): key is ToolKind => Object.hasOwn(TOOL_WOOD, key);
const TOOLS: readonly ToolKind[] = Object.keys(TOOL_WOOD).filter(isTool);

/** What the wood, the worker, the build and the tint of a desk stand for, drawn from the tables the look itself reads. Folded in the Info panel. */
export function LookLegend(): React.ReactElement {
  return (
    <details className="look-legend" data-testid="look-legend">
      <summary>What the looks mean</summary>
      <h4>The desk&apos;s wood is the tool</h4>
      <ul>
        {TOOLS.map((tool) => (
          <li key={tool}>
            <i className="legend-chip" style={{ background: deskFaces(TOOL_WOOD[tool]).top }} />
            {TOOL_LABEL[tool]}: {WOODS[TOOL_WOOD[tool]].label.toLowerCase()}
          </li>
        ))}
      </ul>
      <h4>The worker is the model</h4>
      <ul>
        {TIER_WORKERS.map((tier) => {
          const head = lookOf({ id: "", model: TIER_FAMILIES[tier][0], effort: null }).parts.head;
          return (
            <li key={tier}>
              <img className="legend-face" src={head.src} alt="" draggable={false} />
              {TIER_FAMILIES[tier].map(capital).join(" and ")}
            </li>
          );
        })}
      </ul>
      <p>Any other model, or one the tool does not report, is one of six other workers.</p>
      <h4>The build is the effort, lean to buff</h4>
      <ul className="legend-builds">
        {BUILDS.map((build) => {
          const torso = lookOf({ id: "", model: TIER_FAMILIES.kit[0], effort: build }).parts.torso;
          return (
            <li key={build}>
              <img className="legend-torso" src={torso.src} alt="" draggable={false} />
              {BUILD_LABEL[build]}
            </li>
          );
        })}
      </ul>
      <p>Ultra is drawn as Max. A worker whose effort is not reported has the medium build.</p>
      <p>
        <i className="legend-chip" style={{ background: "var(--ultracode)" }} />A purple tint: Claude Code, hired here with ultracode on.
      </p>
      <p>The manager always looks the same.</p>
    </details>
  );
}
