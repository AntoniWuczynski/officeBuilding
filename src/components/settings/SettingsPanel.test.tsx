import { describe, expect, it, vi } from "vitest";
import { act, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MockApi } from "../../api/mockApi";
import type { Settings, SettingsPatch } from "../../types";
import { SettingsPanel } from "./SettingsPanel";

interface Opened {
  readonly api: MockApi;
  readonly onClose: () => void;
}

function open(overrides: { load?: () => Promise<Settings>; save?: (p: SettingsPatch) => Promise<Settings> } = {}): Opened {
  const api = new MockApi();
  const onClose = vi.fn();
  render(
    <SettingsPanel
      load={overrides.load ?? (() => api.getSettings())}
      save={overrides.save ?? ((p) => api.updateSettings(p))}
      onTestAlert={() => api.testAlert()}
      onClose={onClose}
    />,
  );
  return { api, onClose };
}

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

  it("closes on Escape and on Done", async () => {
    const user = userEvent.setup();
    const { onClose } = open();
    await user.click(await screen.findByTestId("settings-close"));
    expect(onClose).toHaveBeenCalledTimes(1);
    await user.keyboard("{Escape}");
    expect(onClose).toHaveBeenCalledTimes(2);
  });
});
