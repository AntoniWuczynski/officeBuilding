import { describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
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

describe("HandoverGroup", () => {
  it("preselects nothing but Not chosen, and says Codex to Claude Code is not built", () => {
    show();
    expect(screen.getByRole("radio", { name: "Not chosen" })).toBeChecked();
    expect(screen.getByRole("radio", { name: "Codex's own configured default" })).not.toBeChecked();
    expect(screen.getByRole("radio", { name: "A model I choose" })).not.toBeChecked();
    expect(screen.queryByTestId("settings-handover-codex-model")).not.toBeInTheDocument();
    expect(screen.getByTestId("settings-handover-codex-to-claude")).toHaveTextContent("Codex to Claude Code: not built yet. A Codex desk is never handed over in this version.");
    expect(screen.getByText(/Nothing is handed over until you have chosen what Codex starts with\./)).toBeInTheDocument();
    expect(screen.getByText(/including any secret a tool printed/)).toBeInTheDocument();
  });

  it("saves the switch", async () => {
    const onChange = show();
    await userEvent.click(screen.getByRole("switch", { name: "Hand desks over by themselves" }));
    expect(onChange).toHaveBeenCalledWith({ ...makeHandoverSettings(), automatic: true });
  });

  it("saves the tool's own default", async () => {
    const onChange = show();
    await userEvent.click(screen.getByRole("radio", { name: "Codex's own configured default" }));
    expect(onChange).toHaveBeenCalledWith({ ...makeHandoverSettings(), claudeToCodex: { kind: "tool-default" } });
  });

  it("saves a model only once an effort is picked too, and clears the effort with a new model", async () => {
    const user = userEvent.setup();
    const onChange = show();
    await user.click(screen.getByRole("radio", { name: "A model I choose" }));
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
    expect(screen.getByRole("radio", { name: "A model I choose" })).toBeChecked();
    expect(await screen.findByRole("option", { name: "gpt-old" })).toBeInTheDocument();
    expect(screen.getByTestId("settings-handover-codex-model")).toHaveValue("gpt-old");
  });

  it("says so when Codex is not found", async () => {
    show(makeHandoverSettings(), () => Promise.resolve(makeHireOptions().map((o) => (o.tool === "codex" ? { ...o, available: false } : o))));
    expect(await screen.findByText("Codex was not found on this Mac, so no desk can be handed to it.")).toBeInTheDocument();
  });
});
