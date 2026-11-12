import { describe, expect, it, vi } from "vitest";
import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { makeHireOptions } from "../../api/mockHireOptions";
import { makeQaSettings } from "../../api/mockQa";
import type { QaSettings, ToolOptions } from "../../types";
import { QaGroup } from "./QaGroup";

function show(value: QaSettings = makeQaSettings(), load: () => Promise<readonly ToolOptions[]> = () => Promise.resolve(makeHireOptions())): ReturnType<typeof vi.fn> {
  const onChange = vi.fn();
  render(<QaGroup value={value} loadOptions={load} onChange={onChange} />);
  return onChange;
}

const claude = (): ToolOptions => {
  const found = makeHireOptions().find((o) => o.tool === "claude-code");
  if (found === undefined) throw new Error("the demo catalogue has no Claude Code");
  return found;
};

describe("QaGroup", () => {
  it("shows the saved values", async () => {
    show({ ...makeQaSettings(), defaultOn: true, pollMinutes: 12, maxConcurrent: 2, reviewsPerDay: 7 });
    expect(await screen.findByRole("option", { name: "Claude Code's default" })).toBeInTheDocument();
    expect(within(screen.getByTestId("settings-qa-default")).getByRole("switch")).toBeChecked();
    expect(screen.getByTestId("settings-qa-poll")).toHaveValue(12);
    expect(screen.getByTestId("settings-qa-concurrent")).toHaveValue(2);
    expect(screen.getByTestId("settings-qa-per-day")).toHaveValue(7);
    expect(screen.getByTestId("settings-qa-tool")).toHaveValue("claude-code");
    expect(screen.getByText(/switched on from its Info panel/)).toBeInTheDocument();
  });

  it.each([
    ["settings-qa-default", "defaultOn", true],
    ["settings-qa-every-push", "everyPush", false],
    ["settings-qa-skip-drafts", "skipDrafts", false],
    ["settings-qa-post-clean", "postWhenClean", false],
  ])("%s changes only %s", async (id, field, next) => {
    const user = userEvent.setup();
    const onChange = show();
    await user.click(within(screen.getByTestId(id)).getByRole("switch"));
    expect(onChange).toHaveBeenCalledWith({ ...makeQaSettings(), [field]: next });
  });

  it("explains first-push-only when every push is off", () => {
    show({ ...makeQaSettings(), everyPush: false });
    expect(screen.getByText("Only the first commit of each pull request is reviewed.")).toBeInTheDocument();
  });

  it.each([
    ["settings-qa-poll", "pollMinutes", "61", "Between 1 and 60 minutes.", "60"],
    ["settings-qa-concurrent", "maxConcurrent", "5", "Between 1 and 4.", "4"],
    ["settings-qa-per-day", "reviewsPerDay", "0", "Between 1 and 20.", "20"],
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
    await user.type(box, `${good}{Enter}`);
    expect(onChange).toHaveBeenCalledWith({ ...makeQaSettings(), [field]: Number(good) });
    expect(screen.queryByText(error)).not.toBeInTheDocument();
  });

  it("offers the Claude Code models and clears the effort when one is chosen", async () => {
    const user = userEvent.setup();
    const first = claude().models[0];
    if (first === undefined) throw new Error("no model");
    const onChange = show({ ...makeQaSettings(), effort: "low" });
    await screen.findByRole("option", { name: first.label });
    await user.selectOptions(screen.getByTestId("settings-qa-model"), first.id);
    expect(onChange).toHaveBeenCalledWith({ ...makeQaSettings(), model: first.id, effort: null });
  });

  it("offers the efforts of the chosen model and can go back to the defaults", async () => {
    const user = userEvent.setup();
    const model = claude().models[0];
    if (model === undefined) throw new Error("no model");
    const onChange = show({ ...makeQaSettings(), model: model.id });
    await screen.findByRole("option", { name: "High" });
    await user.selectOptions(screen.getByTestId("settings-qa-effort"), "high");
    expect(onChange).toHaveBeenLastCalledWith({ ...makeQaSettings(), model: model.id, effort: "high" });
    await user.selectOptions(screen.getByTestId("settings-qa-model"), "");
    expect(onChange).toHaveBeenLastCalledWith({ ...makeQaSettings(), model: null, effort: null });
  });

  it("clears the effort to the model's default", async () => {
    const user = userEvent.setup();
    const onChange = show({ ...makeQaSettings(), effort: "high" });
    await screen.findByRole("option", { name: "High" });
    await user.selectOptions(screen.getByTestId("settings-qa-effort"), "");
    expect(onChange).toHaveBeenCalledWith({ ...makeQaSettings(), effort: null });
  });

  it("keeps the one reviewer tool", async () => {
    const user = userEvent.setup();
    const onChange = show();
    await user.selectOptions(screen.getByTestId("settings-qa-tool"), "claude-code");
    expect(onChange).toHaveBeenCalledWith(makeQaSettings());
  });

  it("disables the selects while the options load", () => {
    show(makeQaSettings(), () => new Promise(() => undefined));
    expect(screen.getByTestId("settings-qa-model")).toBeDisabled();
    expect(screen.getByTestId("settings-qa-effort")).toBeDisabled();
    expect(screen.getAllByRole("option", { name: "Looking for models…" })).toHaveLength(2);
  });

  it("says so when the options cannot be read", async () => {
    show(makeQaSettings(), () => Promise.reject(new Error("no")));
    expect(await screen.findByText("Claude Code was not found on this Mac, so no review can start.")).toBeInTheDocument();
    expect(screen.getByTestId("settings-qa-model")).toBeDisabled();
  });

  it("says so when Claude Code is not available", async () => {
    show(makeQaSettings(), () => Promise.resolve(makeHireOptions().map((o) => (o.tool === "claude-code" ? { ...o, available: false } : o))));
    expect(await screen.findByText(/Claude Code was not found/)).toBeInTheDocument();
    expect(screen.getByTestId("settings-qa-effort")).toBeDisabled();
  });

  it("says so when the catalogue has no Claude Code", async () => {
    show(makeQaSettings(), () => Promise.resolve(makeHireOptions().filter((o) => o.tool !== "claude-code")));
    expect(await screen.findByText(/Claude Code was not found/)).toBeInTheDocument();
  });

  it("still shows a saved model and effort that are not listed", async () => {
    show({ ...makeQaSettings(), model: "old-model", effort: "odd" });
    expect(await screen.findByRole("option", { name: "old-model" })).toBeInTheDocument();
    expect(screen.getByRole("option", { name: "odd" })).toBeInTheDocument();
    expect(screen.getByTestId("settings-qa-model")).toHaveValue("old-model");
  });
});
