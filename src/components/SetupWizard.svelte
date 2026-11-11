<script lang="ts">
  import { onMount, tick, type Snippet } from "svelte";
  import StudioIcon from "./StudioIcon.svelte";
  import ToggleRow from "./ToggleRow.svelte";
  import ModeChoice from "./ModeChoice.svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { appSettings, updateAppSettings, type AppSettings } from "../appSettings.svelte";

  interface SetupCheck {
    id: string;
    level: "ok" | "info" | "warn";
    title: string;
    detail: string;
  }

  interface Props {
    open: boolean;
    native: boolean;
    /** The device / virtual-cable configuration UI (shared with Settings → Audio). */
    devices: Snippet;
    onFinish: () => void;
  }

  let { open, native, devices, onFinish }: Props = $props();

  const STEPS = ["Welcome", "Devices", "Interface", "Preferences", "Check", "Ready"] as const;
  const CHECK_STEP = 4;
  let step = $state(0);
  let checks = $state<SetupCheck[]>([]);
  let checking = $state(false);

  async function runChecks() {
    if (!native || checking) return;
    checking = true;
    try {
      // Give Windows a moment to register cables installed in the previous step.
      checks = await invoke<SetupCheck[]>("get_setup_checks");
    } catch (err) {
      error = `Could not check your setup: ${String(err)}`;
    } finally {
      checking = false;
    }
  }
  let dialogEl = $state<HTMLDivElement>();
  let error = $state<string | null>(null);

  const isLast = $derived(step === STEPS.length - 1);

  async function save(patch: Partial<AppSettings>) {
    error = null;
    try {
      await updateAppSettings(patch);
    } catch (err) {
      error = `Could not save that setting: ${String(err)}`;
    }
  }

  async function finish() {
    await save({ onboardingCompleted: true });
    step = 0;
    onFinish();
  }

  function focusHeading() {
    void tick().then(() => dialogEl?.querySelector<HTMLElement>("[data-step-heading]")?.focus());
  }

  function go(delta: number) {
    step = Math.min(STEPS.length - 1, Math.max(0, step + delta));
    if (step === CHECK_STEP) void runChecks();
    focusHeading();
  }

  function handleKeydown(e: KeyboardEvent) {
    if (!open) return;
    if (e.key === "Escape") {
      if (document.querySelector(".studio-select-menu")) return;
      e.preventDefault();
      e.stopPropagation();
      void finish();
    }
  }

  $effect(() => {
    if (!open) return;
    document.body.classList.add("has-active-modal");
    step = 0;
    focusHeading();
    return () => document.body.classList.remove("has-active-modal");
  });

  onMount(() => {
    window.addEventListener("keydown", handleKeydown, true);
    return () => window.removeEventListener("keydown", handleKeydown, true);
  });
</script>

