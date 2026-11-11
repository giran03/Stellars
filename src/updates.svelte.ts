import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

/** Mirrors `Release` and `Status` in src-tauri/src/update.rs (camelCase over IPC). */
export interface UpdateRelease {
  version: string;
  setupName: string;
  notes: string | null;
  pageUrl: string;
}

export type UpdateStatus =
  | { kind: "idle" }
  | { kind: "checking" }
  | { kind: "upToDate" }
  | { kind: "available"; release: UpdateRelease }
  | { kind: "downloading"; version: string }
  | { kind: "ready"; version: string }
  | { kind: "installing"; version: string }
  | { kind: "installed"; version: string }
  | { kind: "installFailed"; version: string; message: string }
  | { kind: "failed"; message: string };

export const updateState = $state<{ status: UpdateStatus }>({ status: { kind: "idle" } });

const isNative = () => typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

/** The version the user was last told about, so a notice shows once per release. */
let announcedVersion = "";
let started = false;

/**
 * Follows the update status kept by the desktop app. `onAvailable` runs once for each new
 * version found, so the window can tell the user without them opening Settings.
 */
export async function startUpdates(onAvailable: (release: UpdateRelease) => void): Promise<void> {
  if (!isNative() || started) return;
  started = true;
  const apply = (status: UpdateStatus) => {
    updateState.status = status;
    if (status.kind === "available" && status.release.version !== announcedVersion) {
      announcedVersion = status.release.version;
      onAvailable(status.release);
    }
  };
  try {
    await listen<UpdateStatus>("update-status", (event) => apply(event.payload));
    apply(await invoke<UpdateStatus>("get_update_status"));
  } catch {
    /* updates unavailable right now */
  }
}

export const checkForUpdates = () => invoke("check_for_updates").catch(() => {});
export const downloadUpdate = () => invoke("download_update").catch(() => {});
export const installUpdate = () => invoke("install_update").catch(() => {});
