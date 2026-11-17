import { describe, expect, it, vi } from "vitest";
import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { App } from "./App";
import { MockApi } from "./api/mockApi";

describe("the Needs-you list in the app", () => {
  it("lists the decisions-file call after the live asks, and Answer reaches the api with that question's id", async () => {
    const api = new MockApi();
    const spy = vi.spyOn(api, "answerQuestion");
    const user = userEvent.setup();
    render(<App api={api} />);
    await user.click(await screen.findByTestId("needs-you"));
    const rows = screen.getAllByTestId("needs-you-row");
    expect(rows.map((r) => r.getAttribute("data-question"))).toEqual(["q-1", "q-2", "q-3"]);
    const file = rows[2] as HTMLElement;
    expect(file).toHaveTextContent("officeBuilding: From FOUNDER_DECISIONS.md");
    await user.click(within(file).getByRole("button", { name: "Answer" }));
    await user.click(within(file).getByRole("button", { name: /^A:/ }));
    expect(spy).toHaveBeenCalledWith("q-3", { kind: "option", optionId: "o-5" });
  });
});
