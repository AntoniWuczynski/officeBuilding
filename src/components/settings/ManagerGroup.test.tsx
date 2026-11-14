import { describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { makeHireOptions } from "../../api/mockHireOptions";
import { makeManagerSettings } from "../../api/mockManager";
import type { ManagerSettings, ToolOptions } from "../../types";
import { ManagerGroup } from "./ManagerGroup";

function show(value: ManagerSettings = makeManagerSettings(), load: () => Promise<readonly ToolOptions[]> = () => Promise.resolve(makeHireOptions())): ReturnType<typeof vi.fn> {
  const onChange = vi.fn();
  render(<ManagerGroup value={value} loadOptions={load} onChange={onChange} />);
  return onChange;
}

describe("ManagerGroup", () => {
  it("shows the saved values and the hint", async () => {
    show({ ...makeManagerSettings(), maxDesks: 6, spendCeilingUsd: 50 });
    expect(await screen.findByRole("option", { name: "Claude Code's default" })).toBeInTheDocument();
    expect(screen.getByTestId("settings-manager-tool")).toHaveValue("claude-code");
    expect(screen.getByTestId("settings-manager-max-desks")).toHaveValue(6);
    expect(screen.getByTestId("settings-manager-ceiling")).toHaveValue(50);
    expect(screen.getByText(/Model and effort apply to the next manager you hire/)).toBeInTheDocument();
  });

  it("keeps the one tool", async () => {
    const user = userEvent.setup();
    const onChange = show();
    await user.selectOptions(screen.getByTestId("settings-manager-tool"), "claude-code");
    expect(onChange).toHaveBeenCalledWith(makeManagerSettings());
  });

  it.each([
    ["settings-manager-max-desks", "maxDesks", "11", "Between 1 and 10.", "10"],
    ["settings-manager-ceiling", "spendCeilingUsd", "1001", "Between 1 and 1000 dollars.", "1000"],
  ])("%s refuses %s out of range and saves one in range", async (id, field, bad, error, good) => {
    const user = userEvent.setup();
    const onChange = show();
    const box = screen.getByTestId(id);
    await user.clear(box);
    await user.type(box, bad);
    await user.tab();
    expect(screen.getByText(error)).toBeInTheDocument();
    expect(onChange).not.toHaveBeenCalled();
    await user.clear(box);
    await user.type(box, good);
    await user.tab();
    expect(onChange).toHaveBeenCalledWith({ ...makeManagerSettings(), [field]: Number(good) });
  });

  it("changes the model, which clears the effort, and then the effort", async () => {
    const user = userEvent.setup();
    const first = makeHireOptions().find((o) => o.tool === "claude-code")?.models[0];
    if (first === undefined) throw new Error("no model");
    const onChange = show({ ...makeManagerSettings(), effort: "low" });
    await screen.findByRole("option", { name: first.label });
    await user.selectOptions(screen.getByTestId("settings-manager-model"), first.id);
    expect(onChange).toHaveBeenLastCalledWith({ ...makeManagerSettings(), model: first.id, effort: null });
    await user.selectOptions(screen.getByTestId("settings-manager-effort"), "");
    expect(onChange).toHaveBeenLastCalledWith({ ...makeManagerSettings(), effort: null });
  });

  it("says so when Claude Code was not found", async () => {
    show(makeManagerSettings(), () => Promise.resolve([]));
    expect(await screen.findByText("Claude Code was not found on this Mac, so no manager can be hired.")).toBeInTheDocument();
    expect(screen.getByTestId("settings-manager-model")).toBeDisabled();
  });
});
