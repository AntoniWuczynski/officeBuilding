import { describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { HireDialog } from "./HireDialog";
import { makeHireOptions } from "../../api/mockHireOptions";
import type { HireRequest } from "../../api/DiscoveryApi";
import type { ToolOptions } from "../../types";

function open(options: readonly ToolOptions[] = makeHireOptions()): { onHire: ReturnType<typeof vi.fn<(r: HireRequest) => Promise<void>>> } {
  const onHire = vi.fn<(r: HireRequest) => Promise<void>>().mockResolvedValue(undefined);
  render(<HireDialog projectName="arcade" loadOptions={() => Promise.resolve(options)} onHire={onHire} onCancel={() => {}} />);
  return { onHire };
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
    expect(onHire).toHaveBeenCalledWith(expect.objectContaining({ tool: "codex", ultracode: false, structured: false }));
  });

  it("starts on when the tool's default is on", async () => {
    const [claude, ...rest] = makeHireOptions();
    open([{ ...(claude as ToolOptions), ultracode: true }, ...rest]);
    expect(await screen.findByRole("switch", { name: "Ultracode" })).toBeChecked();
  });
});

describe("HireDialog structured desks", () => {
  it("offers to run without a terminal only for tools that can, and sends the choice", async () => {
    const user = userEvent.setup();
    const { onHire } = open();
    await screen.findByLabelText("Claude Code");
    expect(screen.queryByRole("switch", { name: "Without a terminal" })).toBeNull();
    await user.click(screen.getByLabelText("Codex"));
    await user.click(screen.getByRole("switch", { name: "Without a terminal" }));
    await user.type(screen.getByPlaceholderText("What should they work on?"), "port it");
    await user.click(screen.getByRole("button", { name: /hire/i }));
    expect(onHire).toHaveBeenCalledWith(expect.objectContaining({ tool: "codex", structured: true }));
  });
});
