import { describe, expect, it, vi } from "vitest";
import { act, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MockApi } from "../../api/mockApi";
import type { EditorList, Settings, SettingsPatch } from "../../types";
import { SettingsPanel } from "./SettingsPanel";

interface Opened {
  readonly api: MockApi;
  readonly onClose: () => void;
}

function open(
  overrides: { load?: () => Promise<Settings>; save?: (p: SettingsPatch) => Promise<Settings>; editors?: EditorList | null } = {},
): Opened {
  const api = new MockApi();
  const onClose = vi.fn();
  render(
    <SettingsPanel
      load={overrides.load ?? (() => api.getSettings())}
      save={overrides.save ?? ((p) => api.updateSettings(p))}
      editors={overrides.editors === undefined ? LIST : overrides.editors}
      loadHireOptions={() => api.hireOptions()}
      onTestAlert={() => api.testAlert()}
      onClose={onClose}
    />,
  );
  return { api, onClose };
}

const LIST: EditorList = {
  editors: [
    { editor: "cursor", label: "Cursor", installed: true },
    { editor: "vs-code", label: "VS Code", installed: true },
    { editor: "zed", label: "Zed", installed: false },
    { editor: "devin-desktop", label: "Devin Desktop", installed: false },
  ],
  defaultEditor: "cursor",
};

const NAMES = ["Finishes its turn", "Needs you", "Is stuck on an error", "Sound", "Dock bounce", "System notification"];

describe("SettingsPanel", () => {
  it("shows the six switches on and the cooldown at 10", async () => {
    open();
    for (const name of NAMES) expect(await screen.findByRole("switch", { name })).toBeChecked();
    expect(screen.getByTestId("settings-cooldown")).toHaveValue(10);
  });

  it("saves a switch at once", async () => {
    const user = userEvent.setup();
    const { api } = open();
    await user.click(await screen.findByRole("switch", { name: "Sound" }));
    expect(screen.getByRole("switch", { name: "Sound" })).not.toBeChecked();
    expect((await api.getSettings()).notifications.sound).toBe(false);
  });

  it("saves a cooldown on blur and on Enter", async () => {
    const user = userEvent.setup();
    const { api } = open();
    const box = await screen.findByTestId("settings-cooldown");
    await user.clear(box);
    await user.type(box, "30");
    await user.tab();
    expect((await api.getSettings()).notifications.cooldownSecs).toBe(30);
    await user.clear(box);
    await user.type(box, "45{Enter}");
    expect((await api.getSettings()).notifications.cooldownSecs).toBe(45);
  });

  it.each(["700", "1.5", ""])("refuses a cooldown of %j and saves nothing", async (text) => {
    const user = userEvent.setup();
    const { api } = open();
    const box = await screen.findByTestId("settings-cooldown");
    await user.clear(box);
    if (text !== "") await user.type(box, text);
    await user.tab();
    expect(screen.getByText("Enter a whole number of seconds from 0 to 600.")).toBeInTheDocument();
    expect((await api.getSettings()).notifications.cooldownSecs).toBe(10);
  });

  it("shows a failed save and puts the switch back", async () => {
    const user = userEvent.setup();
    open({ save: () => Promise.reject(new Error("disk full")) });
    await user.click(await screen.findByRole("switch", { name: "Needs you" }));
    expect(await screen.findByText(/Could not save that: disk full/)).toBeInTheDocument();
    expect(screen.getByRole("switch", { name: "Needs you" })).toBeChecked();
  });

  it("shows a failed load and recovers on Try again", async () => {
    const user = userEvent.setup();
    const api = new MockApi();
    let fail = true;
    open({ load: () => (fail ? Promise.reject(new Error("no file")) : api.getSettings()) });
    expect(await screen.findByText(/Could not read the settings: no file/)).toBeInTheDocument();
    fail = false;
    await user.click(screen.getByRole("button", { name: "Try again" }));
    expect(await screen.findByRole("switch", { name: "Sound" })).toBeChecked();
  });

  it("sends a test alert once and disables the button for three seconds", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    try {
      const user = userEvent.setup({ advanceTimers: vi.advanceTimersByTime });
      const { api } = open();
      const button = await screen.findByTestId("settings-test");
      await user.click(button);
      expect(api.testAlerts).toBe(1);
      expect(button).toBeDisabled();
      expect(screen.getByText(/Switch to another app now/)).toBeInTheDocument();
      await act(() => vi.advanceTimersByTimeAsync(3100));
      expect(button).toBeEnabled();
    } finally {
      vi.useRealTimers();
    }
  });

  describe("Editor group", () => {
    it("lists the editors and disables the ones that are not installed", async () => {
      open();
      const select = await screen.findByTestId("settings-editor");
      const options = within(select).getAllByRole("option");
      expect(options.map((o) => o.textContent)).toEqual(["First one found", "Cursor", "VS Code", "Zed (not installed)", "Devin Desktop (not installed)"]);
      expect(options.map((o) => o instanceof HTMLOptionElement && o.disabled)).toEqual([false, false, false, true, true]);
      expect(select).toHaveValue("");
      expect(screen.getByText(/opens its code in Cursor\. The arrow beside it offers the other editors found on this Mac\./)).toBeInTheDocument();
    });

    it("saves the chosen editor, and null for First one found", async () => {
      const user = userEvent.setup();
      const { api } = open();
      const select = await screen.findByTestId("settings-editor");
      await user.selectOptions(select, "VS Code");
      expect((await api.getSettings()).editor).toEqual({ preferred: "vs-code" });
      expect(select).toHaveValue("vs-code");
      await user.selectOptions(select, "First one found");
      expect((await api.getSettings()).editor).toEqual({ preferred: null });
    });

    it("keeps a saved editor that is not installed selectable and says it is skipped", async () => {
      const api = new MockApi();
      await api.updateSettings({ editor: { preferred: "zed" } });
      open({ load: () => api.getSettings() });
      const select = await screen.findByTestId("settings-editor");
      expect(select).toHaveValue("zed");
      expect(screen.getByText("Zed is not installed, so Cursor is used until it is.")).toBeInTheDocument();
    });

    it("says when no editor was found", async () => {
      open({ editors: { editors: LIST.editors.map((o) => ({ ...o, installed: false })), defaultEditor: null } });
      expect(await screen.findByText("No editor was found on this Mac. The building looks for Cursor, VS Code, Zed and Devin Desktop.")).toBeInTheDocument();
    });

    it("is disabled until the list has loaded", async () => {
      open({ editors: null });
      const select = await screen.findByTestId("settings-editor");
      expect(select).toBeDisabled();
      expect(within(select).getByRole("option", { name: "Looking for editors…" })).toBeInTheDocument();
    });
  });

  it("closes on Escape and on Done", async () => {
    const user = userEvent.setup();
    const { onClose } = open();
    await user.click(await screen.findByTestId("settings-close"));
    expect(onClose).toHaveBeenCalledTimes(1);
    await user.keyboard("{Escape}");
    expect(onClose).toHaveBeenCalledTimes(2);
  });

  it("shows the Quality assurance group once the settings have loaded", async () => {
    open();
    expect(await screen.findByRole("heading", { name: "Quality assurance" })).toBeInTheDocument();
  });
});
