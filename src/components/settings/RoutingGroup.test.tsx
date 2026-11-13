import { useState } from "react";
import { describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { makeRoutingSettings, MockRouting } from "../../api/mockRouting";
import { makeSettings } from "../../api/mockSettings";
import { RoutingContext } from "../routing/RoutingContext";
import type { RoutingConnection, RoutingSettings } from "../../types";
import { RoutingGroup } from "./RoutingGroup";
import { nth } from "../../test/nth";

const NO_TARGETS = { fable: null, opus: null, sonnet: null, haiku: null } as const;

function connection(id: string, kind: RoutingConnection["kind"], over: Partial<RoutingConnection> = {}): RoutingConnection {
  return { id, name: id, kind, context: "ctx-a", paid: false, experimental: false, targets: NO_TARGETS, ...over };
}

function seeded(connections: readonly RoutingConnection[]): RoutingSettings {
  return { ...makeRoutingSettings(), contexts: [{ id: "ctx-a", name: "Work", chain: connections.map((c) => c.id) }], connections };
}

interface Shown {
  readonly onChange: ReturnType<typeof vi.fn<(next: RoutingSettings) => void>>;
  readonly latest: () => RoutingSettings;
}

function show(initial: RoutingSettings, withProvider = true): Shown {
  let current = initial;
  const onChange = vi.fn<(next: RoutingSettings) => void>((next) => {
    current = next;
  });
  const api = new MockRouting(() => Promise.resolve({ ...makeSettings(), routing: current }));
  function Harness(): React.ReactElement {
    const [value, setValue] = useState(initial);
    const group = (
      <RoutingGroup
        value={value}
        onChange={(next) => {
          onChange(next);
          setValue(next);
        }}
      />
    );
    return withProvider ? <RoutingContext value={api}>{group}</RoutingContext> : group;
  }
  render(<Harness />);
  return { onChange, latest: () => current };
}

describe("RoutingGroup", () => {
  it("saves the switch", async () => {
    const { latest } = show(makeRoutingSettings());
    await userEvent.click(screen.getByRole("switch", { name: "Route Claude Code desks through the building" }));
    expect(latest().enabled).toBe(true);
  });

  it("adds a context by button and by Enter, and keeps Add off for a blank name", async () => {
    const { latest } = show(makeRoutingSettings());
    const add = screen.getByRole("button", { name: "Add context" });
    expect(add).toBeDisabled();
    await userEvent.type(screen.getByTestId("routing-new-context"), " Work ");
    await userEvent.click(add);
    expect(latest().contexts.map((c) => c.name)).toEqual(["Work"]);
    await userEvent.type(screen.getByTestId("routing-new-context"), "Home{Enter}");
    expect(latest().contexts.map((c) => c.name)).toEqual(["Work", "Home"]);
    expect(screen.getByTestId("routing-new-context")).toHaveValue("");
  });

  it("renames a context and refuses an empty name", async () => {
    const { latest } = show(seeded([]));
    const input = screen.getByTestId("routing-context-name-ctx-a");
    await userEvent.clear(input);
    fireEvent.blur(input);
    expect(screen.getByText("Enter a name of 1 to 60 characters.")).toBeInTheDocument();
    expect(latest().contexts[0]?.name).toBe("Work");
    await userEvent.type(input, "Office{Enter}");
    expect(latest().contexts[0]?.name).toBe("Office");
  });

  it("disables a second Claude subscription and the sign-in, with the sentence", async () => {
    show(seeded([connection("one", "claude-subscription")]));
    const picker = screen.getByRole("combobox", { name: "Kind" });
    expect(screen.getByRole("option", { name: "Claude subscription" })).toBeDisabled();
    expect(screen.getByRole("option", { name: "Sign in with ChatGPT" })).toBeDisabled();
    expect(screen.getByRole("option", { name: "OpenRouter" })).toBeEnabled();
    await userEvent.selectOptions(picker, "OpenRouter");
    expect(screen.getByRole("button", { name: "Add connection" })).toBeEnabled();
  });

  it("shows why a refused kind is refused", () => {
    const { latest } = show(seeded([connection("one", "claude-subscription")]));
    fireEvent.change(screen.getByRole("combobox", { name: "Kind" }), { target: { value: "claude-subscription" } });
    expect(screen.getByText("The Claude login on this Mac can belong to one context only.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Add connection" })).toBeDisabled();
    expect(latest().connections).toHaveLength(1);
  });

  it("appends a new connection to the order", async () => {
    const { latest } = show(seeded([connection("one", "claude-subscription")]));
    await userEvent.click(screen.getByRole("button", { name: "Add connection" }));
    const { connections, contexts } = latest();
    expect(connections.map((c) => c.kind)).toEqual(["claude-subscription", "anthropic-api-key"]);
    expect(contexts[0]?.chain).toEqual(connections.map((c) => c.id));
    expect(screen.getByText(/2\. .*, Anthropic API key/)).toBeInTheDocument();
  });

  it("shows the spend switch only on a metered kind and the experimental switch only on the OpenAI kind", async () => {
    show(seeded([connection("sub", "claude-subscription"), connection("api", "anthropic-api-key"), connection("oai", "openai-api-key")]));
    await screen.findAllByTestId(/-status$/);
    expect(screen.getAllByRole("switch", { name: "May spend money" })).toHaveLength(2);
    expect(screen.getAllByRole("switch", { name: "Allow this experimental route" })).toHaveLength(1);
    expect(screen.getByText(/never stores that login/)).toBeInTheDocument();
  });

  it("saves the opt-ins", async () => {
    const { latest } = show(seeded([connection("oai", "openai-api-key")]));
    await userEvent.click(screen.getByRole("switch", { name: "May spend money" }));
    await userEvent.click(screen.getByRole("switch", { name: "Allow this experimental route" }));
    expect(latest().connections[0]).toMatchObject({ paid: true, experimental: true });
  });

  it("reorders and disables the buttons at the ends", async () => {
    const { latest } = show(seeded([connection("a", "anthropic-api-key"), connection("b", "openrouter")]));
    const cards = (): HTMLElement[] => screen.getAllByRole("listitem");
    expect(within(nth(cards(), 0)).getByRole("button", { name: "Move up" })).toBeDisabled();
    expect(within(nth(cards(), 1)).getByRole("button", { name: "Move down" })).toBeDisabled();
    await userEvent.click(within(nth(cards(), 1)).getByRole("button", { name: "Move up" }));
    expect(latest().contexts[0]?.chain).toEqual(["b", "a"]);
    await userEvent.click(within(nth(cards(), 0)).getByRole("button", { name: "Move down" }));
    expect(latest().contexts[0]?.chain).toEqual(["a", "b"]);
  });

  it("renames a connection", async () => {
    const { latest } = show(seeded([connection("a", "openrouter")]));
    const input = screen.getByTestId("routing-name-a");
    await userEvent.clear(input);
    await userEvent.type(input, "Spare{Enter}");
    expect(latest().connections[0]?.name).toBe("Spare");
    await userEvent.clear(input);
    fireEvent.blur(input);
    expect(screen.getByText("Enter a name of 1 to 60 characters.")).toBeInTheDocument();
    await userEvent.type(input, "Spare");
    fireEvent.blur(input);
    expect(screen.queryByText("Enter a name of 1 to 60 characters.")).not.toBeInTheDocument();
  });

  it("asks before removing a connection, in a row", async () => {
    const { onChange, latest } = show(seeded([connection("a", "anthropic-api-key")]));
    await userEvent.click(screen.getByRole("button", { name: "Remove connection" }));
    expect(screen.getByText("Remove this connection? Its saved key is removed too.")).toBeInTheDocument();
    expect(onChange).not.toHaveBeenCalled();
    await userEvent.click(screen.getByRole("button", { name: "Keep" }));
    expect(screen.queryByText(/Remove this connection/)).not.toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Remove connection" }));
    await userEvent.click(screen.getByRole("button", { name: "Remove" }));
    expect(latest().connections).toEqual([]);
    expect(screen.getByText("No connections yet.")).toBeInTheDocument();
  });

  it("asks before removing a context, naming its connections", async () => {
    const { latest } = show(seeded([connection("a", "openrouter"), connection("b", "claude-subscription")]));
    await userEvent.click(screen.getByRole("button", { name: "Remove context" }));
    expect(screen.getByText("Remove Work and its 2 connections? Their saved keys are removed too.")).toBeInTheDocument();
    await userEvent.click(nth(screen.getAllByRole("button", { name: "Remove" }), 0));
    expect(latest().contexts).toEqual([]);
    expect(latest().connections).toEqual([]);
  });

  it("words the removal of an empty context and of one connection", async () => {
    show(seeded([]));
    await userEvent.click(screen.getByRole("button", { name: "Remove context" }));
    expect(screen.getByText("Remove Work?")).toBeInTheDocument();
  });

  it("says one connection in the singular", async () => {
    show(seeded([connection("b", "claude-subscription")]));
    await userEvent.click(screen.getByRole("button", { name: "Remove context" }));
    expect(screen.getByText("Remove Work and its 1 connection? Their saved keys are removed too.")).toBeInTheDocument();
  });

  it("saves a model name and an efforts text, and refuses a bad one", async () => {
    const { latest } = show(seeded([connection("a", "anthropic-api-key")]));
    const efforts = screen.getByTestId("routing-efforts-a-opus");
    expect(efforts).toBeDisabled();
    await userEvent.type(screen.getByTestId("routing-model-a-opus"), "model-x{Enter}");
    expect(latest().connections[0]?.targets.opus).toEqual({ model: "model-x", efforts: {} });
    await userEvent.type(efforts, "xhigh=high{Enter}");
    expect(latest().connections[0]?.targets.opus).toEqual({ model: "model-x", efforts: { xhigh: "high" } });
    const before = latest();
    await userEvent.clear(efforts);
    await userEvent.type(efforts, "nonsense{Enter}");
    expect(screen.getByText("Write each pair as asked=sent, separated by commas.")).toBeInTheDocument();
    expect(latest()).toBe(before);
    await userEvent.clear(screen.getByTestId("routing-model-a-opus"));
    fireEvent.blur(screen.getByTestId("routing-model-a-opus"));
    expect(latest().connections[0]?.targets.opus).toBeNull();
  });

  it("words the effort hint by kind", async () => {
    show(seeded([connection("a", "anthropic-api-key"), connection("b", "openai-api-key")]));
    await screen.findAllByTestId(/-status$/);
    expect(screen.getByText("Efforts are sent unchanged unless a pair here says otherwise.")).toBeInTheDocument();
    expect(screen.getByText("Every effort a desk may ask for needs a pair here, or that request is refused.")).toBeInTheDocument();
  });

  it("offers no models for a pass-through connection", () => {
    show(seeded([connection("a", "claude-subscription")]));
    expect(screen.queryByText("Models")).not.toBeInTheDocument();
  });

  it("says a ChatGPT sign-in is unavailable", () => {
    show(seeded([connection("a", "chatgpt-sign-in")]));
    expect(screen.getByText(/Sign in with ChatGPT is not built\. OpenAI has to accept the building/, { selector: "p" })).toBeInTheDocument();
  });

  it("saves a tier's model names", async () => {
    const { latest } = show(makeRoutingSettings());
    await userEvent.type(screen.getByTestId("routing-tier-sol".replace("sol", "opus")), "one, two  three,one{Enter}");
    expect(latest().tiers.opus).toEqual(["one", "two", "three"]);
  });

  it("saves a key, reads it as saved and keeps the typed key out of the page", async () => {
    show(seeded([connection("a", "anthropic-api-key")]));
    const status = await screen.findByTestId("routing-key-a-status");
    expect(status).toHaveTextContent("No API key is saved for this connection.");
    await userEvent.type(screen.getByTestId("routing-key-a"), "sk-test-1234");
    await userEvent.click(screen.getByTestId("routing-key-a-save"));
    expect(await screen.findByText(/An API key is saved in your Keychain/)).toBeInTheDocument();
    expect(screen.getByTestId("routing-key-a")).toHaveValue("");
    expect(document.body.innerHTML).not.toContain("sk-test-1234");
    await userEvent.click(screen.getByTestId("routing-key-a-remove"));
    expect(await screen.findByText("No API key is saved for this connection.")).toBeInTheDocument();
  });

  it("shows a key sentence for a Keychain that did not answer", async () => {
    const api = new MockRouting(() => Promise.resolve(makeSettings()));
    vi.spyOn(api, "keyStatus").mockResolvedValue({ kind: "unavailable", reason: "locked" });
    render(
      <RoutingContext value={api}>
        <RoutingGroup value={seeded([connection("a", "openrouter")])} onChange={vi.fn()} />
      </RoutingContext>,
    );
    expect(await screen.findByText("The Keychain did not hand over the key: locked.")).toBeInTheDocument();
  });

  it("has no key field without a provider", () => {
    show(seeded([connection("a", "anthropic-api-key")]), false);
    expect(screen.queryByTestId("routing-key-a")).not.toBeInTheDocument();
  });

  it("words the master switch hint for a running building and drops the old one", () => {
    show(makeRoutingSettings());
    expect(screen.getByText(/Desks that are already running are not changed either way/)).toHaveTextContent("if the building quits, the desk loses its model access");
    expect(screen.queryByText(/Nothing is routed until the router itself is built/)).not.toBeInTheDocument();
  });

  it("shows the key and unsupported-kind hints on their kinds only", () => {
    show(seeded([connection("sub", "claude-subscription"), connection("api", "anthropic-api-key"), connection("oai", "openai-api-key")]));
    expect(screen.getAllByText("Use a key that belongs to one workspace. The building cannot send a workspace id.")).toHaveLength(1);
    expect(screen.queryByText(/This build cannot use this kind of connection yet/)).not.toBeInTheDocument();
    expect(screen.getAllByText(/^It carries text, images, tool calls and tool results\./)).toHaveLength(1);
    expect(screen.getAllByText(/^It refuses a request that needs documents/)).toHaveLength(1);
    expect(screen.getAllByText("Use a key that belongs to one organisation and project. The building cannot name another.")).toHaveLength(1);
    expect(screen.getAllByText(/^This route is experimental\. Each request is translated from Anthropic’s format to OpenAI’s/)).toHaveLength(1);
  });
});
