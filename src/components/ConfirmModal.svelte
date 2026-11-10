<script lang="ts">
  import { onMount } from 'svelte';
  import StudioIcon from './StudioIcon.svelte';

  type Props = {
    open: boolean;
    title: string;
    subtitle?: string;
    message: string;
    details?: string[];
    confirmLabel?: string;
    cancelLabel?: string;
    tone?: 'danger' | 'warning' | 'info';
    icon?: string;
    accentColor?: string;
    onConfirm: () => void | Promise<void>;
    onCancel: () => void;
  };

  let {
    open,
    title,
    subtitle,
    message,
    details = [],
    confirmLabel = 'Confirm',
    cancelLabel = 'Cancel',
    tone = 'warning',
    icon = 'reset',
    accentColor,
    onConfirm,
    onCancel
  }: Props = $props();

  let isSubmitting = $state(false);
  let cancelButtonRef = $state<HTMLButtonElement>();

  const effectiveColor = $derived.by(() => {
    if (accentColor) return accentColor;
    if (tone === 'danger') return '#f08c7f';
    if (tone === 'warning') return '#edc990';
    return '#87ddcf';
  });

  function handleKeydown(e: KeyboardEvent) {
    if (!open) return;
    if (e.key === 'Escape') {
      e.stopPropagation();
      e.preventDefault();
      if (!isSubmitting) onCancel();
    }
  }

  function handleBackdropClick(e: MouseEvent) {
    if (e.target === e.currentTarget && !isSubmitting) {
      e.preventDefault();
      e.stopPropagation();
      onCancel();
    }
  }

  async function handleConfirmClick() {
    if (isSubmitting) return;
    isSubmitting = true;
    try {
      await onConfirm();
    } finally {
      isSubmitting = false;
    }
  }

  $effect(() => {
    if (open) {
      const prevOverflow = document.body.style.overflow;
      document.body.classList.add('has-active-modal');
      document.body.style.overflow = 'hidden';

      setTimeout(() => {
        cancelButtonRef?.focus();
      }, 50);

      return () => {
        document.body.classList.remove('has-active-modal');
        document.body.style.overflow = prevOverflow;
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

{#if open}
  <div
    class="modal-backdrop confirm-backdrop"
    onclick={handleBackdropClick}
    role="presentation"
  >
    <div
      class="confirm-modal panel"
      style={`--confirm-accent: ${effectiveColor};`}
      role="alertdialog"
      aria-modal="true"
      aria-labelledby="confirm-modal-title"
      aria-describedby="confirm-modal-desc"
      tabindex="-1"
      onclick={e => e.stopPropagation()}
      onkeydown={e => e.stopPropagation()}
    >
      <div class="confirm-icon-wrap">
        <div class="confirm-icon-badge">
          <StudioIcon name={icon} size={22} />
        </div>
      </div>

      <header class="confirm-header">
        {#if subtitle}
          <span class="confirm-subtitle">{subtitle}</span>
        {/if}
        <h2 id="confirm-modal-title">{title}</h2>
      </header>

      <div class="confirm-body">
        <p id="confirm-modal-desc">{message}</p>
        {#if details && details.length > 0}
          <ul class="confirm-details-list">
            {#each details as item}
              <li>{item}</li>
            {/each}
          </ul>
        {/if}
      </div>

      <footer class="confirm-footer">
        <button
          bind:this={cancelButtonRef}
          type="button"
          class="confirm-cancel-btn"
          disabled={isSubmitting}
          onclick={onCancel}
        >
          {cancelLabel}
        </button>
        <button
          type="button"
          class="confirm-action-btn"
          class:danger={tone === 'danger'}
          class:warning={tone === 'warning'}
          disabled={isSubmitting}
          onclick={handleConfirmClick}
        >
          {#if isSubmitting}
            <span>Processing…</span>
          {:else}
            <StudioIcon name={icon} size={15} />
            <span>{confirmLabel}</span>
          {/if}
        </button>
      </footer>
    </div>
  </div>
{/if}

<style>
  .modal-backdrop {
    position: fixed;
    inset: 0;
    z-index: 9999;
    background: rgba(8, 7, 12, 0.75);
    backdrop-filter: blur(8px);
    -webkit-backdrop-filter: blur(8px);
    display: grid;
    place-items: center;
    padding: 20px;
    animation: backdrop-fade 180ms ease-out;
  }

  @keyframes backdrop-fade {
    from { opacity: 0; }
    to { opacity: 1; }
  }

  .confirm-modal {
    position: relative;
    width: 100%;
    max-width: 440px;
    background: #18161d;
    border-radius: 18px;
    border: 1px solid color-mix(in srgb, var(--confirm-accent, #edc990) 30%, #ffffff14);
    box-shadow: 0 24px 64px rgba(0, 0, 0, 0.75), 0 0 32px color-mix(in srgb, var(--confirm-accent, #edc990) 12%, transparent);
    padding: 26px 24px 22px;
    display: flex;
    flex-direction: column;
    gap: 16px;
    animation: modal-pop 200ms cubic-bezier(.22, 1, .36, 1);
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

  .confirm-icon-wrap {
    display: flex;
    justify-content: center;
    margin-top: 2px;
  }

  .confirm-icon-badge {
    width: 48px;
    height: 48px;
    border-radius: 50%;
    background: color-mix(in srgb, var(--confirm-accent, #edc990) 14%, #1e1b24);
    border: 1px solid color-mix(in srgb, var(--confirm-accent, #edc990) 35%, transparent);
    color: var(--confirm-accent, #edc990);
    display: grid;
    place-items: center;
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.35), 0 0 16px color-mix(in srgb, var(--confirm-accent, #edc990) 18%, transparent);
  }

  .confirm-header {
    text-align: center;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 6px;
  }

  .confirm-subtitle {
    font-size: 10px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--confirm-accent, #edc990);
    padding: 2px 8px;
    border-radius: 4px;
    background: color-mix(in srgb, var(--confirm-accent, #edc990) 12%, transparent);
  }

  .confirm-header h2 {
    font-size: 17px;
    font-weight: 600;
    letter-spacing: -0.025em;
    color: #f4f3f7;
    margin: 0;
  }

  .confirm-body {
    display: flex;
    flex-direction: column;
    gap: 12px;
    font-size: 12px;
    line-height: 1.6;
    color: #b7b0c4;
    text-align: center;
    padding: 0 6px;
  }

  .confirm-body p {
    margin: 0;
  }

  .confirm-details-list {
    margin: 4px 0 0;
    padding: 10px 14px 10px 24px;
    background: #141318;
    border-radius: 8px;
    border: 1px solid #ffffff0b;
    text-align: left;
    font-size: 11px;
    color: #c8c1d5;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .confirm-footer {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 10px;
    margin-top: 6px;
    padding-top: 14px;
    border-top: 1px solid #ffffff0d;
  }

  .confirm-cancel-btn {
    padding: 9px 16px;
    background: #24222a;
    color: #cfc7dc;
    border: 0;
    border-radius: 8px;
    font-size: 11px;
    font-weight: 500;
    cursor: pointer;
    transition: background 160ms ease, color 160ms ease;
  }

  .confirm-cancel-btn:hover {
    background: #2e2b36;
    color: #f4f2fa;
  }

  .confirm-action-btn {
    padding: 9px 18px;
    background: var(--confirm-accent, #edc990);
    color: #151319;
    border: 0;
    border-radius: 8px;
    font-size: 11px;
    font-weight: 600;
    display: inline-flex;
    align-items: center;
    gap: 7px;
    cursor: pointer;
    transition: filter 160ms ease, transform 120ms ease, box-shadow 160ms ease;
    box-shadow: 0 4px 14px color-mix(in srgb, var(--confirm-accent, #edc990) 30%, transparent);
  }

  .confirm-action-btn:hover {
    filter: brightness(1.1);
    transform: translateY(-1px);
    box-shadow: 0 6px 18px color-mix(in srgb, var(--confirm-accent, #edc990) 45%, transparent);
  }

  .confirm-action-btn:active {
    transform: scale(0.97);
  }

  .confirm-action-btn:disabled,
  .confirm-cancel-btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
    transform: none;
  }

  :global(:root[data-theme="light"]) .modal-backdrop {
    background: rgba(30, 24, 45, 0.45);
  }
  :global(:root[data-theme="light"]) .confirm-modal {
    background: #ffffff;
    border-color: color-mix(in srgb, var(--confirm-accent, #edc990) 45%, rgba(35, 28, 56, 0.12));
    box-shadow: 0 24px 64px rgba(25, 15, 45, 0.2), 0 0 32px color-mix(in srgb, var(--confirm-accent, #edc990) 14%, transparent);
  }
  :global(:root[data-theme="light"]) .confirm-icon-badge {
    background: color-mix(in srgb, var(--confirm-accent, #edc990) 16%, #ffffff);
    border-color: color-mix(in srgb, var(--confirm-accent, #edc990) 40%, #e0dbe9);
    box-shadow: 0 4px 14px rgba(35, 28, 56, 0.08);
  }
  :global(:root[data-theme="light"]) .confirm-header h2 {
    color: #1a1824;
  }
  :global(:root[data-theme="light"]) .confirm-body p {
    color: #555164;
  }
  :global(:root[data-theme="light"]) .confirm-details-list {
    background: #f7f5fb;
    border: 1px solid rgba(35, 28, 56, 0.08);
    color: #4a4558;
  }
  :global(:root[data-theme="light"]) .confirm-cancel-btn {
    background: #f0edf6;
    color: #4a4558;
    border: 1px solid rgba(35, 28, 56, 0.1);
  }
  :global(:root[data-theme="light"]) .confirm-cancel-btn:hover {
    background: #e4e0ee;
    color: #1a1824;
  }
</style>
