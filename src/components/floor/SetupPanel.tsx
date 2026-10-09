import { useCallback, useEffect, useState } from "react";
import type { HarnessSetup, HookInfo, SetupItemKind, SetupView, Support, Targets, ToolKind } from "../../types";
import { TOOL_LABEL } from "../../state/selectors";

export interface SetupProps {
  readonly load: () => Promise<SetupView>;
  /** Hear when the setup changed on disk; returns an unsubscribe. */
  readonly subscribe: (listener: () => void) => () => void;
  readonly onSetTargets: (kind: SetupItemKind, name: string, targets: Targets) => Promise<SetupView>;
  readonly onImport: (from: ToolKind, kind: SetupItemKind, name: string) => Promise<SetupView>;
}

type Loaded = { readonly kind: "loading" } | { readonly kind: "error"; readonly message: string } | { readonly kind: "ready"; readonly view: SetupView };

function message(e: unknown): string {
  return e instanceof Error ? e.message : String(e);
}

/** How each kind of item is introduced in its row (a skill just shows its description). */
const KIND_LABEL: Readonly<Record<SetupItemKind, string>> = {
  skill: "",
  instructions: "Instructions",
  "mcp-server": "MCP server",
  hook: "Hook",
};

function detail(kind: SetupItemKind, description: string): string {
  const label = KIND_LABEL[kind];
  if (label === "") return description;
  return description === "" ? label : `${label} · ${description}`;
}

/** The tooltip on a tool's box: why it cannot take the item, or what is left to do. */
function hint(support: Support): string | undefined {
  if (support.kind === "unsupported") return support.reason;
  return support.note ?? undefined;
}

/** The setup floor's panel: the building's items, each aimed at chosen tools, and what each tool already has. */
export function SetupPanel({ load, subscribe, onSetTargets, onImport }: SetupProps): React.ReactElement {
  const [loaded, setLoaded] = useState<Loaded>({ kind: "loading" });
  const [saveErrors, setSaveErrors] = useState<readonly string[]>([]);

  const reload = useCallback(() => {
    setLoaded({ kind: "loading" });
    load().then(
      (view) => setLoaded({ kind: "ready", view }),
      (e: unknown) => setLoaded({ kind: "error", message: message(e) }),
    );
  }, [load]);
  useEffect(reload, [reload]);
  // A change on disk refreshes in place, keeping the current view until the new one arrives.
  useEffect(
    () =>
      subscribe(() => {
        load().then(
          (view) => setLoaded({ kind: "ready", view }),
          () => {},
        );
      }),
    [subscribe, load],
  );

  const save = (change: Promise<SetupView>): void => {
    setSaveErrors([]);
    change.then(
      (view) => setLoaded({ kind: "ready", view }),
      (e: unknown) => setSaveErrors([message(e)]),
    );
  };
  const aim = (kind: SetupItemKind, name: string, targets: Targets): void => save(onSetTargets(kind, name, targets));
  // One at a time, since each copy rewrites the building's setup.json, and one that fails does not stop the rest.
  const importAll = async (from: ToolKind, picks: readonly Pick[]): Promise<void> => {
    setSaveErrors([]);
    const failed: string[] = [];
    for (const p of picks) {
      try {
        const view = await onImport(from, p.kind, p.name);
        setLoaded({ kind: "ready", view });
      } catch (e: unknown) {
        failed.push(message(e));
      }
    }
    setSaveErrors(failed);
  };

  if (loaded.kind === "loading") {
    return (
      <div className="panel-body">
        <p className="panel-empty" role="status">
          Reading each tool’s setup…
        </p>
      </div>
    );
  }
  if (loaded.kind === "error") {
    return (
      <div className="panel-body">
        <p className="panel-empty" role="alert">
          The setup could not be read: {loaded.message}
        </p>
        <button type="button" className="btn" onClick={reload}>
          Try again
        </button>
      </div>
    );
  }
  const { view } = loaded;
  const { items } = view;
  return (
    <div className="panel-body" data-testid="setup-panel">
      <p className="panel-hint">
        The building hands these to the agents it starts, and only to those. Your own tool folders are never changed. Hire an agent on this floor to add skills under{" "}
        <code>skills/</code>, instructions under <code>instructions/</code>, or MCP servers and hooks in <code>setup.json</code>.
      </p>
      {view.problems.map((p) => (
        <p key={p} className="panel-hint setup-problem" role="alert">
          {p}
        </p>
      ))}
      {saveErrors.map((e) => (
        <p key={e} className="panel-hint setup-problem" role="alert">
          That change was not saved: {e}
        </p>
      ))}

      <h3 className="panel-subhead">The building’s setup</h3>
      {items.length === 0 ? <p className="panel-empty">Nothing here yet.</p> : null}
      <ul className="setup-list">
        {items.map((item) => (
          <li key={`${item.kind}:${item.name}`} className="setup-item" data-testid={`setup-item-${item.name}`}>
            <span className="setup-name">{item.name}</span>
            {detail(item.kind, item.description) === "" ? null : <span className="setup-detail">{detail(item.kind, item.description)}</span>}
            <TargetPicker
              harnesses={view.harnesses}
              kind={item.kind}
              targets={item.targets}
              onChange={(targets) => aim(item.kind, item.name, targets)}
            />
          </li>
        ))}
      </ul>

      <div className="setup-installed-head">
        <h3 className="panel-subhead">Already in each tool</h3>
        <button type="button" className="btn" data-testid="setup-recheck" onClick={reload}>
          Re-check
        </button>
      </div>
      {view.harnesses.map((h) => (
        <Inventory
          key={h.tool}
          harness={h}
          inLibrary={(kind, name) => items.some((i) => i.kind === kind && i.name === name)}
          onImport={(kind, name) => save(onImport(h.tool, kind, name))}
          onImportAll={(picks) => void importAll(h.tool, picks)}
        />
      ))}
    </div>
  );
}

