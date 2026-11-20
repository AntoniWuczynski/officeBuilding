import { useState } from "react";
import { NeedsYou } from "./NeedsYou";

/** Tests only: the Needs-you list keeping its own open state, as the building view keeps it. */
export function OwnNeedsYou(props: Omit<React.ComponentProps<typeof NeedsYou>, "open" | "onOpenChange">): React.ReactElement {
  const [open, setOpen] = useState(false);
  return <NeedsYou {...props} open={open} onOpenChange={setOpen} />;
}
