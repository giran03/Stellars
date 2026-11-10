<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import StudioIcon from "./StudioIcon.svelte";

  interface BackupInfo {
    name: string;
    kind: "manual" | "auto" | "prerestore";
    createdAt: string;
    sizeBytes: number;
  }

  interface Props {
    native: boolean;
    onNotify: (msg: string) => void;
    /** Called after a backup was applied so the app can reload the router state. */
    onRestored: () => void;
  }

  let { native, onNotify, onRestored }: Props = $props();

  const KIND_LABELS: Record<BackupInfo["kind"], string> = {
    manual: "Manual",
    auto: "Automatic",
    prerestore: "Before restore",
  };

  let backups = $state<BackupInfo[]>([]);
  let busy = $state(false);
  let error = $state<string | null>(null);
  /** `${action}:${name}` of the button waiting for its confirming second click. */
  let armed = $state<string | null>(null);
  let armTimer: ReturnType<typeof setTimeout> | undefined;

  async function refresh() {
    if (!native) return;
    try {
      backups = await invoke<BackupInfo[]>("list_backups");
    } catch (err) {
      error = String(err);
    }
  }

  async function run(task: () => Promise<void>) {
    if (busy) return;
    busy = true;
    error = null;
    try {
      await task();
      await refresh();
    } catch (err) {
      error = String(err);
    } finally {
      busy = false;
    }
  }

  const createBackup = () =>
    run(async () => {
      await invoke("create_backup");
      onNotify("Backup saved");
    });

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

  const restore = (name: string) =>
    confirmThen(`restore:${name}`, async () => {
      await invoke("restore_backup", { name });
      onNotify("Backup restored");
      onRestored();
    });

  const remove = (name: string) =>
    confirmThen(`delete:${name}`, async () => {
      await invoke("delete_backup", { name });
    });

  const openFolder = () => run(() => invoke("open_backups_folder"));

  function formatSize(bytes: number): string {
    return bytes < 1024 ? `${bytes} B` : `${(bytes / 1024).toFixed(1)} KB`;
  }

  onMount(() => {
    void refresh();
    return () => clearTimeout(armTimer);
  });
</script>

<div class="setting-row backups-head" class:is-disabled={!native}>
  <div class="head-copy">
    <strong>Backups</strong>
    <small>Saves your routing, EQ, devices and app settings. A backup is also made automatically once a day and before every restore; the newest 5 of each are kept, and your manual backups are never removed{#if !native} · Desktop app only{/if}.</small>
  </div>
  <div class="head-actions">
    <button type="button" class="cable-install-button" disabled={!native || busy} onclick={createBackup}>
      <StudioIcon name="download" size={14} /> Back up now
    </button>
    <button type="button" class="quiet-button" disabled={!native || busy} onclick={openFolder}>
      <StudioIcon name="folder" size={14} /> Open folder
    </button>
  </div>
</div>

{#if error}<p class="backup-error" role="alert">{error}</p>{/if}

{#if native}
  {#if backups.length === 0}
    <p class="backup-empty">No backups yet.</p>
  {:else}
    <ul class="backup-list">
      {#each backups as b (b.name)}
        <li>
          <div class="meta">
            <strong>{b.createdAt || b.name}</strong>
            <small>{KIND_LABELS[b.kind]} · {formatSize(b.sizeBytes)}</small>
          </div>
          <div class="row-actions">
            <button type="button" class="quiet-button" class:danger={armed === `restore:${b.name}`} disabled={busy} onclick={() => restore(b.name)}>
              <StudioIcon name="reset" size={13} />
              {armed === `restore:${b.name}` ? "Click again to restore" : "Restore"}
            </button>
            <button type="button" class="quiet-button" class:danger={armed === `delete:${b.name}`} disabled={busy} aria-label={`Delete backup ${b.createdAt}`} onclick={() => remove(b.name)}>
              {armed === `delete:${b.name}` ? "Confirm" : "Delete"}
            </button>
          </div>
        </li>
      {/each}
    </ul>
  {/if}
{/if}

<style>
  .setting-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 20px;
    padding: 13px 14px;
    border-radius: 11px;
    background: var(--panel-raised);
    border: 1px solid var(--line);
    margin-bottom: 6px;
  }
  .setting-row.is-disabled { opacity: 0.55; }
  .head-copy { display: flex; flex-direction: column; gap: 3px; min-width: 0; }
  .head-copy strong { font-size: 12px; font-weight: 600; }
  .head-copy small { font-size: 10.5px; line-height: 1.5; color: var(--text-muted); }
  .head-actions, .row-actions { display: flex; gap: 8px; flex-shrink: 0; flex-wrap: wrap; }
  .backup-error { margin: 0 0 6px; padding: 10px 12px; border-radius: 9px; font-size: 11px; color: #ffc4d7; background: #f0a7c61a; border: 1px solid #f0a7c633; }
  :global(:root[data-theme="light"]) .backup-error { color: #9c2f58; }
  .backup-empty { margin: 4px 2px; font-size: 11px; color: var(--text-muted); }
  .backup-list { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 6px; max-height: 220px; overflow-y: auto; }
  .backup-list li { display: flex; align-items: center; justify-content: space-between; gap: 14px; padding: 10px 14px; border-radius: 10px; background: var(--panel-inset); border: 1px solid var(--line); }
  .meta { display: flex; flex-direction: column; gap: 2px; min-width: 0; }
  .meta strong { font-size: 12px; font-weight: 600; font-variant-numeric: tabular-nums; }
  .meta small { font-size: 10px; color: var(--text-muted); }
  :global(.backup-list .quiet-button.danger) { background: #f08c7f26; color: #ffb4aa; }
  :global(:root[data-theme="light"] .backup-list .quiet-button.danger) { color: #a3342a; }
</style>