function TargetPicker({
  harnesses,
  kind,
  targets,
  onChange,
}: {
  readonly harnesses: readonly HarnessSetup[];
  readonly kind: SetupItemKind;
  readonly targets: Targets;
  readonly onChange: (targets: Targets) => void;
}): React.ReactElement {
  const all = targets.kind === "all";
  const has = (tool: ToolKind): boolean => all || (targets.kind === "only" && targets.tools.includes(tool));
  const toggle = (tool: ToolKind): void => {
    const chosen = harnesses.map((h) => h.tool).filter((t) => (t === tool ? !has(t) : has(t)));
    onChange({ kind: "only", tools: chosen });
  };
  return (
    <div className="setup-targets" role="group" aria-label="Tools">
      <label className="setup-target">
        <input type="checkbox" checked={all} onChange={() => onChange(all ? { kind: "only", tools: [] } : { kind: "all" })} />
        All tools
      </label>
      {harnesses.map((h) => {
        const support = h.support[kind];
        const off = support.kind === "unsupported";
        return (
          <label key={h.tool} className={off ? "setup-target is-off" : "setup-target"} title={hint(support)}>
            <input type="checkbox" checked={has(h.tool) && !off} disabled={off} onChange={() => toggle(h.tool)} />
            {TOOL_LABEL[h.tool]}
          </label>
        );
      })}
    </div>
  );
}

function count(n: number, one: string, many: string): string {
  return `${n} ${n === 1 ? one : many}`;
}

interface InventoryProps {
  readonly harness: HarnessSetup;
  readonly inLibrary: (kind: SetupItemKind, name: string) => boolean;
  readonly onImport: (kind: SetupItemKind, name: string) => void;
  readonly onImportAll: (picks: readonly Pick[]) => void;
}

/** One of a tool's items that can be copied into the building. */
interface Pick {
  readonly kind: SetupItemKind;
  readonly name: string;
}

/**
 * The name a copied item gets in the building (src-tauri/src/setup/import.rs):
 * skills and servers keep theirs, instructions become `<tool>-instructions`,
 * hooks `<tool>-<event>-<n>`.
 */
function copiedName(tool: HarnessSetup["tool"], kind: SetupItemKind, name: string): string {
  if (kind === "instructions") return `${tool}-instructions`;
  if (kind === "hook") return `${tool}-${name.replace(":", "-").toLowerCase()}`;
  return name;
}

