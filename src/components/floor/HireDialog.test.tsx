import { describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { HireDialog } from "./HireDialog";
import { makeHireOptions, withDefaultView } from "../../api/mockHireOptions";
import type { HireRequest } from "../../api/DiscoveryApi";
import type { ToolOptions } from "../../types";

function open(options: readonly ToolOptions[] = makeHireOptions()): { onHire: ReturnType<typeof vi.fn<(r: HireRequest) => Promise<void>>>; unmount: () => void } {
  const onHire = vi.fn<(r: HireRequest) => Promise<void>>().mockResolvedValue(undefined);
  const { unmount } = render(<HireDialog projectName="arcade" loadOptions={() => Promise.resolve(options)} onHire={onHire} onCancel={() => {}} />);
  return { onHire, unmount };
}

describe("HireDialog tools", () => {
  it("greys out tools that are not installed and cannot select them", async () => {
    const user = userEvent.setup();
    open();
    const cursor = await screen.findByLabelText(/Cursor/);
    expect(cursor).toBeDisabled();
    expect(screen.getByLabelText(/opencode/)).toBeDisabled();
    expect(screen.getAllByText("Not installed")).toHaveLength(2);
    await user.click(cursor);
    expect(screen.getByLabelText("Claude Code")).toBeChecked();
    expect(screen.getByLabelText("Antigravity")).toBeEnabled();
  });

  it("starts on the first available tool", async () => {
    const [claude, ...rest] = makeHireOptions();
    open([{ ...(claude as ToolOptions), available: false, models: [] }, ...rest]);
    expect(await screen.findByLabelText("Codex")).toBeChecked();
  });
});

describe("HireDialog tool without models", () => {
  it("keeps the tool list so the user can switch away", async () => {
    const user = userEvent.setup();
    const options = makeHireOptions().map((o) => (o.tool === "antigravity" ? { ...o, models: [] } : o));
    open(options);
    await user.click(await screen.findByLabelText("Antigravity"));
    expect(screen.queryByTestId("hire-model")).not.toBeInTheDocument();
    await user.click(screen.getByLabelText("Codex"));
    expect(screen.getByLabelText("Codex")).toBeChecked();
    expect(screen.getByTestId("hire-model")).toBeInTheDocument();
  });
});

describe("HireDialog ultracode", () => {
  it("offers ultracode only for tools that have it, defaulting to the tool's setting, and sends it", async () => {
    const user = userEvent.setup();
    const { onHire } = open();
    const toggle = await screen.findByRole("switch", { name: "Ultracode" });
    expect(toggle).not.toBeChecked();
    await user.click(toggle);
    expect(toggle).toBeChecked();
    await user.click(screen.getByTestId("hire-submit"));
    expect(onHire).toHaveBeenCalledWith(expect.objectContaining({ tool: "claude-code", ultracode: true }));
  });

  it("hides the toggle and sends false for tools without ultracode", async () => {
    const user = userEvent.setup();
    const { onHire } = open();
    await user.click(await screen.findByLabelText("Codex"));
    expect(screen.queryByRole("switch", { name: "Ultracode" })).not.toBeInTheDocument();
    await user.click(screen.getByTestId("hire-submit"));
    expect(onHire).toHaveBeenCalledWith(expect.objectContaining({ tool: "codex", ultracode: false, view: "terminal" }));
  });

  it("starts on when the tool's default is on", async () => {
    const [claude, ...rest] = makeHireOptions();
    open([{ ...(claude as ToolOptions), ultracode: true }, ...rest]);
    expect(await screen.findByRole("switch", { name: "Ultracode" })).toBeChecked();
  });
});

describe("HireDialog chat view", () => {
  const chatFirst = (): readonly ToolOptions[] => withDefaultView(makeHireOptions(), "chat");

  it("starts off when the setting is Terminal and on when it is Chat", async () => {
    const first = open();
    expect(await screen.findByRole("switch", { name: "Chat view" })).not.toBeChecked();
    first.unmount();
    open(chatFirst());
    expect(await screen.findByRole("switch", { name: "Chat view" })).toBeChecked();
  });

  it("sends chat for Codex and says it runs without a terminal", async () => {
    const user = userEvent.setup();
    const { onHire } = open();
    await user.click(await screen.findByLabelText("Codex"));
    expect(screen.queryByTestId("hire-view-hint")).toBeNull();
    await user.click(screen.getByRole("switch", { name: "Chat view" }));
    expect(screen.getByTestId("hire-view-hint")).toHaveTextContent("It runs without a terminal, and the view cannot be changed after the hire.");
    await user.type(screen.getByPlaceholderText("What should they work on?"), "port it");
    await user.click(screen.getByRole("button", { name: /hire/i }));
    expect(onHire).toHaveBeenCalledWith(expect.objectContaining({ tool: "codex", view: "chat" }));
  });

  it("says Claude Code keeps its terminal", async () => {
    const user = userEvent.setup();
    open();
    await user.click(await screen.findByRole("switch", { name: "Chat view" }));
    expect(screen.getByTestId("hire-view-hint")).toHaveTextContent("The desk keeps its terminal.");
  });

  it("greys the switch out for Antigravity with the reason", async () => {
    const user = userEvent.setup();
    open(chatFirst());
    await user.click(await screen.findByLabelText("Antigravity"));
    expect(screen.getByRole("switch", { name: "Chat view" })).toBeDisabled();
    expect(screen.getByRole("switch", { name: "Chat view" })).not.toBeChecked();
    expect(screen.getByTestId("hire-view-hint")).toHaveTextContent(
      "Chat is not available: chat for Antigravity is not built yet, so it runs in the terminal.",
    );
  });

  it("resets to the next tool's default when the tool changes", async () => {
    const user = userEvent.setup();
    open();
    await user.click(await screen.findByRole("switch", { name: "Chat view" }));
    expect(screen.getByRole("switch", { name: "Chat view" })).toBeChecked();
    await user.click(screen.getByLabelText("Codex"));
    expect(screen.getByRole("switch", { name: "Chat view" })).not.toBeChecked();
  });
});