{#if open}
  <div class="wizard-backdrop" role="presentation">
    <div bind:this={dialogEl} class="wizard" role="dialog" aria-modal="true" aria-labelledby="wizard-title" tabindex="-1">
      <header class="wizard-head">
        <ol class="progress" aria-label="Setup progress">
          {#each STEPS as label, i}
            <li class:done={i < step} class:current={i === step} aria-current={i === step ? "step" : undefined}>
              <span class="num">{i < step ? "✓" : i + 1}</span><span class="label">{label}</span>
            </li>
          {/each}
        </ol>
        <button type="button" class="skip" onclick={finish}>Skip setup</button>
      </header>

      <div class="wizard-body">
        {#if error}<div class="wizard-alert" role="alert">{error}</div>{/if}

        {#if step === 0}
          <h2 id="wizard-title" data-step-heading tabindex="-1">Welcome to Stellars</h2>
          <p class="lead">Give every app its own volume, EQ and destination. This quick setup gets your devices ready in about a minute.</p>
          <ul class="points">
            <li><StudioIcon name="mix" size={18} /><div><strong>Five channels</strong><span>Game, Chat, Media, Aux and Mic, each with its own fader, EQ and dynamics.</span></div></li>
            <li><StudioIcon name="route" size={18} /><div><strong>Route any app</strong><span>Drag an app onto a channel. Stellars remembers it next time.</span></div></li>
            <li><StudioIcon name="broadcast" size={18} /><div><strong>Separate stream mix</strong><span>Send a different blend to OBS without changing what you hear.</span></div></li>
          </ul>
        {:else if step === 1}
          <h2 id="wizard-title" data-step-heading tabindex="-1">Pick your devices</h2>
          <p class="lead">Choose where you listen and which microphone to use. If virtual cables are missing, use <em>One-click Install</em>; Windows will ask for administrator permission.</p>
          {@render devices()}
        {:else if step === 2}
          <h2 id="wizard-title" data-step-heading tabindex="-1">How much do you want to see?</h2>
          <p class="lead">Pick the layout that suits you. You can switch at any time.</p>
          <ModeChoice value={appSettings.interfaceMode} onchoose={(mode) => save({ interfaceMode: mode, interfaceModeChosen: true })} />
        {:else if step === 3}
          <h2 id="wizard-title" data-step-heading tabindex="-1">How should Stellars behave?</h2>
          <p class="lead">You can change any of this later in Settings.</p>
          <ToggleRow
            title="Launch when Windows starts"
            desc="Your routing is ready as soon as you sign in."
            checked={appSettings.launchOnStartup}
            disabled={!native}
            note={native ? undefined : "Desktop app only"}
            onchange={(v) => save({ launchOnStartup: v })}
          />
          <ToggleRow
            title="Keep running in the tray when closed"
            desc="Closing the window keeps the audio engine and your cables active."
            checked={appSettings.closeBehavior === "tray"}
            disabled={!native}
            note={native ? undefined : "Desktop app only"}
            onchange={(v) => save({ closeBehavior: v ? "tray" : "quit" })}
          />
          <ToggleRow
            title="Reconnect devices automatically"
            desc="If a headset or microphone is unplugged and returns, Stellars binds to it again."
            checked={appSettings.autoReconnectDevices}
            onchange={(v) => save({ autoReconnectDevices: v })}
          />
        {:else if step === CHECK_STEP}
          <h2 id="wizard-title" data-step-heading tabindex="-1">Checking your setup</h2>
          <p class="lead">Stellars checks your cables, routes and Windows defaults. Fix anything marked in orange, then check again. You can still finish with warnings.</p>
          {#if !native}
            <p class="lead">The setup check reads your real audio devices, so it is only available in the desktop app.</p>
          {:else}
            <ul class="checks" aria-live="polite">
              {#each checks as check (check.id)}
                <li class={check.level}><span class="dot"></span><span><strong>{check.title}</strong>{check.detail ? ` · ${check.detail}` : ""}</span></li>
              {:else}
                <li class="info"><span class="dot"></span><span>{checking ? "Checking…" : "No results yet."}</span></li>
              {/each}
            </ul>
            <div><button type="button" class="quiet-button" disabled={checking} onclick={runChecks}><StudioIcon name="reset" size={14} /> Check again</button></div>
          {/if}
        {:else}
          <h2 id="wizard-title" data-step-heading tabindex="-1">You’re all set</h2>
          <p class="lead">A few things worth knowing:</p>
          <ul class="points">
            <li><StudioIcon name="route" size={18} /><div><strong>Drag apps onto channels</strong><span>Anything playing audio appears in the app list on the right.</span></div></li>
            <li><StudioIcon name="settings" size={18} /><div><strong>Settings are one click away</strong><span>Press <kbd>,</kbd> any time. Diagnostics and backups live there too.</span></div></li>
            <li><StudioIcon name="headphones" size={18} /><div><strong>Find Stellars in the tray</strong><span>Right-click the tray icon to mute everything or quit.</span></div></li>
          </ul>
        {/if}
      </div>

      <footer class="wizard-foot">
        <button type="button" class="quiet-button" disabled={step === 0} onclick={() => go(-1)}>Back</button>
        {#if isLast}
          <button type="button" class="cable-install-button" onclick={finish}><StudioIcon name="check" size={14} /> Start mixing</button>
        {:else}
          <button type="button" class="cable-install-button" onclick={() => go(1)}>Next <StudioIcon name="arrow" size={14} /></button>
        {/if}
      </footer>
    </div>
  </div>
{/if}

<style>
  .wizard-backdrop { position: fixed; inset: 0; z-index: 9998; display: grid; place-items: center; padding: 24px; background: rgba(8, 7, 12, 0.78); backdrop-filter: blur(10px); -webkit-backdrop-filter: blur(10px); }
  :global(:root[data-theme="light"]) .wizard-backdrop { background: rgba(30, 24, 45, 0.5); }
  .wizard { display: flex; flex-direction: column; width: min(720px, 100%); max-height: min(640px, 100%); border-radius: 18px; background: var(--panel); color: var(--text); border: 1px solid var(--line-strong); box-shadow: 0 28px 72px rgba(0, 0, 0, 0.55); outline: none; overflow: hidden; }
  .wizard-head { display: flex; align-items: center; justify-content: space-between; gap: 16px; padding: 18px 24px; border-bottom: 1px solid var(--line); }
  .progress { display: flex; gap: 18px; margin: 0; padding: 0; list-style: none; }
  .progress li { display: flex; align-items: center; gap: 8px; font-size: 11px; color: var(--text-muted); }
  .progress .num { display: grid; place-items: center; width: 22px; height: 22px; border-radius: 50%; font-size: 10px; font-weight: 600; background: var(--control); }
  .progress .current { color: var(--text); }
  .progress .current .num { background: var(--violet); color: var(--on-violet); }
  .progress .done .num { background: color-mix(in srgb, var(--teal) 25%, transparent); color: var(--teal); }
  .skip { border: 0; background: transparent; color: var(--text-muted); font: inherit; font-size: 11px; cursor: pointer; padding: 6px 8px; border-radius: 7px; }
  .skip:hover { color: var(--text); background: var(--control); }
  .wizard-body { flex: 1; min-height: 0; overflow-y: auto; padding: 24px 28px; display: flex; flex-direction: column; gap: 14px; }
  .wizard-body h2 { margin: 0; font-size: 22px; font-weight: 600; letter-spacing: -0.035em; outline: none; }
  .lead { margin: 0; font-size: 12.5px; line-height: 1.6; color: var(--text-secondary); }
  .points { list-style: none; margin: 6px 0 0; padding: 0; display: flex; flex-direction: column; gap: 8px; }
  .points li { display: flex; gap: 14px; align-items: flex-start; padding: 13px 14px; border-radius: 11px; background: var(--panel-raised); border: 1px solid var(--line); }
  .points li :global(svg) { flex-shrink: 0; margin-top: 2px; color: var(--violet); }
  .points strong { display: block; font-size: 12.5px; margin-bottom: 2px; }
  .points span { font-size: 11.5px; line-height: 1.5; color: var(--text-muted); }
  .checks { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 6px; }
  .checks li { display: flex; gap: 10px; align-items: flex-start; padding: 10px 12px; border-radius: 10px; font-size: 11.5px; line-height: 1.55; background: var(--panel-raised); border: 1px solid var(--line); }
  .checks .dot { flex-shrink: 0; width: 7px; height: 7px; margin-top: 6px; border-radius: 50%; background: var(--text-muted); }
  .checks .ok .dot { background: var(--teal); }
  .checks .warn .dot { background: #f08c7f; }
  .checks .info .dot { background: #edc990; }
  kbd { padding: 1px 6px; border-radius: 5px; background: var(--control); border: 1px solid var(--line-strong); font: inherit; font-size: 10.5px; font-weight: 600; }
  .wizard-alert { padding: 10px 12px; border-radius: 9px; font-size: 11px; color: #ffc4d7; background: #f0a7c61a; border: 1px solid #f0a7c633; }
  :global(:root[data-theme="light"]) .wizard-alert { color: #9c2f58; }
  .wizard-foot { display: flex; justify-content: space-between; padding: 14px 24px; border-top: 1px solid var(--line); }
  .wizard-body :global(.hardware-settings) { display: flex; flex-direction: column; gap: 16px; }
  .wizard-body :global(.hardware-toolbar), .wizard-body :global(.hardware-badges), .wizard-body :global(.hardware-actions) { display: flex; flex-wrap: wrap; gap: 8px; align-items: center; }
  .wizard-body :global(.hardware-settings label) { display: flex; }
  .wizard-body :global(.virtual-routes-grid) { grid-template-columns: 1fr; }
  @media (max-width: 640px) { .progress .label { display: none; } }
</style>
