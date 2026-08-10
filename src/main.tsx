import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { invoke } from "@tauri-apps/api/core";
import { check } from "@tauri-apps/plugin-updater";
import { relaunch, exit } from "@tauri-apps/plugin-process";
import { ask } from "@tauri-apps/plugin-dialog";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);

async function updateFromNative(): Promise<boolean> {
  try {
    const info = await invoke<{
      available: boolean;
      current: string;
      latest: string;
      error: string | null;
    }>("check_native_update");
    if (info.error || !info.available) return false;
    const yes = await ask(
      `A new version (${info.latest}) is available. Update now?`,
      { title: "Update Available", kind: "info", okLabel: "Update", cancelLabel: "Later" },
    );
    if (!yes) return false;
    await invoke("install_native_update", { version: info.latest });
    await exit(0);
    return true;
  } catch {
    return false;
  }
}

async function updateFromPlugin(): Promise<void> {
  try {
    const update = await check();
    if (!update) return;
    const yes = await ask(
      `A new version (${update.version}) is available. Update now?`,
      { title: "Update Available", kind: "info", okLabel: "Update", cancelLabel: "Later" },
    );
    if (!yes) return;
    await update.downloadAndInstall();
    await relaunch();
  } catch {
    // ignore update errors at startup
  }
}

(async () => {
  const updated = await updateFromNative();
  if (!updated) {
    await updateFromPlugin();
  }
})();
