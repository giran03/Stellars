<script lang="ts">
  import { onMount } from 'svelte';
  import StudioIcon from './StudioIcon.svelte';
  import type { AudioEndpointInfo } from '../types';

  interface Props {
    open: boolean;
    /** "output" lists playback devices (monitor output); "input" lists microphones. */
    kind: 'output' | 'input';
    endpoints: AudioEndpointInfo[];
    /** The saved device id, or null when Stellars follows the Windows default. */
    currentId: string | null;
    onSelect: (id: string | null) => void;
    onClose: () => void;
    onRefresh?: () => void;
  }

  let { open, kind, endpoints, currentId, onSelect, onClose, onRefresh }: Props = $props();

  const flow = $derived(kind === 'output' ? 'Render' : 'Capture');
  const list = $derived(endpoints.filter((e) => e.flow === flow && !(kind === 'output' && e.isDummySinkCandidate)));
  const systemDefault = $derived(list.find((e) => e.isDefault)?.name);
  // Row 0 is "Windows default" (null); the rest are devices.
  const rows = $derived([
    { id: null as string | null, name: 'Windows default', note: systemDefault },
    ...list.map((e) => ({ id: e.id as string | null, name: e.name, note: e.isDefault ? 'Default' : undefined })),
  ]);
  let active = $state(0);
  let listEl = $state<HTMLDivElement>();

  const title = $derived(kind === 'output' ? 'Monitor output device' : 'Microphone device');
  const hint = $derived(
    kind === 'output'
      ? 'Where you hear the mix: your headphones or speakers.'
      : 'The physical microphone Stellars processes.'
  );

  $effect(() => {
    if (!open) return;
    const at = rows.findIndex((r) => r.id === currentId);
    active = at >= 0 ? at : 0;
    document.body.classList.add('has-active-modal');
    onRefresh?.();
    queueMicrotask(() => listEl?.focus());
    return () => document.body.classList.remove('has-active-modal');
  });

  function choose(i: number) {
    if (rows[i].id !== currentId) onSelect(rows[i].id);
    onClose();
  }

  function scrollActive() {
    queueMicrotask(() => listEl?.querySelector<HTMLElement>(`[data-row="${active}"]`)?.scrollIntoView({ block: 'nearest' }));
  }

  function onKey(e: KeyboardEvent) {
    if (!open) return;
    if (e.key === 'Escape') { e.preventDefault(); e.stopPropagation(); onClose(); }
    else if (e.key === 'ArrowDown') { e.preventDefault(); active = (active + 1) % rows.length; scrollActive(); }
    else if (e.key === 'ArrowUp') { e.preventDefault(); active = (active - 1 + rows.length) % rows.length; scrollActive(); }
    else if (e.key === 'Enter') { e.preventDefault(); choose(active); }
  }

  onMount(() => {
    window.addEventListener('keydown', onKey, true);
    return () => {
      window.removeEventListener('keydown', onKey, true);
      document.body.classList.remove('has-active-modal');
    };
  });
</script>

{#if open}
  <div class="modal-backdrop device-backdrop" role="presentation" onclick={(e) => { if (e.target === e.currentTarget) onClose(); }}>
    <div class="device-modal" role="dialog" aria-modal="true" aria-labelledby="device-modal-title" tabindex="-1">
      <header>
        <span class="device-icon"><StudioIcon name={kind === 'output' ? 'headphones' : 'mic'} size={18} /></span>
        <div>
          <h3 id="device-modal-title">{title}</h3>
          <p>{hint}</p>
        </div>
        <button type="button" class="device-close" aria-label="Close" onclick={onClose}>×</button>
      </header>
      <div class="device-list" role="listbox" aria-label={title} tabindex="0" bind:this={listEl}>
        {#each rows as row, i (row.id ?? 'default')}
          <button
            type="button"
            role="option"
            class="device-row"
            class:active={active === i}
            class:current={row.id === currentId}
            aria-selected={row.id === currentId}
            data-row={i}
            tabindex="-1"
            onpointerenter={() => (active = i)}
            onclick={() => choose(i)}
          >
            <span class="device-name">{row.name}{#if row.id === null && row.note}<small>{row.note}</small>{/if}</span>
            {#if row.id !== null && row.note}<span class="device-pill">{row.note}</span>{/if}
            {#if row.id === currentId}<span class="device-check"><StudioIcon name="check" size={15} /></span>{/if}
          </button>
        {/each}
        {#if list.length === 0}<p class="device-empty">No {kind === 'output' ? 'playback' : 'recording'} devices found.</p>{/if}
      </div>
      <footer>Changing the device restarts audio for a moment.</footer>
    </div>
  </div>
{/if}

<style>
  .device-backdrop {
    position: fixed; inset: 0; z-index: 12000; display: grid; place-items: center;
    background: rgba(8, 7, 12, 0.55);
  }
  .device-modal {
    width: min(440px, calc(100vw - 32px)); max-height: min(520px, calc(100vh - 48px)); display: flex; flex-direction: column;
    border-radius: 16px; background: var(--panel-raised, #242328); color: var(--text, #f4f3f7);
    border: 1px solid var(--line-strong, rgba(255,255,255,.16)); box-shadow: 0 24px 60px rgba(0,0,0,.45);
    animation: device-pop 180ms cubic-bezier(.22,1,.36,1); outline: none;
  }
  header { display: flex; align-items: center; gap: 12px; padding: 16px 16px 12px; }
  header > div { flex: 1; min-width: 0; }
  .device-icon { width: 34px; height: 34px; display: grid; place-items: center; border-radius: 10px; background: var(--control, #29282e); color: var(--violet, #b8a1fa); }
  h3 { margin: 0; font-size: 14px; font-weight: 700; }
  p { margin: 2px 0 0; font-size: 11px; color: var(--text-muted, #a2a1ac); }
  .device-close { width: 28px; height: 28px; min-height: 0; padding: 0; border-radius: 8px; background: transparent; color: var(--text-muted, #a2a1ac); font-size: 18px; line-height: 1; }
  .device-close:hover { background: var(--control, #29282e); color: var(--text, #f4f3f7); }
  .device-list { overflow-y: auto; padding: 4px 8px 8px; display: flex; flex-direction: column; gap: 2px; outline: none; }
  .device-row {
    display: flex; align-items: center; gap: 10px; width: 100%; min-height: 40px; padding: 8px 10px; text-align: left;
    border-radius: 9px; background: transparent; color: var(--text-secondary, #c7c5ce); font-size: 12px; font-weight: 500;
  }
  .device-row.active { background: var(--control, #29282e); color: var(--text, #f4f3f7); }
  .device-row.current { color: var(--text, #f4f3f7); }
  .device-name { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .device-name small { margin-left: 8px; font-size: 10px; color: var(--text-muted, #a2a1ac); font-weight: 400; }
  .device-pill { padding: 2px 8px; border-radius: 999px; font-size: 10px; color: var(--text-muted, #a2a1ac); background: var(--control, #29282e); }
  .device-check { color: var(--violet, #b8a1fa); display: grid; }
  .device-empty { padding: 14px 10px; }
  footer { padding: 10px 16px 14px; font-size: 10px; color: var(--text-muted, #a2a1ac); border-top: 1px solid var(--line, rgba(255,255,255,.08)); }
  /* Motion only: the dialog is never left see-through if the webview skips frames. */
  @keyframes device-pop { from { transform: translateY(6px) scale(.98); } to { transform: none; } }
  @media (prefers-reduced-motion: reduce) { .device-modal { animation: none; } }
</style>
