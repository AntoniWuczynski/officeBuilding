import { describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { HireDialog } from "./HireDialog";
import { makeHireOptions, withDefaultView } from "../../api/mockHireOptions";
import type { HireRequest } from "../../api/DiscoveryApi";
import type { TakeOverStart, ToolOptions } from "../../types";

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

describe("HireDialog in take-over mode", () => {
  const ONE_EFFORT: readonly ToolOptions[] = makeHireOptions().map((o) =>
    o.tool === "codex" ? { ...o, models: [...o.models, { id: "solo", label: "Solo", efforts: [{ id: "only", label: "Only", description: "" }], defaultEffort: "only" }] } : o,
  );

  function openTakeOver(options: readonly ToolOptions[] = ONE_EFFORT): { onTakeOver: ReturnType<typeof vi.fn<(s: TakeOverStart) => Promise<void>>> } {
    const onTakeOver = vi.fn<(s: TakeOverStart) => Promise<void>>().mockResolvedValue(undefined);
    render(<HireDialog takeOver={{ title: "fix the build" }} loadOptions={() => Promise.resolve(options)} onTakeOver={onTakeOver} onCancel={() => {}} />);
    return { onTakeOver };
  }

  it("names the session and lists only Claude Code and Codex, with nothing chosen", async () => {
    openTakeOver();
    expect(await screen.findByRole("heading")).toHaveTextContent("Take over “fix the build” with a new session");
    expect(screen.getAllByRole("radio").map((r) => r.closest("label")?.textContent)).toEqual(["Claude Code", "Codex"]);
    for (const r of screen.getAllByRole("radio")) expect(r).not.toBeChecked();
    expect(screen.queryByTestId("hire-model")).not.toBeInTheDocument();
    expect(screen.getByTestId("hire-submit")).toBeDisabled();
    expect(screen.getByTestId("hire-submit")).toHaveTextContent("Start and take over");
  });

  it("offers a model on Choose a model, then efforts with none checked", async () => {
    const user = userEvent.setup();
    openTakeOver();
    await user.click(await screen.findByLabelText("Claude Code"));
    const model = screen.getByTestId("hire-model");
    expect(model).toHaveValue("");
    expect(screen.getByRole("option", { name: "Choose a model" })).toBeDisabled();
    expect(screen.queryByText("Effort")).not.toBeInTheDocument();
    await user.selectOptions(model, "claude-opus-5-5");
    expect(screen.getByText("Effort")).toBeInTheDocument();
    for (const r of screen.getAllByRole("radio", { name: /Low|Medium|High|Extra high|Max/ })) expect(r).not.toBeChecked();
    expect(screen.getByTestId("hire-submit")).toBeDisabled();
    await user.click(screen.getByLabelText("High"));
    expect(screen.getByTestId("hire-submit")).toBeEnabled();
  });

  it("takes the only effort of a model that offers one", async () => {
    const user = userEvent.setup();
    const { onTakeOver } = openTakeOver();
    await user.click(await screen.findByLabelText("Codex"));
    await user.selectOptions(screen.getByTestId("hire-model"), "solo");
    expect(screen.getByLabelText("Only")).toBeChecked();
    await user.click(screen.getByTestId("hire-submit"));
    expect(onTakeOver).toHaveBeenCalledWith({ tool: "codex", model: "solo", effort: "only" });
  });

  it("clears the model and the effort when another tool is chosen", async () => {
    const user = userEvent.setup();
    openTakeOver();
    await user.click(await screen.findByLabelText("Claude Code"));
    await user.selectOptions(screen.getByTestId("hire-model"), "claude-opus-5-5");
    await user.click(screen.getByLabelText("High"));
    await user.click(screen.getByLabelText("Codex"));
    expect(screen.getByTestId("hire-model")).toHaveValue("");
    expect(screen.queryByText("Effort")).not.toBeInTheDocument();
    expect(screen.getByTestId("hire-submit")).toBeDisabled();
  });

  it("keeps the effort when the new model offers it, and clears it otherwise", async () => {
    const user = userEvent.setup();
    openTakeOver();
    await user.click(await screen.findByLabelText("Codex"));
    await user.selectOptions(screen.getByTestId("hire-model"), "gpt-6-astra");
    await user.click(screen.getByLabelText("Low"));
    await user.selectOptions(screen.getByTestId("hire-model"), "solo");
    expect(screen.getByLabelText("Only")).toBeChecked();
    await user.selectOptions(screen.getByTestId("hire-model"), "gpt-6-astra");
    expect(screen.getByLabelText("Low")).not.toBeChecked();
    expect(screen.getByTestId("hire-submit")).toBeDisabled();
  });

  it("has no Task field, Ultracode switch or Chat view switch", async () => {
    const user = userEvent.setup();
    openTakeOver();
    await user.click(await screen.findByLabelText("Claude Code"));
    await user.selectOptions(screen.getByTestId("hire-model"), "claude-opus-5-5");
    expect(screen.queryByText("Task")).not.toBeInTheDocument();
    expect(screen.queryByRole("switch")).not.toBeInTheDocument();
    expect(screen.queryByTestId("hire-view-hint")).not.toBeInTheDocument();
  });

  it("shows the confirmation from the start and names the tool once chosen", async () => {
    const user = userEvent.setup();
    openTakeOver();
    expect(await screen.findByTestId("take-over-confirm")).toHaveTextContent("so they go to the provider of the tool you choose.");
    await user.click(screen.getByLabelText("Codex"));
    expect(screen.getByTestId("take-over-confirm")).toHaveTextContent("so they go to Codex's provider.");
  });

  it("submits exactly the tool, model and effort", async () => {
    const user = userEvent.setup();
    const { onTakeOver } = openTakeOver();
    await user.click(await screen.findByLabelText("Claude Code"));
    await user.selectOptions(screen.getByTestId("hire-model"), "claude-fable-5-1");
    await user.click(screen.getByLabelText("Max"));
    await user.click(screen.getByTestId("hire-submit"));
    expect(onTakeOver).toHaveBeenCalledWith({ tool: "claude-code", model: "claude-fable-5-1", effort: "max" });
  });

  it("shows a refusal and keeps the choices", async () => {
    const user = userEvent.setup();
    const onTakeOver = vi.fn<(s: TakeOverStart) => Promise<void>>().mockRejectedValue(new Error("Codex is not signed in"));
    render(<HireDialog takeOver={{ title: "t" }} loadOptions={() => Promise.resolve(makeHireOptions())} onTakeOver={onTakeOver} onCancel={() => {}} />);
    await user.click(await screen.findByLabelText("Claude Code"));
    await user.selectOptions(screen.getByTestId("hire-model"), "claude-opus-5-5");
    await user.click(screen.getByLabelText("High"));
    await user.click(screen.getByTestId("hire-submit"));
    expect(await screen.findByRole("alert")).toHaveTextContent("Could not take over: Codex is not signed in.");
    expect(screen.getByLabelText("High")).toBeChecked();
    expect(screen.getByTestId("hire-model")).toHaveValue("claude-opus-5-5");
    expect(screen.getByTestId("hire-submit")).toBeEnabled();
  });

  it("greys out a tool that is not installed", async () => {
    openTakeOver(makeHireOptions().map((o) => (o.tool === "codex" ? { ...o, available: false } : o)));
    expect(await screen.findByLabelText(/Codex/)).toBeDisabled();
    expect(screen.getByText("Not installed")).toBeInTheDocument();
  });

  it("offers a retry when the tools cannot be loaded", async () => {
    const user = userEvent.setup();
    const load = vi.fn<() => Promise<readonly ToolOptions[]>>().mockRejectedValueOnce(new Error("offline")).mockResolvedValue(makeHireOptions());
    render(<HireDialog takeOver={{ title: "t" }} loadOptions={load} onTakeOver={() => Promise.resolve()} onCancel={() => {}} />);
    expect(await screen.findByRole("alert")).toHaveTextContent("Could not load the tools: offline.");
    await user.click(screen.getByRole("button", { name: "Try again" }));
    expect(await screen.findByLabelText("Codex")).toBeEnabled();
  });
});
