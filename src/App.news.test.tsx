import { beforeEach, describe, expect, it, vi } from "vitest";
import { act, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { App } from "./App";
import { MockApi } from "./api/mockApi";
import type { OfficeSnapshot } from "./types";

// jsdom has no canvas: a terminal that draws nothing is enough to have a session open in the docked panel.
vi.mock("@xterm/xterm", () => ({
  Terminal: class {
    cols = 80;
    rows = 24;
    options = {};
    loadAddon(): void {}
    open(): void {}
    write(): void {}
    onData(): { dispose(): void } {
      return { dispose: () => {} };
    }
    onResize(): { dispose(): void } {
      return { dispose: () => {} };
    }
    focus(): void {}
    resize(): void {}
    dispose(): void {}
  },
}));
vi.mock("@xterm/addon-fit", () => ({ FitAddon: class { fit(): void {} } }));

/** A MockApi whose subscribers can be handed an arbitrary snapshot (an upstream change). */
class PushableApi extends MockApi {
  private readonly pushed = new Set<(snapshot: OfficeSnapshot) => void>();
  override subscribe(listener: (snapshot: OfficeSnapshot) => void): () => void {
    this.pushed.add(listener);
    const off = super.subscribe(listener);
    return () => {
      this.pushed.delete(listener);
      off();
    };
  }
  push(snapshot: OfficeSnapshot): void {
    for (const listener of this.pushed) listener(snapshot);
  }
}

const VIEW = "office-building:view";
const SEEN = "office-building:seen";
const MODES = ["office", "islands"] as const;
type Mode = (typeof MODES)[number];

beforeEach(() => {
  localStorage.clear();
  // A news baseline older than the seed data, so its idle agents count as new.
  localStorage.setItem("office-building:seen-since", "0");
});

async function show(mode: Mode, api = new MockApi()): Promise<ReturnType<typeof userEvent.setup>> {
  localStorage.setItem(VIEW, mode);
  render(<App api={api} />);
  await screen.findAllByTestId(mode === "office" ? "floor-nameplate" : "island-nameplate");
  return userEvent.setup();
}

const plate = (name: string): Promise<HTMLElement> => screen.findByRole("button", { name: new RegExp(`^${name}: .* Show (floor|island)\\.$`) });

async function openPlace(user: ReturnType<typeof userEvent.setup>, name: string): Promise<HTMLElement> {
  await user.click(await plate(name));
  return screen.getByTestId("place-panel");
}

describe.each(MODES)("the last reply, in the %s view", (mode) => {
  it("shows on a place's row, and a null reply shows nothing", async () => {
    const user = await show(mode);
    const panel = await openPlace(user, "corner_shop");
    const rows = within(panel).getAllByTestId("place-row");
    const byTitle = (title: string): HTMLElement => rows.find((r) => r.textContent?.includes(title)) as HTMLElement;
    expect(within(byTitle("audit checkout")).getByTestId("place-reply")).toHaveTextContent("Last reply: Checkout audit is done: 3 findings filed to TODO.md.");
    expect(within(byTitle("map-tiles mirror")).queryByTestId("place-reply")).not.toBeInTheDocument();
  });
});

describe.each(MODES)("the floor's own screens, entered from the %s view", (mode) => {
  it("show the reply on the hover card (and none for a null one) and on the sign-out sheet", async () => {
    const user = await show(mode);
    const panel = await openPlace(user, "corner_shop");
    await user.click(within(panel).getByTestId("place-enter"));
    await screen.findByTestId("back-button");
    const worker = await screen.findByRole("button", { name: /audit checkout/ });
    await user.hover(worker);
    expect(screen.getByTestId("worker-card")).toHaveTextContent("Last reply: Checkout audit is done: 3 findings filed to TODO.md.");
    await user.unhover(worker);
    await user.hover(screen.getByRole("button", { name: /map-tiles mirror/ }));
    expect(screen.getByTestId("worker-card")).toHaveTextContent("map-tiles mirror");
    expect(screen.queryByTestId("worker-reply")).not.toBeInTheDocument();
    await user.click(screen.getByTestId("rail-signed-out"));
    const [refund, price] = screen.getAllByTestId("signed-out-row");
    expect(within(refund as HTMLElement).getByTestId("signed-out-reply")).toHaveTextContent("Last reply: Refund emails now go out in the customer's language.");
    expect(within(price as HTMLElement).queryByTestId("signed-out-reply")).not.toBeInTheDocument();
  });
});

describe.each(MODES)("new since you looked, in the %s view", (mode) => {
  it("dots an agent at rest and counts it in the top bar; opening it clears both", async () => {
    const user = await show(mode);
    // Never opened: the four at rest (s-2 waiting, s-3 stuck, s-5 and s-6 idle) are new; s-1 working and s-4 thinking are not.
    expect(screen.getByTestId("new-count")).toHaveTextContent("4 new");
    const panel = await openPlace(user, "corner_shop");
    const dotted = (): string[] => within(panel).getAllByTestId("place-row").filter((r) => within(r).queryByTestId("new-dot") !== null).map((r) => r.querySelector("b")?.textContent ?? "");
    expect(dotted()).toEqual(["map-tiles mirror", "orders ingest fix"]);
    // map-tiles mirror runs in its own window: opening it only raises that window, and still counts as looking.
    await user.click(within(panel).getByRole("button", { name: /map-tiles mirror/ }));
    expect(dotted()).toEqual(["orders ingest fix"]);
    expect(screen.getByTestId("new-count")).toHaveTextContent("3 new");
    expect(Object.keys(JSON.parse(localStorage.getItem(SEEN) ?? "{}") as object)).toEqual(["s-3"]);
  });

  it("never dots a working agent that moved after the stored time, and leaves an idle one alone that did not", async () => {
    localStorage.setItem(SEEN, JSON.stringify({ "s-1": Date.parse("2026-09-28T15:00:00Z"), "s-3": Date.parse("2026-09-28T16:00:00Z"), "s-5": Date.parse("2026-09-28T16:00:00Z") }));
    const user = await show(mode);
    expect(screen.getByTestId("new-count")).toHaveTextContent("2 new");
    const shop = await openPlace(user, "corner_shop");
    expect(within(shop).getAllByTestId("new-dot")).toHaveLength(1);
    expect(within(shop).getAllByTestId("place-row").find((r) => r.textContent?.includes("audit checkout"))?.querySelector("[data-testid=new-dot]")).toBeNull();
  });

  it("does not count an agent that stops while its terminal is open in front of you", async () => {
    vi.useFakeTimers({ toFake: ["Date"] });
    try {
      vi.setSystemTime(new Date("2026-09-29T09:00:00Z"));
      const api = new PushableApi();
      const user = await show(mode, api);
      const panel = await openPlace(user, "corner_shop");
      await user.click(within(panel).getByRole("button", { name: /orders ingest fix/ }));
      await screen.findByTestId("back-button");
      const seenAt = (): number => (JSON.parse(localStorage.getItem(SEEN) ?? "{}") as Record<string, number>)["s-2"] ?? 0;
      expect(seenAt()).toBe(Date.parse("2026-09-29T09:00:00Z"));
      // It moves on at 10:00 and rests; you have been looking since 09:00.
      vi.setSystemTime(new Date("2026-09-29T10:05:00Z"));
      const snapshot = await api.getSnapshot();
      act(() => api.push({ ...snapshot, sessions: snapshot.sessions.map((s) => (s.id === "s-2" ? { ...s, state: "idle", lastActivityAt: "2026-09-29T10:00:00Z" } : s)) }));
      await vi.waitFor(() => expect(seenAt()).toBe(Date.parse("2026-09-29T10:05:00Z")));
    } finally {
      vi.useRealTimers();
    }
  });

  it("shows no count when nothing is new", async () => {
    localStorage.setItem(SEEN, JSON.stringify(Object.fromEntries(["s-2", "s-3", "s-5", "s-6"].map((id) => [id, Date.now()]))));
    await show(mode);
    expect(screen.queryByTestId("new-count")).not.toBeInTheDocument();
  });
});

describe.each(MODES)("the Boards sheet, in the %s view", (mode) => {
  it("lists every shared file's items and ticks one", async () => {
    const user = await show(mode);
    await user.click(screen.getByRole("button", { name: "Boards 3" }));
    const sheet = screen.getByRole("region", { name: "Shared boards" });
    expect(within(sheet).getByRole("heading", { name: "MY-ACTIONS.md" })).toBeInTheDocument();
    expect(within(sheet).getAllByTestId("todo-check")).toHaveLength(3);
    await user.click(within(sheet).getByRole("checkbox", { name: /Renew the domain/ }));
    // A ticked item leaves the open list; the board keeps it under "Show 1 done".
    expect(await screen.findByRole("button", { name: "Boards 2" })).toBeInTheDocument();
    expect(within(sheet).queryByRole("checkbox", { name: /Renew the domain/ })).not.toBeInTheDocument();
    await user.click(within(sheet).getByTestId("toggle-done"));
    expect(within(sheet).getByRole("checkbox", { name: /Renew the domain/ })).toBeChecked();
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("region", { name: "Shared boards" })).not.toBeInTheDocument();
  });

  it("shows a tagged item on that place's Human TODO, marked with the board's name, and an untagged one only on the board", async () => {
    const user = await show(mode);
    const panel = await openPlace(user, "corner_shop");
    await user.click(within(panel).getByTestId("place-enter"));
    await user.click(await screen.findByTestId("rail-human-todo"));
    const sidebar = screen.getByTestId("sidebar");
    const tagged = within(sidebar).getByRole("checkbox", { name: /Post the release note/ });
    expect(within(tagged.closest("li") as HTMLElement).getByTestId("todo-source")).toHaveTextContent("MY-ACTIONS.md");
    expect(within(sidebar).getByRole("checkbox", { name: /Rotate the tile API key/ })).toBeInTheDocument();
    expect(within(sidebar).queryByRole("checkbox", { name: /Approve the sprite pack/ })).not.toBeInTheDocument();
    expect(within(sidebar).queryByRole("checkbox", { name: /Renew the domain/ })).not.toBeInTheDocument();
  });

  it("takes a linked tag, any case, onto that place's Human TODO", async () => {
    const user = await show(mode);
    const panel = await openPlace(user, "officeBuilding");
    await user.type(within(panel).getByRole("textbox", { name: /Link a board tag/ }), "  office BUILDING ");
    await user.click(within(panel).getByRole("button", { name: "Link" }));
    expect(within(panel).getByTestId("place-tag")).toHaveTextContent("office BUILDING");
    expect(JSON.parse(localStorage.getItem("office-building:overrides") ?? "{}")).toMatchObject({ tags: { "p-office": ["office BUILDING"] } });
    await user.click(within(panel).getByTestId("place-enter"));
    await user.click(await screen.findByTestId("rail-human-todo"));
    expect(within(screen.getByTestId("sidebar")).getByRole("checkbox", { name: /Approve the sprite pack/ })).toBeInTheDocument();
  });
});

