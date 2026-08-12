import { invoke } from "@tauri-apps/api/core";
import { check } from "@tauri-apps/plugin-updater";
import { relaunch, exit } from "@tauri-apps/plugin-process";
import { ask } from "@tauri-apps/plugin-dialog";

export interface UpdateCheckResult {
  available: boolean;
  message: string;
}

export async function checkForUpdate(): Promise<UpdateCheckResult> {
  try {
    const info = await invoke<{
      available: boolean;
      current: string;
      latest: string;
      error: string | null;
    }>("check_native_update");
    if (!info.error && info.available) {
      return { available: true, message: `v${info.latest}` };
    }
  } catch {
    // fall through to plugin updater
  }

  try {
    const update = await check();
    if (update) {
      return { available: true, message: `v${update.version}` };
    }
  } catch {
    // ignore
  }

  return { available: false, message: "" };
}

export async function performUpdate(): Promise<boolean> {
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
    if (!yes) return true;
    await invoke("install_native_update", { version: info.latest });
    await exit(0);
    return true;
  } catch {
    // native updater not available, try the plugin updater
  }

  try {
    const update = await check();
    if (!update) return false;
    const yes = await ask(
      `A new version (${update.version}) is available. Update now?`,
      { title: "Update Available", kind: "info", okLabel: "Update", cancelLabel: "Later" },
    );
    if (!yes) return true;
    await update.downloadAndInstall();
    await relaunch();
    return true;
  } catch {
    return false;
  }
}

export async function updateFromNative(): Promise<boolean> {
  return performUpdate();
}

export async function updateFromPlugin(): Promise<void> {
  await performUpdate();
}
