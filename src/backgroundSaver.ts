// Saves resources while the window is in the background. The audio engine is never affected; this
// only changes how much work the UI does:
//   active  - focused: meters at full rate
//   idle    - visible but unfocused for a few seconds: slower meters, decorative animations paused
//   paused  - minimized or hidden: meters stopped, animations paused
// Anything that becomes active again snaps back immediately.

import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";

export type ActivityMode = "active" | "idle" | "paused";

const METER_HZ: Record<ActivityMode, number> = { active: 60, idle: 20, paused: 0 };
/** How long the window must stay unfocused before it counts as idle. */
const IDLE_AFTER_MS = 5000;

const isNative = () => typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

export interface BackgroundSaver {
  setEnabled(enabled: boolean): void;
  dispose(): void;
}

export function installBackgroundSaver(): BackgroundSaver {
  let enabled = true;
  let focused = document.hasFocus();
  let mode: ActivityMode | null = null;
  let idleTimer: ReturnType<typeof setTimeout> | undefined;
  let unlistenFocus: (() => void) | undefined;
  let disposed = false;

  const decide = (idleElapsed: boolean): ActivityMode => {
    if (!enabled) return "active";
    if (document.hidden) return "paused";
    if (focused) return "active";
    return idleElapsed ? "idle" : "active";
  };

  const apply = (next: ActivityMode) => {
    if (next === mode) return;
    mode = next;
    document.documentElement.dataset.activity = next;
    if (isNative()) invoke("set_meter_rate", { hz: METER_HZ[next] }).catch(() => {});
  };

  const evaluate = () => {
    clearTimeout(idleTimer);
    const immediate = decide(false);
    apply(immediate);
    // Unfocused but visible: wait before slowing down so a quick glance away costs nothing.
    if (enabled && !focused && !document.hidden) {
      idleTimer = setTimeout(() => apply(decide(true)), IDLE_AFTER_MS);
    }
  };

  const onFocus = () => { focused = true; evaluate(); };
  const onBlur = () => { focused = false; evaluate(); };
  window.addEventListener("focus", onFocus);
  window.addEventListener("blur", onBlur);
  document.addEventListener("visibilitychange", evaluate);

  if (isNative()) {
    // The native focus event is more reliable than DOM focus for a frameless window.
    getCurrentWindow()
      .onFocusChanged(({ payload }) => { focused = payload; evaluate(); })
      .then((off) => { if (disposed) off(); else unlistenFocus = off; })
      .catch(() => {});
  }

  evaluate();

  return {
    setEnabled(next) {
      if (next === enabled) return;
      enabled = next;
      evaluate();
    },
    dispose() {
      disposed = true;
      clearTimeout(idleTimer);
      window.removeEventListener("focus", onFocus);
      window.removeEventListener("blur", onBlur);
      document.removeEventListener("visibilitychange", evaluate);
      unlistenFocus?.();
      document.documentElement.removeAttribute("data-activity");
      if (isNative()) invoke("set_meter_rate", { hz: METER_HZ.active }).catch(() => {});
    },
  };
}
