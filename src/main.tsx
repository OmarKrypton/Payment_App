import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { check } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { ask } from "@tauri-apps/plugin-dialog";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);

check().then(async (update) => {
  if (!update) return;
  const yes = await ask(
    `A new version (${update.version}) is available. Update now?`,
    { title: "Update Available", kind: "info", okLabel: "Update", cancelLabel: "Later" },
  );
  if (!yes) return;
  await update.downloadAndInstall();
  await relaunch();
});
