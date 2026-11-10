<script lang="ts">
  interface Props {
    title: string;
    desc: string;
    checked: boolean;
    disabled?: boolean;
    /** Appended to the description, e.g. "Desktop app only". */
    note?: string;
    onchange: (next: boolean) => void;
  }

  let { title, desc, checked, disabled = false, note, onchange }: Props = $props();
  const id = $props.id();
</script>

<div class="toggle-row" class:is-disabled={disabled}>
  <div class="toggle-copy">
    <strong id={`${id}-label`}>{title}</strong>
    <small>{desc}{#if note} · {note}{/if}</small>
  </div>
  <button
    type="button"
    class="sw"
    class:on={checked}
    role="switch"
    aria-checked={checked}
    aria-labelledby={`${id}-label`}
    {disabled}
    onclick={() => onchange(!checked)}
  >
    <span></span>
  </button>
</div>

<style>
  .toggle-row {
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
  .toggle-row.is-disabled { opacity: 0.55; }
  .toggle-copy { display: flex; flex-direction: column; gap: 3px; min-width: 0; }
  .toggle-copy strong { font-size: 12px; font-weight: 600; }
  .toggle-copy small { font-size: 10.5px; line-height: 1.5; color: var(--text-muted); overflow-wrap: anywhere; }

  .sw {
    position: relative;
    flex-shrink: 0;
    width: 40px;
    height: 23px;
    padding: 0;
    border: 0;
    border-radius: 999px;
    background: var(--control-hover);
    cursor: pointer;
    transition: background-color 180ms ease;
  }
  .sw span {
    position: absolute;
    top: 3px;
    left: 3px;
    width: 17px;
    height: 17px;
    border-radius: 50%;
    background: #fff;
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.35);
    transition: transform 180ms cubic-bezier(0.22, 1, 0.36, 1);
  }
  .sw.on { background: var(--violet); }
  .sw.on span { transform: translateX(17px); }
  .sw:disabled { cursor: not-allowed; }
  .sw:focus-visible { outline: 2px solid var(--violet); outline-offset: 2px; }
</style>
