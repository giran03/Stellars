<script lang="ts">
  import StudioIcon from "./StudioIcon.svelte";
  import type { InterfaceMode } from "../appSettings.svelte";

  interface Props {
    open: boolean;
    onchoose: (mode: InterfaceMode) => void;
    ondismiss: () => void;
  }

  let { open, onchoose, ondismiss }: Props = $props();
</script>

{#if open}
  <aside class="mode-prompt" aria-label="Choose interface mode">
    <button type="button" class="close" aria-label="Keep Simple and close" data-tooltip="Keep Simple" onclick={ondismiss}>
      <StudioIcon name="close" size={14} />
    </button>
    <strong>How much do you want to see?</strong>
    <p>Simple shows ready-made presets. Advanced shows every control. Your sound stays the same either way.</p>
    <div class="actions">
      <button type="button" class="primary" onclick={() => onchoose("simple")}>Keep it simple</button>
      <button type="button" onclick={() => onchoose("advanced")}>Show everything</button>
    </div>
    <small>Change it any time in Settings → Appearance.</small>
  </aside>
{/if}

<style>
  .mode-prompt { position: fixed; right: 20px; bottom: 56px; z-index: 900; width: 290px; padding: 16px 16px 14px; border-radius: 14px; background: var(--panel-raised); color: var(--text); border: 1px solid var(--line-strong); box-shadow: 0 18px 44px rgba(0, 0, 0, 0.45); display: flex; flex-direction: column; gap: 8px; animation: rise .25s ease-out; }
  strong { font-size: 13px; padding-right: 22px; }
  p { margin: 0; font-size: 11.5px; line-height: 1.55; color: var(--text-secondary); }
  small { font-size: 10px; color: var(--text-muted); }
  .actions { display: flex; gap: 8px; margin-top: 4px; }
  .actions button { flex: 1; min-height: 32px; border-radius: 8px; border: 1px solid var(--line-strong); background: var(--control); color: var(--text); font: inherit; font-size: 11px; cursor: pointer; }
  .actions .primary { background: var(--violet); color: var(--on-violet); border-color: transparent; font-weight: 600; }
  .close { position: absolute; top: 8px; right: 8px; display: grid; place-items: center; width: 24px; height: 24px; border: 0; border-radius: 6px; background: transparent; color: var(--text-muted); cursor: pointer; }
  .close:hover { background: var(--control); color: var(--text); }
  @keyframes rise { from { opacity: 0; transform: translateY(8px); } }
  @media (prefers-reduced-motion: reduce) { .mode-prompt { animation: none; } }
</style>
