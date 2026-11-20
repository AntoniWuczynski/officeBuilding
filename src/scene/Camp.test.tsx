import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { ROOM, doorOf } from "../lib/layout";
import { Beach } from "./Camp";

/** Where each palm's foot stands on the room's floor plane. */
function feet(): { readonly x: number; readonly y: number }[] {
  return screen.getAllByTestId("camp-palm").map((svg) => {
    const at = svg.closest<HTMLElement>(".billboard");
    return { x: parseFloat(at?.style.left ?? "NaN"), y: parseFloat(at?.style.top ?? "NaN") };
  });
}

describe("the beach's palms", () => {
  for (const depth of [120, 200, 340]) {
    it(`leave the open corner by the door clear, where the sign-out stand stands (depth ${depth})`, () => {
      render(<Beach width={ROOM.width} depth={depth} />);
      const door = doorOf(depth);
      const spots = feet();
      expect(spots).toHaveLength(3);
      // Every palm stands off a corner; none off the one nearest the door.
      for (const p of spots) expect(Math.hypot(p.x - door.x, p.y - door.y)).toBeGreaterThan(Math.hypot(ROOM.width + 18 - door.x, depth + 18 - door.y));
    });
  }
});
