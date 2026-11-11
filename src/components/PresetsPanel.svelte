<script lang="ts">
  import { onMount } from "svelte";
  import StudioIcon from "./StudioIcon.svelte";
  import type { RouterConfig } from "../types";
  import {
    applyOutputProfile,
    applyScene,
    deleteOutputProfile,
    deleteScene,
    exportPresetsFile,
    exportPresetsText,
    importPresets,
    loadPresets,
    parseAppList,
    presetStore,
    saveOutputProfile,
    saveScene,
    updateScene,
  } from "../presets.svelte";

  interface Props {
    native: boolean;
    onApplied: (config: RouterConfig) => void;
    onNotify: (msg: string) => void;
  }

  let { native, onApplied, onNotify }: Props = $props();

  let sceneName = $state("");
  let profileName = $state("");
  let error = $state<string | null>(null);
  let busy = $state(false);
  /** `${kind}:${id}` waiting for a confirming second click (delete / overwrite). */
  let armed = $state<string | null>(null);
  let armTimer: ReturnType<typeof setTimeout> | undefined;
  let editingApps = $state<string | null>(null);
  let appsDraft = $state("");

  async function run(task: () => Promise<void>) {
    if (busy) return;
    busy = true;
    error = null;
    try {
      await task();
    } catch (err) {
      error = String(err);
    } finally {
      busy = false;
    }
  }

  function confirmThen(key: string, task: () => Promise<void>) {
    if (armed !== key) {
      armed = key;
      clearTimeout(armTimer);
      armTimer = setTimeout(() => (armed = null), 4000);
      return;
    }
    armed = null;
    void run(task);
  }

  const addScene = () =>
    run(async () => {
      await saveScene(sceneName);
      sceneName = "";
      onNotify("Scene saved");
    });

  const addProfile = () =>
    run(async () => {
      await saveOutputProfile(profileName);
      profileName = "";
      onNotify("Output profile saved");
    });

  const activateScene = (id: string, name: string) =>
    run(async () => {
      onApplied(await applyScene(id));
      onNotify(`Scene: ${name}`);
    });

  const activateProfile = (id: string, name: string) =>
    run(async () => {
      onApplied(await applyOutputProfile(id));
      onNotify(`Output: ${name}`);
    });

  let importInput = $state<HTMLInputElement>();

  const exportToFile = () =>
    run(async () => {
      await exportPresetsFile();
      onNotify("Exported · the file is selected in Explorer");
    });

  const copyExport = () =>
    run(async () => {
      await navigator.clipboard.writeText(await exportPresetsText());
      onNotify("Export copied to the clipboard");
    });

  const importFromFile = (file: File | undefined) =>
    run(async () => {
      if (!file) return;
      const summary = await importPresets(await file.text());
      const parts = [
        summary.scenes && `${summary.scenes} scene${summary.scenes === 1 ? "" : "s"}`,
        summary.outputProfiles && `${summary.outputProfiles} output profile${summary.outputProfiles === 1 ? "" : "s"}`,
        summary.userPresets && `${summary.userPresets} sound preset${summary.userPresets === 1 ? "" : "s"}`,
      ].filter(Boolean);
      onNotify(`Imported ${parts.join(", ") || "nothing new"}`);
    }).finally(() => {
      if (importInput) importInput.value = "";
    });

  function startEditApps(id: string, apps: string[]) {
    editingApps = id;
    appsDraft = apps.join(", ");
  }

  const saveApps = (id: string) =>
    run(async () => {
      await updateScene(id, { autoApps: parseAppList(appsDraft) });
      editingApps = null;
    });

  onMount(() => {
    void loadPresets();
    return () => clearTimeout(armTimer);
  });
</script>