describe("Settings > Boards", () => {
  it("adds a file by its full path and the sheet lists it, then removes it", async () => {
    const user = await show("office");
    await act(async () => {
      await user.keyboard("{Meta>},{/Meta}");
    });
    const settings = await screen.findByTestId("settings-dialog");
    await user.type(within(settings).getByTestId("settings-boards-add"), "relative.md{Enter}");
    expect(await within(settings).findByRole("alert")).toHaveTextContent("full path");
    await user.clear(within(settings).getByTestId("settings-boards-add"));
    await user.type(within(settings).getByTestId("settings-boards-add"), "/Users/me/QUEUE.md{Enter}");
    expect(await within(settings).findByText("/Users/me/QUEUE.md")).toBeInTheDocument();
    await user.click(within(settings).getByTestId("settings-close"));
    await user.click(screen.getByRole("button", { name: /^Boards/ }));
    expect(within(screen.getByRole("region", { name: "Shared boards" })).getAllByRole("heading").map((h) => h.textContent)).toEqual(["QUEUE.md"]);
    await user.click(screen.getByRole("button", { name: /^Boards/ }));
    await user.keyboard("{Meta>},{/Meta}");
    await user.click(await screen.findByRole("button", { name: "Remove /Users/me/QUEUE.md" }));
    expect(screen.queryByText("/Users/me/QUEUE.md")).not.toBeInTheDocument();
  });
});

describe("Escape in Settings over an open place panel", () => {
  it("closes Settings and leaves the Office place panel open", async () => {
    const user = await show("office");
    await openPlace(user, "corner_shop");
    await act(async () => {
      await user.keyboard("{Meta>},{/Meta}");
    });
    const settings = await screen.findByTestId("settings-dialog");
    within(settings).getByTestId("settings-boards-add").focus();
    await user.keyboard("{Escape}");
    expect(screen.queryByTestId("settings-dialog")).not.toBeInTheDocument();
    expect(screen.getByTestId("place-panel")).toBeInTheDocument();
  });
});
