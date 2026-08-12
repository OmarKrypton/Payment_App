import { invoke } from "@tauri-apps/api/core";
import { check } from "@tauri-apps/plugin-updater";
import { relaunch, exit } from "@tauri-apps/plugin-process";
import { ask } from "@tauri-apps/plugin-dialog";

export type UpdateSource = "native" | "appimage" | "unknown";

export interface UpdateCheckResult {
  available: boolean;
  message: string;
}

export async function getUpdateSource(): Promise<UpdateSource> {
  try {
    const src = await invoke<string>("update_source");
    return src === "native" ? "native" : src === "appimage" ? "appimage" : "unknown";
  } catch {
    return "unknown";
  }
}

// Native Linux builds self-update by downloading the vouchify-linux-x86_64
// asset and swapping the executable. AppImage builds use the plugin updater.
// The two are never mixed: the plugin updater would replace a native binary
// with an AppImage (grey screen on this system), and installing the raw ELF
// over an AppImage would break AppImage launches.
async function applyNative(version: string): Promise<boolean> {
  const yes = await ask(
    `A new version (${version}) is available. Update now?`,
    { title: "Update Available", kind: "info", okLabel: "Update", cancelLabel: "Later" },
  );
  if (!yes) return true;
  await invoke("install_native_update", { version });
  await exit(0);
  return true;
}

async function applyPlugin(): Promise<boolean> {
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
}

export async function checkForUpdate(): Promise<UpdateCheckResult> {
  const source = await getUpdateSource();

  if (source === "appimage" || source === "unknown") {
    try {
      const update = await check();
      if (update) return { available: true, message: `v${update.version}` };
    } catch {
      // ignore
    }
    return { available: false, message: "" };
  }

  // native source
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
    // ignore
  }
  return { available: false, message: "" };
}

export async function performUpdate(): Promise<boolean> {
  const source = await getUpdateSource();

  if (source === "appimage" || source === "unknown") {
    try {
      return await applyPlugin();
    } catch {
      return false;
    }
  }

  // native source
  try {
    const info = await invoke<{
      available: boolean;
      current: string;
      latest: string;
      error: string | null;
    }>("check_native_update");
    if (info.error || !info.available) return false;
    return await applyNative(info.latest);
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