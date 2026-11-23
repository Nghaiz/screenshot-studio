import React from "react";
import ReactDOM from "react-dom/client";
import "./index.css";
import { createQueryClient, Root } from "./Root";
import { usePrefs } from "./state/prefs";
import { applyAppearance } from "./themes";

async function start() {
  // `VITE_MOCK_IPC=1 npm run dev` runs the UI in a plain browser against 10,000 generated images.
  if (import.meta.env.VITE_MOCK_IPC) {
    const { installMockIpc } = await import("./mocks/ipc");
    const { MOCK_SIDECAR_READY } = await import("./mocks/smart");
    installMockIpc({
      firstRun: import.meta.env.VITE_MOCK_FIRST_RUN === "1",
      sidecar: import.meta.env.VITE_MOCK_SIDECAR === "ready" ? MOCK_SIDECAR_READY : undefined,
    });
  }
  applyAppearance(usePrefs.getState());
  ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
    <React.StrictMode>
      <Root client={createQueryClient()} />
    </React.StrictMode>,
  );
}

void start();
