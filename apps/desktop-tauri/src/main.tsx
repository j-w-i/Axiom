import "./e2e-probe"; // benchmark hooks: an empty module outside `vite build --mode e2e`; must run before React loads
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App";
import { host, TauriTransport } from "./daemon/tauri-transport";
import { watchFullscreen } from "./shell/platform";
import { createApp } from "./state/app";
import { shellStorage } from "./state/shell-storage";
import { toast, ToastStack, TooltipProvider } from "./ui/overlays";
import "./styles/fonts.css";
import "./styles/tokens.css";
import "./styles/base.css";
import "./styles/shell.css";

performance.mark("axiom:script-start");
// Per-OS text rendering (fonts.css): grey-scale smoothing on macOS only.
const platform = navigator.platform.toLowerCase();
document.documentElement.dataset.os = platform.includes("mac") ? "mac" : platform.includes("win") ? "windows" : "linux";
watchFullscreen();
// Shell-local preferences come back from the host before any store reads them (D-X-2).
void shellStorage.attach(host).then(start);

function start(): void {
  const app = createApp(new TauriTransport());
  // Diagnostics, end-to-end tests and benchmarks (read-only handles).
  (window as unknown as { __axiom?: unknown; __axiomHost?: unknown }).__axiom = app;
  (window as unknown as { __axiomHost?: unknown }).__axiomHost = host;
  (window as unknown as { __axiomToast?: unknown }).__axiomToast = toast;

  const rootEl = document.getElementById("root");
  if (!rootEl) throw new Error("#root missing");
  createRoot(rootEl).render(
    <StrictMode>
      <TooltipProvider>
        <App app={app} />
        <ToastStack />
      </TooltipProvider>
    </StrictMode>,
  );
  void app.client.connect().then(() => performance.mark("axiom:connected-subscribe"));
  requestAnimationFrame(() => performance.mark("axiom:first-frame"));
}
