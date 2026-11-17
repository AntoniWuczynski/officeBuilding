import { useState } from "react";
import { describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useEscapeClose } from "./useEscapeClose";

function Layer({ name, open = true, close }: { readonly name: string; readonly open?: boolean; readonly close: (name: string) => void }): React.ReactElement {
  // A new function every render, as an inline `() => ...` caller passes.
  useEscapeClose(open, () => close(name));
  return <i data-testid={name} />;
}

describe("useEscapeClose", () => {
  it("closes only the newest layer per Escape", async () => {
    const close = vi.fn();
    render(
      <>
        <Layer name="under" close={close} />
        <Layer name="over" close={close} />
      </>,
    );
    await userEvent.setup().keyboard("{Escape}");
    expect(close.mock.calls).toEqual([["over"]]);
  });

  it("leaves an Escape that something nearer already handled", async () => {
    const close = vi.fn();
    render(
      <>
        <Layer name="popover" close={close} />
        <input data-testid="field" onKeyDown={(e) => e.key === "Escape" && e.preventDefault()} />
      </>,
    );
    const user = userEvent.setup();
    await user.click(screen.getByTestId("field"));
    await user.keyboard("{Escape}");
    expect(close).not.toHaveBeenCalled();
  });

  it("keeps its place in the stack when only the lower layer re-renders with a new close", async () => {
    const close = vi.fn();
    function Under(): React.ReactElement {
      const [n, setN] = useState(0);
      useEscapeClose(true, () => close(`under${n}`));
      return (
        <button type="button" onClick={() => setN(n + 1)}>
          {n}
        </button>
      );
    }
    render(
      <>
        <Under />
        <Layer name="over" close={close} />
      </>,
    );
    const user = userEvent.setup();
    await user.click(screen.getByRole("button"));
    await user.keyboard("{Escape}");
    expect(close.mock.calls).toEqual([["over"]]);
  });
});
