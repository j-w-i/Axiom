import "./e2e-probe"; // benchmark hooks: an empty module outside `vite build --mode e2e`; must run before React loads
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App";
import { host, TauriTransport } from "./daemon/tauri-transport";
import { createApp } from "./state/app";
import "./styles/tokens.css";
import "./styles/app.css";

performance.mark("axiom:script-start");
const app = createApp(new TauriTransport());
// Diagnostics, end-to-end tests and benchmarks (read-only handles).
(window as unknown as { __axiom?: unknown; __axiomHost?: unknown }).__axiom = app;
(window as unknown as { __axiomHost?: unknown }).__axiomHost = host;

const rootEl = document.getElementById("root");
if (!rootEl) throw new Error("#root missing");
createRoot(rootEl).render(
  <StrictMode>
    <App app={app} />
  </StrictMode>,
);
void app.client.connect().then(() => performance.mark("axiom:connected-subscribe"));
requestAnimationFrame(() => performance.mark("axiom:first-frame"));
