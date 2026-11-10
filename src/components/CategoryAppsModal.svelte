<script lang="ts">
  import { onMount } from 'svelte';
  import type { ActiveAudioProcess, PlaybackBusId } from '../types';
  import AppIcon from './AppIcon.svelte';
  import StudioIcon from './StudioIcon.svelte';
  import StudioSelect from './StudioSelect.svelte';

  const PLAYBACK_BUSES: PlaybackBusId[] = ['Game', 'Chat', 'Media', 'Aux'];

  interface Props {
    bus: PlaybackBusId | null;
    processes: ActiveAudioProcess[];
    accentColor: string;
    onClose: () => void;
    onRouteProcess: (pid: number, exeName: string, bus: PlaybackBusId) => void;
    onUnrouteProcess: (pid: number) => void;
  }

  let {
    bus,
    processes,
    accentColor,
    onClose,
    onRouteProcess,
    onUnrouteProcess,
  }: Props = $props();

  const busProcs = $derived(
    bus ? processes.filter(p => (p.assignedBus ?? 'Aux') === bus) : []
  );

  function handleKeydown(e: KeyboardEvent) {
    if (!bus) return;
    if (e.key === 'Escape') {
      e.stopPropagation();
      e.preventDefault();
      onClose();
      return;
    }
    if (['Space', 'PageUp', 'PageDown', 'End', 'Home'].includes(e.code)) {
      const target = e.target as HTMLElement | null;
      if (!target?.closest('.modal-body, input, textarea, select')) {
        e.preventDefault();
      }
    }
  }

  function handleBackdropClick(e: MouseEvent) {
    if (e.target === e.currentTarget) {
      e.preventDefault();
      e.stopPropagation();
      onClose();
    }
  }

  function handleBackdropPointer(e: PointerEvent) {
    if (e.target === e.currentTarget) {
      e.stopPropagation();
    }
  }

  $effect(() => {
    if (bus) {
      const prevOverflow = document.body.style.overflow;
      document.body.classList.add('has-active-modal');
      document.body.style.overflow = 'hidden';

      const handleWheel = (e: WheelEvent) => {
        const target = e.target as HTMLElement | null;
        if (target?.closest('.studio-select-menu')) {
          return;
        }
        const modalBody = target?.closest('.modal-body') as HTMLElement | null;
        if (!modalBody) {
          e.preventDefault();
          e.stopPropagation();
          return;
        }

        if (modalBody.scrollHeight <= modalBody.clientHeight) {
          e.preventDefault();
          return;
        }

        const atTop = modalBody.scrollTop <= 0;
        const atBottom = Math.ceil(modalBody.scrollTop + modalBody.clientHeight) >= modalBody.scrollHeight;
        if ((atTop && e.deltaY < 0) || (atBottom && e.deltaY > 0)) {
          e.preventDefault();
        }
      };

      const handleTouchMove = (e: TouchEvent) => {
        const target = e.target as HTMLElement | null;
        if (target?.closest('.studio-select-menu')) return;
        if (!target?.closest('.modal-body')) {
          e.preventDefault();
        }
      };

      window.addEventListener('wheel', handleWheel, { capture: true, passive: false });
      window.addEventListener('touchmove', handleTouchMove, { capture: true, passive: false });

      return () => {
        document.body.classList.remove('has-active-modal');
        document.body.style.overflow = prevOverflow;
        window.removeEventListener('wheel', handleWheel, { capture: true } as EventListenerOptions);
        window.removeEventListener('touchmove', handleTouchMove, { capture: true } as EventListenerOptions);
      };
    }
  });

  onMount(() => {
    window.addEventListener('keydown', handleKeydown, true);
    return () => {
      window.removeEventListener('keydown', handleKeydown, true);
      document.body.classList.remove('has-active-modal');
    };
  });
</script>

