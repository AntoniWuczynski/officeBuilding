//! A floor's project manager: the agent the building seats in a floor's office to
//! hand work to its desks. `wire` holds what the webview and `store.json` see. `auth`,
//! `socket` and `helper` carry its tool calls from its MCP helper to the app, `tools`
//! is what it may call, and `mcp` is the protocol it calls in.

pub mod auth;
pub mod helper;
pub mod mcp;
pub mod socket;
pub mod tools;
pub mod wire;
