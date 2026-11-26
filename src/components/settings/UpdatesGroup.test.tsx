import { beforeEach, describe, expect, it, vi } from "vitest";
import { act, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { AppInfo } from "../../types";
import type { AvailableUpdate, DownloadProgress } from "../../lib/updater";
import { resetUpdate } from "../../state/useUpdate";
import { UpdatesGroup } from "./UpdatesGroup";

const mocks = vi.hoisted(() => ({ checkForUpdate: vi.fn(), relaunchApp: vi.fn() }));
vi.mock("../../lib/updater", () => mocks);

const INFO: AppInfo = { version: "0.2.0", commit: "abc1234def5678" };

function update(install: AvailableUpdate["install"], notes: string | null = "Fixes the desks."): AvailableUpdate {
  return { version: "0.3.0", notes, install };
}

function open(): void {
  render(<UpdatesGroup loadInfo={() => Promise.resolve(INFO)} />);
}

beforeEach(() => {
  resetUpdate();
  mocks.checkForUpdate.mockReset();
  mocks.relaunchApp.mockReset();
});

describe("UpdatesGroup", () => {
  it("shows the version and the short commit", async () => {
    open();
    expect(await screen.findByText("Version 0.2.0 (abc1234)")).toBeInTheDocument();
  });

  it("says when the version cannot be read", async () => {
    render(<UpdatesGroup loadInfo={() => Promise.reject(new Error("no command"))} />);
    expect(await screen.findByText("The version could not be read: no command.")).toBeInTheDocument();
  });

  it("shows checking, then that the latest version is installed", async () => {
    const user = userEvent.setup();
    let finish: (value: AvailableUpdate | null) => void = () => undefined;
    mocks.checkForUpdate.mockReturnValue(new Promise<AvailableUpdate | null>((resolve) => (finish = resolve)));
    open();
    await screen.findByText("Version 0.2.0 (abc1234)");
    await user.click(screen.getByRole("button", { name: "Check for updates" }));
    expect(screen.getByText("Looking for a newer version…")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Check for updates" })).toBeDisabled();
    finish(null);
    expect(await screen.findByText("You have the latest version (0.2.0).")).toBeInTheDocument();
  });

  it("offers a newer version with its notes", async () => {
    const user = userEvent.setup();
    mocks.checkForUpdate.mockResolvedValue(update(() => Promise.resolve()));
    open();
    await user.click(await screen.findByRole("button", { name: "Check for updates" }));
    expect(await screen.findByText("Version 0.3.0 is available.")).toBeInTheDocument();
    expect(screen.getByText("Fixes the desks.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Install" })).toBeInTheDocument();
  });

  it("shows the download's progress, then asks for a relaunch and never relaunches by itself", async () => {
    const user = userEvent.setup();
    let report: (progress: DownloadProgress) => void = () => undefined;
    let finish: () => void = () => undefined;
    mocks.checkForUpdate.mockResolvedValue(
      update((onProgress) => {
        report = onProgress;
        return new Promise<void>((resolve) => (finish = resolve));
      }),
    );
    mocks.relaunchApp.mockResolvedValue(undefined);
    open();
    await user.click(await screen.findByRole("button", { name: "Check for updates" }));
    await user.click(await screen.findByRole("button", { name: "Install" }));
    expect(screen.getByText(/Downloading version 0\.3\.0…/)).toBeInTheDocument();
    report({ received: 50, total: 200 });
    expect(await screen.findByText(/Downloading version 0\.3\.0… 25%/)).toBeInTheDocument();
    finish();
    expect(await screen.findByText(/Version 0\.3\.0 is installed\. Relaunch when you are ready/)).toBeInTheDocument();
    expect(mocks.relaunchApp).not.toHaveBeenCalled();
    await user.click(screen.getByRole("button", { name: "Relaunch now" }));
    expect(mocks.relaunchApp).toHaveBeenCalledTimes(1);
  });

  it("shows a failed check and recovers on Try again", async () => {
    const user = userEvent.setup();
    mocks.checkForUpdate.mockRejectedValueOnce(new Error("offline")).mockResolvedValueOnce(null);
    open();
    await user.click(await screen.findByRole("button", { name: "Check for updates" }));
    expect(await screen.findByText(/Could not check for updates: offline/)).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Try again" }));
    expect(await screen.findByText("You have the latest version (0.2.0).")).toBeInTheDocument();
  });

  it("shows a failed install and retries the install, not the check", async () => {
    const user = userEvent.setup();
    const install = vi.fn<AvailableUpdate["install"]>().mockRejectedValueOnce(new Error("bad signature")).mockResolvedValueOnce(undefined);
    mocks.checkForUpdate.mockResolvedValue(update(install));
    open();
    await user.click(await screen.findByRole("button", { name: "Check for updates" }));
    await user.click(await screen.findByRole("button", { name: "Install" }));
    expect(await screen.findByText(/Could not install the update: bad signature/)).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Try again" }));
    expect(await screen.findByRole("button", { name: "Relaunch now" })).toBeInTheDocument();
    expect(install).toHaveBeenCalledTimes(2);
    expect(mocks.checkForUpdate).toHaveBeenCalledTimes(1);
    expect(mocks.relaunchApp).not.toHaveBeenCalled();
  });

  it("keeps the installed update across closing and opening Settings", async () => {
    const user = userEvent.setup();
    mocks.checkForUpdate.mockResolvedValue(update(() => Promise.resolve()));
    const first = render(<UpdatesGroup loadInfo={() => Promise.resolve(INFO)} />);
    await user.click(await screen.findByRole("button", { name: "Check for updates" }));
    await user.click(await screen.findByRole("button", { name: "Install" }));
    await screen.findByRole("button", { name: "Relaunch now" });
    first.unmount();
    open();
    expect(await screen.findByRole("button", { name: "Relaunch now" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Check for updates" })).toBeDisabled();
  });

  it("keeps a download going across closing and opening Settings", async () => {
    const user = userEvent.setup();
    let report: (progress: DownloadProgress) => void = () => undefined;
    let finish: () => void = () => undefined;
    mocks.checkForUpdate.mockResolvedValue(
      update((onProgress) => {
        report = onProgress;
        return new Promise<void>((resolve) => (finish = resolve));
      }),
    );
    const first = render(<UpdatesGroup loadInfo={() => Promise.resolve(INFO)} />);
    await user.click(await screen.findByRole("button", { name: "Check for updates" }));
    await user.click(await screen.findByRole("button", { name: "Install" }));
    first.unmount();
    act(() => report({ received: 100, total: 200 }));
    open();
    expect(await screen.findByText(/Downloading version 0\.3\.0… 50%/)).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Install" })).not.toBeInTheDocument();
    act(() => finish());
    expect(await screen.findByRole("button", { name: "Relaunch now" })).toBeInTheDocument();
  });
});
