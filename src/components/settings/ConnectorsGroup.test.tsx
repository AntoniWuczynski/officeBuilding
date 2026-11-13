import { describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { LinearKeyStatus } from "../../types";
import { ConnectorsContext } from "../connectors/ConnectorsContext";
import type { ConnectorActions } from "../connectors/ConnectorsContext";
import { ConnectorsGroup } from "./ConnectorsGroup";

function actionsFor(status: LinearKeyStatus, over: Partial<ConnectorActions> = {}): ConnectorActions {
  return {
    keyStatus: () => Promise.resolve(status),
    setKey: vi.fn(() => Promise.resolve<LinearKeyStatus>({ kind: "saved", account: "Demo User" })),
    teams: vi.fn(),
    connect: vi.fn(),
    disconnect: vi.fn(),
    recheck: vi.fn(),
    openLink: vi.fn(),
    openSettings: vi.fn(),
    ...over,
  };
}

function show(actions: ConnectorActions | null, onChange = vi.fn()): typeof onChange {
  const group = <ConnectorsGroup value={{ linearAgentLabel: "agent" }} onChange={onChange} />;
  render(actions === null ? group : <ConnectorsContext value={actions}>{group}</ConnectorsContext>);
  return onChange;
}

describe("ConnectorsGroup key", () => {
  it.each<[LinearKeyStatus, string]>([
    [{ kind: "missing" }, "No Linear API key is saved."],
    [{ kind: "saved", account: "Demo User" }, "A Linear API key is saved in your Keychain. It belongs to Demo User."],
    [{ kind: "saved", account: null }, "A Linear API key is saved in your Keychain."],
    [{ kind: "rejected" }, "Linear did not accept the saved key. Paste a new one."],
    [{ kind: "unavailable", reason: "locked" }, "The Keychain did not hand over the key: locked."],
  ])("says %j", async (status, sentence) => {
    show(actionsFor(status));
    expect(await screen.findByTestId("linear-key-status")).toHaveTextContent(sentence);
  });

  it("shows a failed read", async () => {
    show(actionsFor({ kind: "missing" }, { keyStatus: () => Promise.reject(new Error("boom")) }));
    expect(await screen.findByText("Could not read the key’s status: boom.")).toBeInTheDocument();
  });

  it("clears the input at once on Save, then shows the new status", async () => {
    let finish: (s: LinearKeyStatus) => void = () => {};
    const setKey = vi.fn(() => new Promise<LinearKeyStatus>((resolve) => (finish = resolve)));
    show(actionsFor({ kind: "missing" }, { setKey }));
    const input = await screen.findByTestId("linear-key");
    await userEvent.type(input, "test-key");
    await userEvent.click(screen.getByTestId("linear-key-save"));
    expect(input).toHaveValue("");
    expect(setKey).toHaveBeenCalledWith("test-key");
    expect(screen.getByTestId("linear-key-status")).toHaveTextContent("Checking the key with Linear…");
    finish({ kind: "saved", account: "Demo User" });
    expect(await screen.findByText(/It belongs to Demo User/)).toBeInTheDocument();
  });

  it("shows a refused key and keeps the old status", async () => {
    show(actionsFor({ kind: "missing" }, { setKey: () => Promise.reject(new Error("Linear did not accept that key")) }));
    await userEvent.type(await screen.findByTestId("linear-key"), "test-key");
    await userEvent.click(screen.getByTestId("linear-key-save"));
    expect(await screen.findByText("Could not save the key: Linear did not accept that key.")).toBeInTheDocument();
    expect(screen.getByTestId("linear-key-status")).toHaveTextContent("No Linear API key is saved.");
  });

  it("keeps Save off for a blank key, saves on Enter and removes the key", async () => {
    const actions = actionsFor({ kind: "saved", account: null });
    show(actions);
    const input = await screen.findByTestId("linear-key");
    expect(screen.getByTestId("linear-key-save")).toBeDisabled();
    await userEvent.type(input, "test-key{Enter}");
    expect(actions.setKey).toHaveBeenCalledWith("test-key");
    await userEvent.click(await screen.findByTestId("linear-key-remove"));
    expect(actions.setKey).toHaveBeenLastCalledWith(null);
  });

  it("offers Remove key only for a saved or refused key", async () => {
    show(actionsFor({ kind: "missing" }));
    await screen.findByTestId("linear-key");
    expect(screen.queryByTestId("linear-key-remove")).not.toBeInTheDocument();
  });
});

describe("ConnectorsGroup agent label", () => {
  it("commits on blur and on Enter", async () => {
    const onChange = show(null);
    const input = screen.getByTestId("linear-agent-label");
    await userEvent.clear(input);
    await userEvent.type(input, " bot ");
    fireEvent.blur(input);
    expect(onChange).toHaveBeenCalledWith({ linearAgentLabel: "bot" });
    await userEvent.type(input, "{Enter}");
    expect(onChange).toHaveBeenCalledTimes(2);
  });

  it("does not send an unchanged label", () => {
    const onChange = show(null);
    fireEvent.blur(screen.getByTestId("linear-agent-label"));
    expect(onChange).not.toHaveBeenCalled();
  });

  it.each([["   "], ["x".repeat(81)]])("refuses %j", async (text) => {
    const onChange = show(null);
    const input = screen.getByTestId("linear-agent-label");
    await userEvent.clear(input);
    await userEvent.type(input, text);
    fireEvent.blur(input);
    expect(screen.getByText("Enter a label of 1 to 80 characters.")).toBeInTheDocument();
    expect(onChange).not.toHaveBeenCalled();
  });

  it("shows only the agent label without a provider", () => {
    show(null);
    expect(screen.queryByTestId("linear-key")).not.toBeInTheDocument();
    expect(screen.getByTestId("linear-agent-label")).toBeInTheDocument();
  });
});
