import { describe, expect, it, vi } from "vitest";
import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { makeHireOptions } from "../../api/mockHireOptions";
import { makeHandoverSettings } from "../../api/mockHandover";
import type { HandoverSettings, ToolOptions } from "../../types";
import { HandoverGroup } from "./HandoverGroup";

function show(value: HandoverSettings = makeHandoverSettings(), load: () => Promise<readonly ToolOptions[]> = () => Promise.resolve(makeHireOptions())): ReturnType<typeof vi.fn> {
  const onChange = vi.fn();
  render(<HandoverGroup value={value} loadOptions={load} onChange={onChange} />);
  return onChange;
}

const codex = (): HTMLElement => screen.getByTestId("settings-handover-codex");
const claude = (): HTMLElement => screen.getByTestId("settings-handover-claude-code");

describe("HandoverGroup", () => {
  it("preselects nothing but Not chosen in either fieldset, and has no not-built line", () => {
    show();
    for (const scope of [codex(), claude()]) {
      expect(within(scope).getByRole("radio", { name: "Not chosen" })).toBeChecked();
      expect(within(scope).getByRole("radio", { name: "A model I choose" })).not.toBeChecked();
    }
    expect(within(codex()).getByRole("radio", { name: "Codex's own configured default" })).not.toBeChecked();
    expect(within(claude()).getByRole("radio", { name: "Claude Code's own configured default" })).not.toBeChecked();
    expect(screen.queryByTestId("settings-handover-codex-model")).not.toBeInTheDocument();
    expect(screen.queryByText(/not built yet/)).not.toBeInTheDocument();
    expect(screen.getByText(/Nothing is handed over in a direction until you have chosen/)).toBeInTheDocument();
    expect(screen.getByText(/Claude Code is given leave to read that one file/)).toBeInTheDocument();
    expect(screen.getByText(/A goal you cleared still reads as set/)).toBeInTheDocument();
  });

  it("saves the switch", async () => {
    const onChange = show();
    await userEvent.click(screen.getByRole("switch", { name: "Hand desks over by themselves" }));
    expect(onChange).toHaveBeenCalledWith({ ...makeHandoverSettings(), automatic: true });
  });

  it("saves the tool's own default", async () => {
    const onChange = show();
    await userEvent.click(within(codex()).getByRole("radio", { name: "Codex's own configured default" }));
    expect(onChange).toHaveBeenCalledWith({ ...makeHandoverSettings(), claudeToCodex: { kind: "tool-default" } });
  });

  it("saves Claude Code's own default in its own direction only", async () => {
    const onChange = show({ ...makeHandoverSettings(), claudeToCodex: { kind: "tool-default" } });
    await userEvent.click(within(claude()).getByRole("radio", { name: "Claude Code's own configured default" }));
    expect(onChange).toHaveBeenCalledWith({ ...makeHandoverSettings(), claudeToCodex: { kind: "tool-default" }, codexToClaude: { kind: "tool-default" } });
  });

  it("saves a Claude Code model only with an effort, from Claude Code's list", async () => {
    const user = userEvent.setup();
    const onChange = show();
    await user.click(within(claude()).getByRole("radio", { name: "A model I choose" }));
    await screen.findByRole("option", { name: "Opus 5.5" });
    expect(screen.queryByRole("option", { name: "GPT-5.5" })).not.toBeInTheDocument();
    await user.selectOptions(screen.getByTestId("settings-handover-claude-code-model"), "claude-opus-5-5");
    expect(onChange).not.toHaveBeenCalled();
    await user.selectOptions(screen.getByTestId("settings-handover-claude-code-effort"), "high");
    expect(onChange).toHaveBeenCalledWith({ ...makeHandoverSettings(), codexToClaude: { kind: "chosen", model: "claude-opus-5-5", effort: "high" } });
  });

  it("shows a saved choice in each direction", async () => {
    show({ ...makeHandoverSettings(), claudeToCodex: { kind: "tool-default" }, codexToClaude: { kind: "chosen", model: "claude-opus-5-5", effort: "high" } });
    expect(within(codex()).getByRole("radio", { name: "Codex's own configured default" })).toBeChecked();
    expect(within(claude()).getByRole("radio", { name: "A model I choose" })).toBeChecked();
    expect(await screen.findByRole("option", { name: "Opus 5.5" })).toBeInTheDocument();
    expect(screen.getByTestId("settings-handover-claude-code-model")).toHaveValue("claude-opus-5-5");
  });

  it("saves a model only once an effort is picked too, and clears the effort with a new model", async () => {
    const user = userEvent.setup();
    const onChange = show();
    await user.click(within(codex()).getByRole("radio", { name: "A model I choose" }));
    expect(onChange).not.toHaveBeenCalled();
    await screen.findByRole("option", { name: "GPT-5.5" });
    await user.selectOptions(screen.getByTestId("settings-handover-codex-model"), "gpt-5.5");
    expect(onChange).not.toHaveBeenCalled();
    await user.selectOptions(screen.getByTestId("settings-handover-codex-effort"), "high");
    expect(onChange).toHaveBeenCalledTimes(1);
    expect(onChange).toHaveBeenCalledWith({ ...makeHandoverSettings(), claudeToCodex: { kind: "chosen", model: "gpt-5.5", effort: "high" } });
    await user.selectOptions(screen.getByTestId("settings-handover-codex-model"), "gpt-5.6-luna");
    expect(screen.getByTestId("settings-handover-codex-effort")).toHaveValue("");
    expect(onChange).toHaveBeenCalledTimes(1);
  });

  it("shows a saved choice, even a model Codex no longer lists", async () => {
    show({ ...makeHandoverSettings(), claudeToCodex: { kind: "chosen", model: "gpt-old", effort: "low" } });
    expect(within(codex()).getByRole("radio", { name: "A model I choose" })).toBeChecked();
    expect(await screen.findByRole("option", { name: "gpt-old" })).toBeInTheDocument();
    expect(screen.getByTestId("settings-handover-codex-model")).toHaveValue("gpt-old");
  });

  it("says so when Codex is not found", async () => {
    show(makeHandoverSettings(), () => Promise.resolve(makeHireOptions().map((o) => (o.tool === "codex" ? { ...o, available: false } : o))));
    expect(await screen.findByText("Codex was not found on this Mac, so no desk can be handed to it.")).toBeInTheDocument();
  });

  it("says so when Claude Code is not found", async () => {
    show(makeHandoverSettings(), () => Promise.resolve(makeHireOptions().map((o) => (o.tool === "claude-code" ? { ...o, available: false } : o))));
    expect(await screen.findByText("Claude Code was not found on this Mac, so no desk can be handed to it.")).toBeInTheDocument();
  });
});
