import { describe, expect, it } from "vitest";
import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MockApi } from "../../api/mockApi";
import { SetupPanel } from "./SetupPanel";

function open(): MockApi {
  const api = new MockApi();
  render(
    <SetupPanel
      load={() => api.setupView()}
      subscribe={(l) => api.subscribeSetupChanges(l)}
      onSetTargets={(k, n, t) => api.setSetupTargets(k, n, t)}
      onImport={(f, k, n) => api.importSetupItem(f, k, n)}
    />,
  );
  return api;
}

describe("SetupPanel", () => {
  it("aims an item at all tools, then at the ones picked", async () => {
    const user = userEvent.setup();
    const api = open();
    const item = within(await screen.findByTestId("setup-item-release-notes"));
    expect(item.getByLabelText("All tools")).toBeChecked();
    await user.click(item.getByLabelText("Antigravity"));
    expect(item.getByLabelText("All tools")).not.toBeChecked();
    expect(item.getByLabelText("Antigravity")).not.toBeChecked();
    expect(item.getByLabelText("Claude Code")).toBeChecked();
    const saved = (await api.setupView()).items.find((i) => i.name === "release-notes");
    expect(saved?.targets).toEqual({ kind: "only", tools: ["claude-code", "codex", "antigravity", "opencode", "cursor"].filter((t) => t !== "antigravity") });
    await user.click(item.getByLabelText("All tools"));
    expect(item.getByLabelText("All tools")).toBeChecked();
  });

  it("greys out a tool that cannot take the item, with the reason", async () => {
    open();
    const skill = within(await screen.findByTestId("setup-item-release-notes"));
    expect(skill.getByLabelText("Cursor")).toBeEnabled();
    const server = within(screen.getByTestId("setup-item-docs"));
    expect(server.getByLabelText("Cursor")).toBeDisabled();
    expect(server.getByLabelText("Cursor").closest("label")).toHaveAttribute("title", expect.stringMatching(/skills only/));
    expect(server.getByLabelText("Codex")).toBeEnabled();
    expect(server.getByLabelText("Antigravity")).toBeDisabled();
  });

  it("lists what each tool already has, and what is not read yet", async () => {
    open();
    const claude = await screen.findByTestId("setup-harness-claude-code");
    expect(claude).toHaveTextContent("2 skills · 1 MCP server · 3 hooks · 2 plugins");
    expect(claude).toHaveTextContent("vercel@claude-plugins-official (off)");
    expect(screen.getByTestId("setup-harness-antigravity")).toHaveTextContent("Hooks: not read yet");
    expect(screen.getByTestId("setup-harness-cursor")).toHaveTextContent("Not read yet");
  });

  it("refreshes in place when the setup changes on disk", async () => {
    const api = new MockApi();
    let changed = (): void => {};
    let view = await api.setupView();
    render(
      <SetupPanel
        load={() => Promise.resolve(view)}
        subscribe={(l) => {
          changed = l;
          return () => {};
        }}
        onSetTargets={(k, n, t) => api.setSetupTargets(k, n, t)}
        onImport={(f, k, n) => api.importSetupItem(f, k, n)}
      />,
    );
    await screen.findByTestId("setup-item-release-notes");
    view = { ...view, items: [...view.items, { kind: "skill", name: "new-one", description: "Added by an agent.", targets: { kind: "all" } }] };
    changed();
    expect(await screen.findByTestId("setup-item-new-one")).toBeInTheDocument();
    expect(screen.getByTestId("setup-item-release-notes")).toBeInTheDocument();
  });

  it("copies a tool's own skill into the building's setup, aimed at the other tools", async () => {
    const user = userEvent.setup();
    open();
    const [fromClaude] = await screen.findAllByRole("button", { name: "Copy humanizer into the building's setup" });
    await user.click(fromClaude as HTMLElement);
    const item = within(await screen.findByTestId("setup-item-humanizer"));
    expect(item.getByLabelText("Claude Code")).not.toBeChecked();
    expect(item.getByLabelText("Codex")).toBeChecked();
    // Codex has one too; both now read as in the building.
    expect(screen.queryAllByRole("button", { name: "Copy humanizer into the building's setup" })).toHaveLength(0);
  });

  it("lists instructions and hooks, and says what Codex still needs for a hook", async () => {
    open();
    expect(await screen.findByTestId("setup-item-house-rules")).toHaveTextContent("Instructions · British English, no semicolons.");
    const hook = within(screen.getByTestId("setup-item-format"));
    expect(hook.getByText("Hook · PostToolUse (Edit|Write): ./format.sh")).toBeInTheDocument();
    expect(hook.getByLabelText("Codex")).toBeEnabled();
    expect(hook.getByLabelText("Codex").closest("label")).toHaveAttribute("title", expect.stringMatching(/trust it once/));
    expect(hook.getByLabelText("opencode")).toBeDisabled();
  });

  it("copies one of a tool's own hooks into the building, for the tools that take hooks", async () => {
    const user = userEvent.setup();
    open();
    await user.click(await screen.findByRole("button", { name: "Copy Stop: sync.sh into the building's setup" }));
    const item = within(await screen.findByTestId("setup-item-claude-code-stop-1"));
    expect(item.getByText("Hook · Stop: sync.sh")).toBeInTheDocument();
    expect(item.getByLabelText("Codex")).toBeChecked();
    expect(item.getByLabelText("Claude Code")).not.toBeChecked();
    expect(screen.queryByRole("button", { name: "Copy Stop: sync.sh into the building's setup" })).toBeNull();
  });

  it("copies everything a tool has left into the building at once, without opening its list", async () => {
    const user = userEvent.setup();
    open();
    await user.click(await screen.findByTestId("setup-copy-all-codex"));
    expect(await screen.findByTestId("setup-item-codex-instructions")).toBeInTheDocument();
    expect(screen.getByTestId("setup-item-humanizer")).toBeInTheDocument();
    expect(screen.queryByTestId("setup-copy-all-codex")).toBeNull();
    expect(screen.getByTestId("setup-harness-codex")).not.toHaveAttribute("open");
  });

  it("keeps copying the rest when one item cannot be copied, and says which", async () => {
    const user = userEvent.setup();
    const api = new MockApi();
    render(
      <SetupPanel
        load={() => api.setupView()}
        subscribe={(l) => api.subscribeSetupChanges(l)}
        onSetTargets={(k, n, t) => api.setSetupTargets(k, n, t)}
        onImport={(f, k, n) => (k === "mcp-server" ? Promise.reject(new Error(`${n} cannot be copied yet`)) : api.importSetupItem(f, k, n))}
      />,
    );
    await user.click(await screen.findByTestId("setup-copy-all-claude-code"));
    // Hooks come after the servers, and are still copied.
    expect(await screen.findByTestId("setup-item-claude-code-stop-1")).toBeInTheDocument();
    expect(screen.getByRole("alert")).toHaveTextContent(/cannot be copied yet/);
  });
});
