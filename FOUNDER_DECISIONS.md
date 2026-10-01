# FOUNDER DECISIONS — calls for Antek

A section with no status on its heading is open and shows in Office Building's
Decisions queue for this floor. Answering it in the app marks the heading
`DECIDED <date>` and records the answer here. Agent work lives in
[TODO.md](TODO.md), the founder's actions in [FOUNDER_TODO.md](FOUNDER_TODO.md).

## OB-001 — Base the backend on CCC or build our own? — DECIDED 2026-09-29: our own Rust backend

CCC (claude-command-center) moved to a revocable, non-commercial,
source-available licence on 2026-07-28. Office Building will be public and
never commercial. We port logic only from CCC v5.14.0, the last MIT release,
and keep its notice (THIRD_PARTY_NOTICES.md). Evidence:
`docs/research/ccc-evaluation.md`. Supersedes the 2026-09-28 call to base it
on CCC.

## OB-002 — Where do Decisions and TODOs come from? — DECIDED 2026-09-29: the repo's own files

Decisions read from FOUNDER_DECISIONS.md or HUMAN_DECISIONS.md (plus the
agents' DECISIONS.md log), human TODOs from FOUNDER_TODO.md or HUMAN_TODO.md,
agent TODOs from TODO.md, each falling back to the app's own store when a repo
has no such file.

## OB-011 — How should assistant agents reach the building (MCP transport and auth)? — DECIDED 2026-10-01: Defer for now, later a proper server deployment of the app itself might happen…

Decided in Office Building on 2026-10-01: Defer for now, later a proper server deployment of the app itself might happen and then we’ll come back to this.

The MCP server (OB-010 in TODO.md) lets assistant agents list
floors and desks, read the decisions queue, answer calls and hire agents. It
exposes write actions on your Mac, so how it listens and who may call it is
your call.

**Options.**

- **Streamable HTTP on 127.0.0.1 with a bearer token (recommended)** — the
  same shape as other local HTTP MCP servers (`claude mcp add --transport http … --header
  "Authorization: Bearer …"`). The app writes a random token to a mode-600
  file on first run. Works for any local client, and a server-side agent can
  reach it through the SSH tunnel you already use. Cost: one more local port.
- **stdio through a small launcher** — the client starts
  `office-building-mcp`, which forwards to the running app over a Unix
  socket. No port and no token, but only clients on this Mac can use it, and
  each client needs the launcher path.
- **Defer** — keep the building GUI-only until an assistant actually needs it.

**Recommendation.** Streamable HTTP with a token file. It matches the HTTP
MCP servers assistants already use, so their config pattern is the same.
