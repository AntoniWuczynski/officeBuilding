import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { App } from "./App";
import { MockApi } from "./api/mockApi";

afterEach(() => {
  cleanup();
  localStorage.clear();
});

async function openShop(api: MockApi) {
  const user = userEvent.setup();
  render(<App api={api} />);
  await screen.findAllByTestId("storey");
  const shop = screen.getAllByTestId("storey").find((s) => s.getAttribute("data-project") === "p-shop");
  await user.click(shop ?? document.body);
  await screen.findByTestId("back-button");
  return user;
}

describe("the app's claude attach", () => {
  it("is hung up when the docked terminal closes", async () => {
    const api = new MockApi();
    vi.spyOn(api, "attachSession").mockResolvedValue(true);
    const detach = vi.spyOn(api, "detachSession");
    const user = await openShop(api);
    await user.click(screen.getByRole("button", { name: /map-tiles mirror/ }));
    await screen.findByTestId("terminal-panel");
    expect(detach).not.toHaveBeenCalled();
    await user.click(screen.getByTestId("terminal-close"));
    expect(detach).toHaveBeenCalledTimes(1);
    expect(detach).toHaveBeenCalledWith("s-3");
  });

  it("is hung up when another session takes the panel", async () => {
    const api = new MockApi();
    vi.spyOn(api, "attachSession").mockResolvedValue(true);
    const detach = vi.spyOn(api, "detachSession");
    const user = await openShop(api);
    await user.click(screen.getByRole("button", { name: /map-tiles mirror/ }));
    await screen.findByTestId("terminal-panel");
    await user.click(screen.getByRole("button", { name: /audit checkout/ }));
    expect(detach).toHaveBeenCalledTimes(1);
    expect(detach).toHaveBeenCalledWith("s-3");
  });

  it("is left alone for a session the app did not attach", async () => {
    const api = new MockApi();
    const detach = vi.spyOn(api, "detachSession");
    const user = await openShop(api);
    await user.click(screen.getByRole("button", { name: /audit checkout/ }));
    await user.click(await screen.findByTestId("terminal-close"));
    expect(detach).not.toHaveBeenCalled();
  });
});
