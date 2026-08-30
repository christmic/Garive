import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "../../desktop/frontend/src/App";
import type { DesktopCapabilities } from "../../desktop/frontend/src/ipc/host";
import "../../desktop/frontend/src/style.css";
import { WebProductEffectPort } from "./WebProductEffectPort";
import { FetchHostClient } from "./host";

const parameters = new URLSearchParams(window.location.search);
const configuredUrl = parameters.get("host") ?? import.meta.env.VITE_GARIVE_HOST_URL
  ?? localStorage.getItem("garive.web.host") ?? `${window.location.origin}/`;
let configured = true;
let host: FetchHostClient | undefined;
try {
  host = new FetchHostClient(configuredUrl, {
    maxCommandBytes: 64 * 1024, maxEventBytes: 64 * 1024,
    maxEvents: 2_048, followDeadlineMs: 24 * 60 * 60 * 1_000,
  });
  localStorage.setItem("garive.web.host", configuredUrl);
} catch { configured = false; }

const capabilities: DesktopCapabilities = {
  configured, agent_definition_id: configured ? "web-host-definition" : undefined,
  multi_turn: configured, durable_navigation: configured, activity: configured,
  setup: false, workspaces: false, artifacts: false, updater: false,
};
const createProductPort = host ? () => new WebProductEffectPort(host!) : undefined;
document.documentElement.dataset.client = "web";

createRoot(document.getElementById("root")!).render(
  <StrictMode><App client="web" webCapabilities={capabilities}
    createProductPort={createProductPort} /></StrictMode>,
);
