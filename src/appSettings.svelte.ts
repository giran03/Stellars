import { invoke } from "@tauri-apps/api/core";

export type CloseBehavior = "tray" | "quit";
export type InterfaceMode = "simple" | "advanced";

/** Mirrors `AppSettings` in src-tauri/src/state/app_settings.rs (camelCase over IPC). */
export interface AppSettings {
  launchOnStartup: boolean;
  startMinimized: boolean;
  closeBehavior: CloseBehavior;
  minimizeToTray: boolean;
  alwaysOnTop: boolean;
  confirmBeforeReset: boolean;
  showNotifications: boolean;
  showTooltips: boolean;
  reduceMotion: boolean;
  compactMode: boolean;
  autoReconnectDevices: boolean;
  /** Slow the live meters when unfocused and stop them while minimized or in the tray. */
  saveResourcesInBackground: boolean;
  onboardingCompleted: boolean;
  autoCheckUpdates: boolean;
  lastUpdateCheck: string | null;
  interfaceMode: InterfaceMode;
  /** False until the user picks a mode in the wizard or the side prompt. */
  interfaceModeChosen: boolean;
}

export const DEFAULT_APP_SETTINGS: AppSettings = {
  launchOnStartup: false,
  startMinimized: false,
  closeBehavior: "tray",
  minimizeToTray: false,
  alwaysOnTop: false,
  confirmBeforeReset: true,
  showNotifications: true,
  showTooltips: true,
  reduceMotion: false,
  compactMode: false,
  autoReconnectDevices: true,
  saveResourcesInBackground: true,
  onboardingCompleted: false,
  autoCheckUpdates: true,
  lastUpdateCheck: null,
  interfaceMode: "simple",
  interfaceModeChosen: false,
};

const STORAGE_KEY = "stellars-app-settings";

export const appSettings = $state<AppSettings>({ ...DEFAULT_APP_SETTINGS });

export const isNativeApp = (): boolean =>
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

function assign(next: Partial<AppSettings>) {
  Object.assign(appSettings, next);
}

/** Reflects settings that are purely presentational onto the document. */
export function applyPresentationSettings() {
  const root = document.documentElement;
  root.dataset.tooltips = appSettings.showTooltips ? "on" : "off";
  root.dataset.motion = appSettings.reduceMotion ? "reduced" : "full";
  root.dataset.compact = appSettings.compactMode ? "on" : "off";
  root.dataset.mode = appSettings.interfaceMode;
}

function readLocal(): Partial<AppSettings> {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    return raw ? (JSON.parse(raw) as Partial<AppSettings>) : {};
  } catch {
    return {};
  }
}

function writeLocal() {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify({ ...appSettings }));
  } catch {
    /* storage unavailable: keep in-memory only */
  }
}

export async function loadAppSettings(): Promise<void> {
  if (isNativeApp()) {
    try {
      assign(await invoke<AppSettings>("get_app_settings"));
    } catch (err) {
      console.error("Could not load app settings:", err);
    }
  } else {
    assign(readLocal());
  }
  applyPresentationSettings();
}

/**
 * Optimistically applies `patch`, persists it, and rolls back (rethrowing) if the desktop
 * engine rejects it — e.g. when the Windows startup registry key cannot be written.
 */
export async function updateAppSettings(patch: Partial<AppSettings>): Promise<void> {
  const previous = { ...appSettings };
  assign(patch);
  applyPresentationSettings();
  if (!isNativeApp()) {
    writeLocal();
    return;
  }
  try {
    assign(await invoke<AppSettings>("set_app_settings", { settings: { ...appSettings } }));
  } catch (err) {
    assign(previous);
    applyPresentationSettings();
    throw err;
  }
}

/** Restores defaults but keeps the wizard from reappearing and the real startup-registry state. */
export async function resetAppSettings(): Promise<void> {
  await updateAppSettings({
    ...DEFAULT_APP_SETTINGS,
    onboardingCompleted: true,
    interfaceMode: appSettings.interfaceMode,
    interfaceModeChosen: appSettings.interfaceModeChosen,
  });
}
