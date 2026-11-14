import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { isTauri } from "@tauri-apps/api/core";
import { App } from "./App";
import type { DiscoveryApi } from "./api/DiscoveryApi";
import { MockApi } from "./api/mockApi";
import { makeDemoSnapshot } from "./api/mockData";
import { TauriApi } from "./api/tauriApi";
import type { HandoverApi } from "./api/HandoverApi";
import { MockHandover } from "./api/mockHandover";
import { TauriHandover } from "./api/tauriHandover";
import type { RoutingApi } from "./api/RoutingApi";
import { MockRouting } from "./api/mockRouting";
import { TauriRouting } from "./api/tauriRouting";
import { HandoverContext } from "./components/handover/HandoverContext";
import { RoutingContext } from "./components/routing/RoutingContext";
import "./index.css";

const container = document.getElementById("root");
if (container === null) {
  throw new Error("#root not found");
}

// Inside the desktop app: real sessions from the Rust backend. In a plain
// browser (pnpm dev): the mock office with its demo simulation.
function makeApis(): { readonly api: DiscoveryApi; readonly routing: RoutingApi; readonly handover: HandoverApi } {
  if (isTauri()) return { api: new TauriApi(), routing: new TauriRouting(), handover: new TauriHandover() };
  const mock = new MockApi(makeDemoSnapshot());
  mock.startSimulation();
  return { api: mock, routing: new MockRouting(() => mock.getSettings()), handover: new MockHandover() };
}

const { api, routing, handover } = makeApis();

createRoot(container).render(
  <StrictMode>
    <RoutingContext value={routing}>
      <HandoverContext value={handover}>
        <App api={api} />
      </HandoverContext>
    </RoutingContext>
  </StrictMode>,
);