{#if bus}
  <!-- Backdrop overlay -->
  <div
    class="modal-backdrop"
    onclick={handleBackdropClick}
    onpointerdown={handleBackdropPointer}
    onpointerup={handleBackdropPointer}
    onpointermove={handleBackdropPointer}
    onwheel={e => {
      if (!(e.target as HTMLElement | null)?.closest('.modal-body')) {
        e.preventDefault();
        e.stopPropagation();
      }
    }}
    role="presentation"
  >
    <!-- Modal Dialog -->
    <div
      class="category-modal"
      style={`--modal-accent: ${accentColor};`}
      role="dialog"
      aria-modal="true"
      aria-labelledby="category-modal-title"
      tabindex="-1"
      onclick={e => e.stopPropagation()}
      onkeydown={e => e.stopPropagation()}
    >
      <header class="modal-header">
        <div class="modal-title-group">
          <span class="modal-bus-badge">
            <span class="modal-bus-dot" style={`background: ${accentColor}`}></span>
            {bus} channel
          </span>
          <h2 id="category-modal-title">{bus} Applications</h2>
          <span class="modal-subtitle">
            {busProcs.length} active app{busProcs.length === 1 ? '' : 's'} assigned to this channel
          </span>
        </div>
        <button
          type="button"
          class="modal-close-btn"
          aria-label="Close applications modal"
          onclick={onClose}
        >
          <StudioIcon name="close" size={16} />
        </button>
      </header>

      <div class="modal-body">
        {#if busProcs.length > 0}
          <div class="modal-app-list">
            {#each busProcs as proc (proc.pid)}
              <div class="modal-app-row">
                <AppIcon {proc} size={30} color={accentColor} />
                <div class="modal-app-info">
                  <strong title={proc.displayName}>{proc.displayName}</strong>
                  <span title={proc.exeName}>{proc.exeName} · {proc.pid === 0 ? 'System' : `PID ${proc.pid}`}</span>
                </div>

                <div class="modal-app-controls">
                  <div class="modal-route-select" style={`color: ${accentColor}`}>
                    <StudioSelect
                      aria-label={`Route ${proc.displayName}`}
                      value={proc.assignedBus ?? 'Aux'}
                      onchange={e => onRouteProcess(proc.pid, proc.exeName, e.currentTarget.value as PlaybackBusId)}
                    >
                      {#each PLAYBACK_BUSES as b}
                        <option value={b}>{b}</option>
                      {/each}
                    </StudioSelect>
                  </div>

                  <button
                    type="button"
                    class="modal-unroute-btn"
                    data-tooltip={`Reset ${proc.displayName} to Aux (default)`}
                    aria-label={`Unroute ${proc.displayName}`}
                    onclick={() => onUnrouteProcess(proc.pid)}
                  >
                    <StudioIcon name="reset" size={13} />
                  </button>
                </div>
              </div>
            {/each}
          </div>
        {:else}
          <div class="modal-empty-state">
            <StudioIcon name="search" size={24} />
            <p>No applications currently routed to {bus}.</p>
            <small>Drag an app onto the {bus} channel shelf to assign it here.</small>
          </div>
        {/if}
      </div>

      <footer class="modal-footer">
        <p class="modal-hint">
          <StudioIcon name="info" size={13} />
          Apps assigned to {bus} share its volume, EQ, and dynamics.
        </p>
        <button
          type="button"
          class="quiet-button modal-done-btn"
          onclick={onClose}
        >
          Done
        </button>
      </footer>
    </div>
  </div>
{/if}

<style>
  .modal-backdrop {
    position: fixed;
    inset: 0;
    background: rgba(10, 9, 13, 0.74);
    backdrop-filter: blur(10px);
    -webkit-backdrop-filter: blur(10px);
    z-index: 12000;
    display: grid;
    place-items: center;
    padding: 16px;
    animation: backdrop-fade 180ms ease-out;
  }

  @keyframes backdrop-fade {
    from { opacity: 0; }
    to { opacity: 1; }
  }

  .category-modal {
    width: 100%;
    max-width: 440px;
    max-height: 82vh;
    background: linear-gradient(165deg, #24222c, #19181e 85%);
    border: 1px solid color-mix(in srgb, var(--modal-accent, #b8a1fa) 32%, #ffffff14);
    border-radius: 18px;
    box-shadow: 0 24px 64px rgba(0, 0, 0, 0.72), 0 0 32px color-mix(in srgb, var(--modal-accent, #b8a1fa) 12%, transparent);
    display: flex;
    flex-direction: column;
    overflow: hidden;
    animation: modal-pop 220ms cubic-bezier(.22,1,.36,1);
  }

  @keyframes modal-pop {
    from {
      opacity: 0;
      transform: scale(0.95) translateY(6px);
    }
    to {
      opacity: 1;
      transform: scale(1) translateY(0);
    }
  }

  .modal-header {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 16px;
    padding: 20px 22px 16px;
    border-bottom: 1px solid #ffffff0c;
  }

  .modal-title-group {
    display: flex;
    flex-direction: column;
    gap: 4px;
    min-width: 0;
  }

  .modal-bus-badge {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: 10px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--modal-accent);
  }

  .modal-bus-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
  }

  .modal-title-group h2 {
    font-size: 17px;
    font-weight: 600;
    letter-spacing: -0.02em;
    color: #f5f2fa;
    margin: 0;
  }

  .modal-subtitle {
    font-size: 10px;
    color: #9e93ae;
  }

  .modal-close-btn {
    width: 30px;
    height: 30px;
    display: grid;
    place-items: center;
    background: #ffffff08;
    border: 0;
    border-radius: 8px;
    color: #b0a3c2;
    cursor: pointer;
    flex-shrink: 0;
    transition: background 150ms ease, color 150ms ease, transform 120ms ease;
  }

  .modal-close-btn:hover {
    background: #ffffff14;
    color: #f5f2fa;
    transform: scale(1.05);
  }

  .modal-close-btn:active {
    transform: scale(0.95);
  }

  .modal-body {
    padding: 12px 22px;
    overflow-y: auto;
    overscroll-behavior: contain;
    touch-action: pan-y;
    max-height: 50vh;
    scrollbar-width: thin;
    scrollbar-color: #3b3646 #19181e;
  }

  .modal-app-list {
    display: flex;
    flex-direction: column;
    gap: 7px;
  }

  .modal-app-row {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 9px 12px;
    background: #ffffff04;
    border: 1px solid #ffffff08;
    border-radius: 11px;
    transition: background 150ms ease, border-color 150ms ease;
  }

  .modal-app-row:hover {
    background: #ffffff09;
    border-color: color-mix(in srgb, var(--modal-accent) 24%, transparent);
  }

  .modal-app-info {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
    flex: 1;
  }

  .modal-app-info strong {
    font-size: 12px;
    font-weight: 600;
    color: #ede7f5;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .modal-app-info span {
    font-size: 9.5px;
    color: #9a8fad;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .modal-app-controls {
    display: flex;
    align-items: center;
    gap: 7px;
    flex-shrink: 0;
  }

  .modal-route-select {
    display: inline-flex;
    align-items: center;
  }

  .modal-route-select :global(.studio-select-trigger) {
    background: #2a2533;
    border: 1px solid #ffffff0d;
    border-radius: 7px;
    padding: 5px 9px 5px 10px;
    font-size: 10px;
    font-weight: 500;
    color: currentColor;
    min-width: 82px;
    display: inline-flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    transition: background 150ms ease, border-color 150ms ease;
  }

  .modal-route-select :global(.studio-select-trigger:hover) {
    background: #362f42;
    border-color: color-mix(in srgb, var(--modal-accent, #b8a1fa) 30%, transparent);
  }

  .modal-route-select :global(.studio-select-trigger[aria-expanded="true"]) {
    background: #393147;
    border-color: var(--modal-accent, #b8a1fa);
    box-shadow: 0 0 0 1px color-mix(in srgb, var(--modal-accent, #b8a1fa) 40%, transparent);
  }

  .modal-route-select :global(.studio-select-trigger svg) {
    flex-shrink: 0;
    opacity: 0.8;
    transition: color 150ms ease, opacity 150ms ease;
  }

  .modal-route-select :global(.studio-select-trigger[aria-expanded="true"] svg) {
    opacity: 1;
    color: var(--modal-accent, #b8a1fa);
  }

  .modal-unroute-btn {
    width: 28px;
    height: 28px;
    display: grid;
    place-items: center;
    background: #2a2533;
    border: 1px solid #ffffff0d;
    border-radius: 7px;
    color: #a496b8;
    cursor: pointer;
    transition: background 150ms ease, color 150ms ease;
  }

  .modal-unroute-btn:hover {
    background: #393047;
    color: #ff9cb7;
  }

  .modal-empty-state {
    display: flex;
    flex-direction: column;
    align-items: center;
    text-align: center;
    gap: 8px;
    padding: 32px 12px;
    color: #978ca9;
  }

  .modal-empty-state p {
    font-size: 12px;
    font-weight: 500;
    margin: 0;
    color: #cfc5dd;
  }

  .modal-empty-state small {
    font-size: 10px;
    color: #887d9c;
  }

  .modal-footer {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 14px 22px;
    border-top: 1px solid #ffffff0c;
    background: #17161b;
  }

  .modal-hint {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: 9.5px;
    color: #9387a5;
    margin: 0;
  }

  .modal-done-btn {
    padding: 7px 16px;
    font-size: 10px;
    font-weight: 600;
    background: #2b2535;
    color: #f1ecf7;
    border-radius: 7px;
  }

  .modal-done-btn:hover {
    background: #3a3248;
  }

  :global(:root[data-theme="light"]) .modal-backdrop {
    background: rgba(30, 24, 45, 0.45);
  }
  :global(:root[data-theme="light"]) .category-modal {
    background: linear-gradient(165deg, #ffffff, #f7f6fc 85%);
    border-color: color-mix(in srgb, var(--modal-accent, #b8a1fa) 35%, rgba(35, 28, 56, 0.12));
    box-shadow: 0 24px 64px rgba(25, 15, 45, 0.22), 0 0 32px color-mix(in srgb, var(--modal-accent, #b8a1fa) 16%, transparent);
  }
  :global(:root[data-theme="light"]) .modal-header {
    border-bottom: 1px solid rgba(35, 28, 56, 0.08);
  }
  :global(:root[data-theme="light"]) .modal-title-group h2 {
    color: #1c1a27;
  }
  :global(:root[data-theme="light"]) .modal-subtitle {
    color: #69657b;
  }
  :global(:root[data-theme="light"]) .modal-close-btn {
    background: #f0edf6;
    color: #554d68;
  }
  :global(:root[data-theme="light"]) .modal-close-btn:hover {
    background: #e4e0ee;
    color: #1c1a27;
  }
  :global(:root[data-theme="light"]) .modal-body {
    scrollbar-color: #c5bfd2 #f7f6fc;
  }
  :global(:root[data-theme="light"]) .modal-app-row {
    background: #fbfafc;
    border: 1px solid rgba(35, 28, 56, 0.08);
  }
  :global(:root[data-theme="light"]) .modal-app-row:hover {
    background: #f4f1fa;
    border-color: color-mix(in srgb, var(--modal-accent) 45%, #ccc);
  }
  :global(:root[data-theme="light"]) .modal-app-info strong {
    color: #1c1a27;
  }
  :global(:root[data-theme="light"]) .modal-app-info span {
    color: #69657b;
  }
  :global(:root[data-theme="light"]) .modal-route-select :global(.studio-select-trigger) {
    background: #f0edf6;
    border-color: rgba(35, 28, 56, 0.12);
  }
  :global(:root[data-theme="light"]) .modal-unroute-btn {
    background: #f0edf6;
    border-color: rgba(35, 28, 56, 0.1);
    color: #554d68;
  }
  :global(:root[data-theme="light"]) .modal-unroute-btn:hover {
    background: #ff4d6d;
    color: #ffffff;
    border-color: #ff4d6d;
  }
  :global(:root[data-theme="light"]) .modal-empty-state p {
    color: #1c1a27;
  }
  :global(:root[data-theme="light"]) .modal-empty-state small {
    color: #69657b;
  }
  :global(:root[data-theme="light"]) .modal-footer {
    border-top: 1px solid rgba(35, 28, 56, 0.08);
    background: #f8f7fc;
  }
  :global(:root[data-theme="light"]) .modal-hint {
    color: #69657b;
  }
  :global(:root[data-theme="light"]) .modal-done-btn {
    background: #f0edf6;
    color: #1c1a27;
    border: 1px solid rgba(35, 28, 56, 0.1);
  }
  :global(:root[data-theme="light"]) .modal-done-btn:hover {
    background: #e4e0ee;
  }
</style>
