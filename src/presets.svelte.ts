import { invoke } from "@tauri-apps/api/core";
import type { BusId, RouterConfig } from "./types";

/** Mirrors `PresetStore` in src-tauri/src/state/presets.rs (camelCase over IPC). */
export interface SceneSummary {
  id: string;
  name: string;
  autoApps: string[];
}

export interface OutputProfileSummary {
  id: string;
  name: string;
  masterOutputDeviceId: string | null;
}

export type UserPresetKind = "eq" | "noise" | "gate" | "comp" | "deEsser" | "limiter" | "recipe";

/** A preset the user saved; `data` is shaped per kind by the interface. */
export interface UserPreset {
  id: string;
  kind: UserPresetKind;
  bus: BusId;
  name: string;
  data: unknown;
}

interface PresetStorePayload {
  userPresets?: UserPreset[];
  scenes: SceneSummary[];
  outputProfiles: OutputProfileSummary[];
  activeSceneId: string | null;
  activeProfileId: string | null;
}

export const presetStore = $state<PresetStorePayload>({
  userPresets: [],
  scenes: [],
  outputProfiles: [],
  activeSceneId: null,
  activeProfileId: null,
});

function assign(next: PresetStorePayload) {
  presetStore.userPresets = next.userPresets ?? [];
  presetStore.scenes = next.scenes ?? [];
  presetStore.outputProfiles = next.outputProfiles ?? [];
  presetStore.activeSceneId = next.activeSceneId ?? null;
  presetStore.activeProfileId = next.activeProfileId ?? null;
}

const isNative = () => typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
const LOCAL_KEY = "stellars-user-presets";

function readLocalUserPresets(): UserPreset[] {
  try {
    const parsed = JSON.parse(localStorage.getItem(LOCAL_KEY) ?? "[]");
    return Array.isArray(parsed) ? parsed : [];
  } catch {
    return [];
  }
}

function writeLocalUserPresets(list: UserPreset[]) {
  try {
    localStorage.setItem(LOCAL_KEY, JSON.stringify(list));
  } catch {
    /* storage unavailable: keep in memory only */
  }
}

export async function loadPresets(): Promise<void> {
  if (!isNative()) {
    presetStore.userPresets = readLocalUserPresets();
    return;
  }
  try {
    assign(await invoke<PresetStorePayload>("get_presets"));
  } catch {
    /* presets unavailable right now */
  }
}

/** Saved presets of one kind for one channel, in the order they were created. */
export function userPresetsFor(kind: UserPresetKind, bus: BusId): UserPreset[] {
  return (presetStore.userPresets ?? []).filter((p) => p.kind === kind && p.bus === bus);
}

/** Saves (or replaces, by name) a user preset. Works in the browser preview too. */
export async function saveUserPreset(kind: UserPresetKind, bus: BusId, name: string, data: unknown): Promise<void> {
  if (isNative()) {
    assign(await invoke<PresetStorePayload>("save_user_preset", { kind, bus, name, data }));
    return;
  }
  const clean = name.trim().replace(/\s+/g, " ").slice(0, 40);
  if (!clean) throw new Error("Give it a name first");
  const list = readLocalUserPresets();
  const at = list.findIndex((p) => p.kind === kind && p.bus === bus && p.name.toLowerCase() === clean.toLowerCase());
  const entry: UserPreset = { id: at >= 0 ? list[at].id : "u-" + Date.now().toString(36) + Math.random().toString(36).slice(2, 6), kind, bus, name: clean, data };
  if (at >= 0) list[at] = entry;
  else list.push(entry);
  writeLocalUserPresets(list);
  presetStore.userPresets = list;
}

export async function deleteUserPreset(id: string): Promise<void> {
  if (isNative()) {
    assign(await invoke<PresetStorePayload>("delete_user_preset", { id }));
    return;
  }
  const list = readLocalUserPresets().filter((p) => p.id !== id);
  writeLocalUserPresets(list);
  presetStore.userPresets = list;
}

export async function saveScene(name: string, autoApps: string[] = []): Promise<void> {
  assign(await invoke<PresetStorePayload>("save_scene", { name, autoApps }));
}

export async function updateScene(
  id: string,
  patch: { name?: string; autoApps?: string[]; recapture?: boolean }
): Promise<void> {
  assign(await invoke<PresetStorePayload>("update_scene", { id, ...patch }));
}

export async function deleteScene(id: string): Promise<void> {
  assign(await invoke<PresetStorePayload>("delete_scene", { id }));
}

/** Applies a scene live and returns the resulting config. */
export async function applyScene(id: string): Promise<RouterConfig> {
  const cfg = await invoke<RouterConfig>("apply_scene", { id });
  await loadPresets();
  return cfg;
}

export async function saveOutputProfile(name: string): Promise<void> {
  assign(await invoke<PresetStorePayload>("save_output_profile", { name }));
}

export async function deleteOutputProfile(id: string): Promise<void> {
  assign(await invoke<PresetStorePayload>("delete_output_profile", { id }));
}

/** Switches device, master volume and EQ (restarts the engine) and returns the new config. */
export async function applyOutputProfile(id: string): Promise<RouterConfig> {
  const cfg = await invoke<RouterConfig>("apply_output_profile", { id });
  await loadPresets();
  return cfg;
}

export interface ImportSummary {
  userPresets: number;
  scenes: number;
  outputProfiles: number;
}

/** Writes all presets, scenes and output profiles to a file and shows it in Explorer. Returns the path. */
export const exportPresetsFile = () => invoke<string>("export_presets_file");

/** All presets, scenes and output profiles as shareable JSON text. */
export const exportPresetsText = () => invoke<string>("export_presets");

/** Adds the contents of a shared file; imported items never replace existing scenes or profiles. */
export async function importPresets(json: string): Promise<ImportSummary> {
  const [store, summary] = await invoke<[PresetStorePayload, ImportSummary]>("import_presets", { json });
  assign(store);
  return summary;
}

/** Parses "cs2.exe, valorant.exe" style input into a clean list. */
export function parseAppList(text: string): string[] {
  return text
    .split(/[,;\n]/)
    .map((s) => s.trim().toLowerCase())
    .filter(Boolean);
}