function hookLabel(h: HookInfo): string {
  return `${h.event}${h.matcher === null ? "" : ` (${h.matcher})`}: ${h.command}`;
}

function Inventory({ harness, inLibrary, onImport, onImportAll }: InventoryProps): React.ReactElement {
  const { skills, mcpServers, hooks, plugins, instructions } = harness;
  const parts = [
    skills === null ? null : count(skills.length, "skill", "skills"),
    mcpServers === null ? null : count(mcpServers.length, "MCP server", "MCP servers"),
    hooks === null ? null : count(hooks.length, "hook", "hooks"),
    plugins === null ? null : count(plugins.length, "plugin", "plugins"),
  ].filter((p) => p !== null);
  const summary = parts.length === 0 ? "Not read yet" : parts.join(" · ");
  const groups = (
    [
      ["Instructions", "instructions", instructions?.map((n) => ({ name: n, label: n }))],
      ["Skills", "skill", skills?.map((s) => ({ name: s.name, label: s.name }))],
      ["MCP servers", "mcp-server", mcpServers?.map((n) => ({ name: n, label: n }))],
      ["Hooks", "hook", hooks?.map((h) => ({ name: h.id, label: hookLabel(h) }))],
    ] as const
  ).map(([title, kind, items]) => ({
    title,
    kind,
    items: items?.map((i) => ({ ...i, copied: inLibrary(kind, copiedName(harness.tool, kind, i.name)) })),
  }));
  const left = groups.flatMap(({ kind, items }) => (items ?? []).filter((i) => !i.copied).map((i) => ({ kind, name: i.name })));
  return (
    <details className="setup-harness" data-testid={`setup-harness-${harness.tool}`}>
      <summary>
        <span className="setup-name">{TOOL_LABEL[harness.tool]}</span>
        <span className="setup-detail">
          {harness.installed ? "" : "Not installed · "}
          {summary}
        </span>
        {left.length === 0 ? null : (
          <button
            type="button"
            className="link setup-copy-all"
            data-testid={`setup-copy-all-${harness.tool}`}
            aria-label={`Copy all ${left.length} of ${TOOL_LABEL[harness.tool]}'s items into the building's setup`}
            onClick={(e) => {
              // Inside the summary: copy without opening or closing the list.
              e.preventDefault();
              onImportAll(left);
            }}
          >
            Copy all in ({left.length})
          </button>
        )}
      </summary>
      {groups.map(({ title, kind, items }) => (
        <Copyable key={kind} title={title} items={items} onImport={(name) => onImport(kind, name)} />
      ))}
      <InventoryList title="Plugins" items={plugins?.map((p) => (p.enabled ? p.name : `${p.name} (off)`))} />
    </details>
  );
}

function InventoryList({ title, items }: { readonly title: string; readonly items: readonly string[] | null | undefined }): React.ReactElement {
  return (
    <p className="setup-inventory">
      <b>{title}:</b> {items == null ? "not read yet" : items.length === 0 ? "none" : items.join(", ")}
    </p>
  );
}

/** A tool's own items of one kind, each with a button that copies it into the building's setup. */
function Copyable({
  title,
  items,
  onImport,
}: {
  readonly title: string;
  readonly items: readonly { readonly name: string; readonly label: string; readonly copied: boolean }[] | null | undefined;
  readonly onImport: (name: string) => void;
}): React.ReactElement {
  if (items == null || items.length === 0) return <InventoryList title={title} items={items?.map((i) => i.label)} />;
  return (
    <div className="setup-inventory">
      <b>{title}:</b>
      <ul className="setup-copy-list">
        {items.map((item) => (
          <li key={item.name}>
            <span>{item.label}</span>
            {item.copied ? (
              <span className="setup-detail">In the building</span>
            ) : (
              <button type="button" className="link" aria-label={`Copy ${item.label} into the building's setup`} onClick={() => onImport(item.name)}>
                Copy in
              </button>
            )}
          </li>
        ))}
      </ul>
    </div>
  );
}