{#if !native}
  <p class="presets-note">Scenes and output profiles are saved by the desktop engine, so they’re only available in the desktop app.</p>
{:else}
  {#if error}<div class="presets-alert" role="alert">{error}</div>{/if}

  <div class="presets-group">
    <h4>Scenes</h4>
    <p class="presets-note">A scene remembers your whole mix: faders, EQ, dynamics, routing, per-app volumes, stream mix and ducking. Devices are never changed, so switching is instant. Add an app to a scene to apply it automatically when that app gets focus.</p>
    <form class="presets-add" onsubmit={(e) => { e.preventDefault(); void addScene(); }}>
      <input type="text" maxlength="40" placeholder="Name the current mix, e.g. Late night" aria-label="New scene name" bind:value={sceneName} />
      <button type="submit" class="cable-install-button" disabled={busy || !sceneName.trim()}><StudioIcon name="plus" size={14} /> Save scene</button>
    </form>
    {#if presetStore.scenes.length === 0}
      <p class="presets-empty">No scenes yet.</p>
    {:else}
      <ul class="presets-list">
        {#each presetStore.scenes as scene (scene.id)}
          <li class:active={presetStore.activeSceneId === scene.id}>
            <div class="row-main">
              <div class="row-title">
                <strong>{scene.name}</strong>
                {#if presetStore.activeSceneId === scene.id}<span class="badge">Active</span>{/if}
              </div>
              {#if editingApps === scene.id}
                <form class="apps-edit" onsubmit={(e) => { e.preventDefault(); void saveApps(scene.id); }}>
                  <input type="text" placeholder="cs2.exe, valorant.exe" aria-label={`Auto-apply apps for ${scene.name}`} bind:value={appsDraft} />
                  <button type="submit" class="quiet-button" disabled={busy}>Save</button>
                  <button type="button" class="quiet-button" onclick={() => (editingApps = null)}>Cancel</button>
                </form>
              {:else}
                <button type="button" class="apps-line" onclick={() => startEditApps(scene.id, scene.autoApps)} aria-label={`Edit auto-apply apps for ${scene.name}`}>
                  {scene.autoApps.length ? `Auto-applies for ${scene.autoApps.join(", ")}` : "Add auto-apply apps"}
                </button>
              {/if}
            </div>
            <div class="row-actions">
              <button type="button" class="quiet-button" disabled={busy} onclick={() => activateScene(scene.id, scene.name)}>Apply</button>
              <button
                type="button"
                class="quiet-button"
                class:danger={armed === `overwrite:${scene.id}`}
                disabled={busy}
                data-tooltip="Replace this scene with the current mix"
                onclick={() => confirmThen(`overwrite:${scene.id}`, async () => { await updateScene(scene.id, { recapture: true }); onNotify("Scene updated"); })}
              >{armed === `overwrite:${scene.id}` ? "Confirm" : "Update"}</button>
              <button
                type="button"
                class="quiet-button"
                class:danger={armed === `delete:${scene.id}`}
                disabled={busy}
                aria-label={`Delete scene ${scene.name}`}
                onclick={() => confirmThen(`delete:${scene.id}`, () => deleteScene(scene.id))}
              >{armed === `delete:${scene.id}` ? "Confirm" : "Delete"}</button>
            </div>
          </li>
        {/each}
      </ul>
    {/if}
  </div>

  <div class="presets-group">
    <h4>Output profiles</h4>
    <p class="presets-note">Save your monitor device, master volume and EQ for each way you listen (headphones, speakers). Switching restarts the audio engine, so sound pauses for a moment.</p>
    <form class="presets-add" onsubmit={(e) => { e.preventDefault(); void addProfile(); }}>
      <input type="text" maxlength="40" placeholder="Name the current output, e.g. Headphones" aria-label="New output profile name" bind:value={profileName} />
      <button type="submit" class="cable-install-button" disabled={busy || !profileName.trim()}><StudioIcon name="headphones" size={14} /> Save output</button>
    </form>
    {#if presetStore.outputProfiles.length === 0}
      <p class="presets-empty">No output profiles yet.</p>
    {:else}
      <ul class="presets-list">
        {#each presetStore.outputProfiles as profile (profile.id)}
          <li class:active={presetStore.activeProfileId === profile.id}>
            <div class="row-main">
              <div class="row-title">
                <strong>{profile.name}</strong>
                {#if presetStore.activeProfileId === profile.id}<span class="badge">Last used</span>{/if}
              </div>
            </div>
            <div class="row-actions">
              <button type="button" class="quiet-button" disabled={busy} onclick={() => activateProfile(profile.id, profile.name)}>Switch</button>
              <button
                type="button"
                class="quiet-button"
                class:danger={armed === `pdelete:${profile.id}`}
                disabled={busy}
                aria-label={`Delete output profile ${profile.name}`}
                onclick={() => confirmThen(`pdelete:${profile.id}`, () => deleteOutputProfile(profile.id))}
              >{armed === `pdelete:${profile.id}` ? "Confirm" : "Delete"}</button>
            </div>
          </li>
        {/each}
      </ul>
    {/if}
  </div>

  <div class="presets-group">
    <h4>Share</h4>
    <p class="presets-note">Export your scenes, output profiles and saved sound presets to a file, or import ones from a friend. Imported items are added next to yours and never replace them. Output devices from another PC are left unset.</p>
    <div class="row-actions share-actions">
      <button type="button" class="cable-install-button" disabled={busy} onclick={exportToFile}><StudioIcon name="folder" size={14} /> Export to file</button>
      <button type="button" class="quiet-button" disabled={busy} onclick={copyExport}><StudioIcon name="link" size={14} /> Copy export</button>
      <button type="button" class="quiet-button" disabled={busy} onclick={() => importInput?.click()}><StudioIcon name="plus" size={14} /> Import file…</button>
      <input bind:this={importInput} type="file" accept=".json,application/json" hidden onchange={(e) => importFromFile(e.currentTarget.files?.[0])} />
    </div>
  </div>
{/if}

<style>
  .share-actions { justify-content: flex-start; }
  .presets-group { display: flex; flex-direction: column; gap: 8px; margin-bottom: 22px; }
  .presets-group h4 { margin: 0; font-size: 10px; font-weight: 600; letter-spacing: 0.08em; text-transform: uppercase; color: var(--text-muted); }
  .presets-note { margin: 0; font-size: 11px; line-height: 1.55; color: var(--text-muted); }
  .presets-empty { margin: 2px 0; font-size: 11px; color: var(--text-muted); }
  .presets-alert { margin-bottom: 12px; padding: 10px 12px; border-radius: 9px; font-size: 11px; color: #ffc4d7; background: #f0a7c61a; border: 1px solid #f0a7c633; }
  :global(:root[data-theme="light"]) .presets-alert { color: #9c2f58; }

  .presets-add { display: flex; gap: 8px; }
  .presets-add input, .apps-edit input {
    flex: 1;
    min-width: 0;
    padding: 9px 12px;
    border-radius: 9px;
    border: 1px solid var(--line-strong);
    background: var(--panel-inset);
    color: var(--text);
    font: inherit;
    font-size: 12px;
  }
  .presets-add input:focus-visible, .apps-edit input:focus-visible { outline: 2px solid var(--violet); outline-offset: 1px; }

  .presets-list { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 6px; }
  .presets-list li { display: flex; align-items: center; justify-content: space-between; gap: 14px; padding: 12px 14px; border-radius: 11px; background: var(--panel-raised); border: 1px solid var(--line); }
  .presets-list li.active { border-color: color-mix(in srgb, var(--violet) 55%, transparent); }
  .row-main { display: flex; flex-direction: column; gap: 4px; min-width: 0; flex: 1; }
  .row-title { display: flex; align-items: center; gap: 8px; }
  .row-title strong { font-size: 12.5px; font-weight: 600; }
  .badge { padding: 2px 7px; border-radius: 5px; font-size: 9px; color: var(--teal); background: color-mix(in srgb, var(--teal) 14%, transparent); }
  .apps-line { align-self: flex-start; padding: 0; border: 0; background: transparent; color: var(--text-muted); font: inherit; font-size: 10.5px; cursor: pointer; text-align: left; overflow-wrap: anywhere; }
  .apps-line:hover { color: var(--violet); }
  .apps-edit { display: flex; gap: 6px; }
  .row-actions { display: flex; gap: 6px; flex-shrink: 0; flex-wrap: wrap; justify-content: flex-end; }
  .presets-list :global(.quiet-button.danger) { background: #f08c7f26; color: #ffb4aa; }
  :global(:root[data-theme="light"]) .presets-list :global(.quiet-button.danger) { color: #a3342a; }
  @media (max-width: 760px) { .presets-list li { flex-wrap: wrap; } .presets-add { flex-wrap: wrap; } }
</style>
