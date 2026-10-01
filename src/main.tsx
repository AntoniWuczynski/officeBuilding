import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { isTauri } from "@tauri-apps/api/core";
import { App } from "./App";
import type { DiscoveryApi } from "./api/DiscoveryApi";
import { MockApi } from "./api/mockApi";
import { TauriApi } from "./api/tauriApi";
import "./index.css";

const container = document.getElementById("root");
if (container === null) {
  throw new Error("#root not found");
}

// Inside the desktop app: real sessions from the Rust backend. In a plain
// browser (pnpm dev): the mock office with its demo simulation.
function makeApi(): DiscoveryApi {
  if (isTauri()) return new TauriApi();
  const mock = new MockApi();
  mock.startSimulation();
  return mock;
}

createRoot(container).render(
  <StrictMode>
    <App api={makeApi()} />
  </StrictMode>,
);
