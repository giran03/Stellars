<script lang="ts">
  import StudioSelect from './StudioSelect.svelte';
  import EditableValue from './EditableValue.svelte';
  import StudioIcon from './StudioIcon.svelte';
  import AppIcon from './AppIcon.svelte';
  import { BUS_COLORS } from "../types";
  import type { ActiveAudioProcess, AudioEndpointInfo, BusId, StereoMeterSnapshot, StreamConfig } from "../types";
  type MicSource = NonNullable<StreamConfig["micSource"]>;
  import { linearToDbString } from "../dspMath";

  interface Props {
    stream: StreamConfig;
    endpoints: AudioEndpointInfo[];
    processes: ActiveAudioProcess[];
    meter: StereoMeterSnapshot;
    running: boolean;
    native: boolean;
    saving: boolean;
    error: string | null;
    reservedOutputs: (string | null)[];
    streamMonitoring?: boolean;
    onToggleStreamMonitoring?: () => void;
    cableInstalling?: boolean;
    onInstallOrRepairCables?: () => void;
    onSave: (config: StreamConfig) => Promise<boolean>;
    onRefreshDevices: () => Promise<void>;
  }
  let {
    stream,
    endpoints,
    processes,
    meter,
    running,
    native,
    saving,
    error,
    reservedOutputs,
    streamMonitoring = false,
    onToggleStreamMonitoring,
    cableInstalling = false,
    onInstallOrRepairCables,
    onSave,
    onRefreshDevices,
  }: Props = $props();
  let refreshing = $state(false);
  let draft = $state<StreamConfig | null>(null);
  let search = $state("");
  const categories: BusId[] = ["Game", "Chat", "Media", "Aux", "Mic"];
  const copy = (s: StreamConfig): StreamConfig => ({ ...s, categories: [...s.categories], appOverrides: { ...s.appOverrides }, channels: Object.fromEntries(Object.entries(s.channels).map(([bus, channel]) => [bus, { ...channel }])) as StreamConfig["channels"] });
  const included = (exe: string, bus: BusId | null, s: StreamConfig) =>
    s.appOverrides[exe.toLowerCase()] ?? (bus !== null && s.categories.includes(bus));
  const candidates = $derived(endpoints.filter(e => e.flow === "Render" && !reservedOutputs.includes(e.id)
    && !(reservedOutputs[0] === null && e.isDefault)));
  const destination = $derived(endpoints.find(e => e.id === stream.outputDeviceId)?.name);
  const selectedCount = $derived(processes.filter(p => included(p.exeName, p.assignedBus, stream)).length);
  const overridesCount = $derived(Object.keys(stream.appOverrides).length);
  const appNames = $derived([...new Set([
    ...processes.map(p => p.exeName.toLowerCase()), ...Object.keys(draft?.appOverrides ?? {}),
  ])].sort((a, b) => {
    const running = (n: string) => processes.some(p => p.exeName.toLowerCase() === n);
    return Number(running(b)) - Number(running(a)) || a.localeCompare(b);
  }).filter(name => name.includes(search.toLowerCase())));
  const micSource = $derived<MicSource>(draft?.micSource ?? "virtual");
  const status = $derived(!stream.enabled ? "Off" : stream.muted ? "Muted" : !native ? "Preview" : running ? "Sending audio" : "Output unavailable");

  function toggleCategory(bus: BusId) {
    if (!draft) return;
    draft.categories = draft.categories.includes(bus) ? draft.categories.filter(b => b !== bus) : [...draft.categories, bus];
  }

  function setOverride(exe: string, value: string) {
    if (!draft) return;
    const next = { ...draft.appOverrides };
    if (value === "auto") delete next[exe];
    else next[exe] = value === "include";
    draft.appOverrides = next;
  }

  function toggleStreamEnabled() {
    if (!stream.outputDeviceId && !stream.enabled) {
      if (!draft) {
        draft = copy(stream);
        search = "";
      }
      void onSave({ ...copy(stream), enabled: true });
      return;
    }
    const next = !stream.enabled;
    void onSave({ ...copy(stream), enabled: next });
    if (draft) {
      draft.enabled = next;
    }
  }

  // ── Autosave ─────────────────────────────────────────────────────────────
  // Only the fields edited in this form count; power, mute, level and sends are saved elsewhere.
  type SaveState = "idle" | "pending" | "saving" | "saved" | "invalid" | "error";
  const AUTOSAVE_DELAY_MS = 450;
  const SAVED_FLASH_MS = 2200;
  let saveState = $state<SaveState>("idle");
  let savedSignature = "";
  let failedSignature = "";
  let flashTimer: ReturnType<typeof setTimeout> | undefined;

  const signature = (d: StreamConfig) => JSON.stringify([
    d.outputDeviceId, [...d.categories].sort(), Object.entries(d.appOverrides).sort(), d.micSource ?? "virtual",
  ]);
  const invalidReason = $derived(
    draft?.outputDeviceId && !candidates.some(e => e.id === draft?.outputDeviceId)
      ? "Not saved: this output is disconnected or in use elsewhere. Choose another cable."
      : null);
  const statusText = $derived(
    saveState === "pending" ? "Unsaved changes…"
    : saveState === "saving" ? "Saving…"
    : saveState === "saved" ? "Saved"
    : saveState === "invalid" ? (invalidReason ?? "Not saved")
    : saveState === "error" ? `Could not save${error ? `: ${error}` : ". Check the output and try again."}`
    : native ? "Changes save automatically" : "Preview only · Open the desktop app to send audio.");

  function openSettings() {
    draft = copy(stream);
    savedSignature = signature(draft);
    failedSignature = "";
    saveState = "idle";
    search = "";
  }

  async function commit() {
    if (!draft) return;
    const snapshot = copy(draft);
    const snapshotSignature = signature(snapshot);
    clearTimeout(flashTimer);
    saveState = "saving";
    const ok = await onSave({ ...snapshot, enabled: stream.enabled, channels: copy(stream).channels });
    if (ok) {
      savedSignature = snapshotSignature;
      failedSignature = "";
      saveState = "saved";
      flashTimer = setTimeout(() => { if (saveState === "saved") saveState = "idle"; }, SAVED_FLASH_MS);
    } else {
      failedSignature = snapshotSignature;
      saveState = "error";
    }
  }

  $effect(() => {
    if (!draft) return;
    const current = signature(draft);
    const reason = invalidReason;
    const busy = saving;
    if (current === savedSignature) { if (saveState === "pending" || saveState === "invalid") saveState = "idle"; return; }
    if (reason) { saveState = "invalid"; return; }
    if (current === failedSignature || busy) return; // wait for the in-flight save / the next edit
    saveState = "pending";
    const timer = setTimeout(() => {
      // A save that finished after this was scheduled may already have covered these edits.
      if (draft && signature(draft) !== savedSignature) void commit();
      else if (saveState === "pending") saveState = "idle";
    }, AUTOSAVE_DELAY_MS);
    return () => clearTimeout(timer);
  });

  $effect(() => () => clearTimeout(flashTimer));

  /** Flushes any valid pending edit, then closes. Stays open if the change cannot be saved. */
  async function closeSettings() {
    if (draft && signature(draft) !== savedSignature) {
      if (invalidReason) { saveState = "invalid"; return; }
      await commit();
      if (saveState === "error") return;
    }
    draft = null;
  }
</script>

<section class="stream-panel" aria-label="Stream recording output">
  <div class="stream-summary">
    <div class="identity">
      <svg width="20" height="20" viewBox="0 0 24 24" fill="none" aria-hidden="true">
        <rect x="3" y="4" width="18" height="13" rx="2" stroke="currentColor" stroke-width="1.5" />
        <path d="M8 21h8M12 17v4m-2-14 6 3.5-6 3.5V7Z" stroke="currentColor" stroke-width="1.5" stroke-linejoin="round" />
      </svg>
      <h2>Stream</h2>
      <span class="status" class:sending={stream.enabled && !stream.muted && (running || !native)}>{status}</span>
    </div>
    <div class="mix-summary">
      <strong title={destination}>{destination ?? "Choose a dedicated streaming cable"}</strong>
      <span>{selectedCount} app{selectedCount === 1 ? "" : "s"} selected{stream.categories.includes("Mic") ? " + mic" : ""} · {overridesCount} app override{overridesCount === 1 ? "" : "s"}</span>
    </div>
    <div class="level-control">
      <label for="stream-volume">Stream level <strong><EditableValue value={stream.volume*100} min={0} max={150} step={1} unit="%" label="Stream level" disabled={saving} oncommit={v=>onSave({...copy(stream),volume:v/100})}/> <EditableValue value={stream.volume>0?20*Math.log10(stream.volume):-60} min={-60} max={3.521825} step={.1} unit="dB" display={stream.volume>0?(20*Math.log10(stream.volume)).toFixed(2)+" dB":"−∞ dB"} label="Stream gain" disabled={saving} allowSilence oncommit={v=>onSave({...copy(stream),volume:Math.pow(10,v/20)})} onsilence={()=>onSave({...copy(stream),volume:0})}/></strong></label>
      <input id="stream-volume" class="horizontal-range" type="range" min="0" max="1.5" step="0.01" value={stream.volume}
        disabled={saving} aria-label="Stream output volume"
        onchange={e => onSave({ ...copy(stream), volume: Number(e.currentTarget.value) })} />
      <div class="meter" role="meter" aria-label="Stream stereo peak" aria-valuemin="0" aria-valuemax="100"
        aria-valuenow={Math.round(Math.min(1, Math.max(meter.peakLeft, meter.peakRight)) * 100)}>
        <span style:width={`${Math.min(100, meter.peakLeft * 100)}%`}></span>
        <span style:width={`${Math.min(100, meter.peakRight * 100)}%`}></span>
      </div>
    </div>
    <div class="summary-actions">
      <button
        type="button"
        class="stream-action-btn stream-playback-btn"
        class:engaged={streamMonitoring}
        class:active={streamMonitoring}
        data-tooltip={streamMonitoring ? "Stream playback ON (Solo headphones) · Click to return to monitor mix" : "Playback stream audio to monitor output (Solo headphones)"}
        aria-pressed={streamMonitoring}
        aria-label={streamMonitoring ? "Mute stream playback monitoring" : "Playback stream audio to monitor output (Solo headphones)"}
        onclick={onToggleStreamMonitoring}
      >
        <StudioIcon name="headphones" size={15} />
      </button>
      <button
        type="button"
        class="stream-action-btn stream-mute-btn"
        class:muted={stream.muted}
        class:engaged={stream.muted}
        aria-label={stream.muted ? "Unmute stream output" : "Mute stream output"}
        aria-pressed={stream.muted}
        data-tooltip={stream.muted ? "Stream muted · Click to unmute" : "Mute stream output"}
        disabled={saving}
        onclick={() => onSave({ ...copy(stream), muted: !stream.muted })}
      >
        <StudioIcon name={stream.muted ? "mute" : "volume"} size={16} />
      </button>
      <button
        type="button"
        class="stream-action-btn settings-button"
        class:active={draft !== null}
        data-tooltip={draft !== null ? "Close stream mix settings" : "Stream mix settings · Choose included channels, executable overrides, and recording output"}
        aria-expanded={draft !== null}
        aria-controls="stream-settings"
        aria-label="Stream mix settings"
        disabled={saving}
        onclick={() => { if (draft) void closeSettings(); else openSettings(); }}
      >
        <StudioIcon name="settings" size={16} />
      </button>
      <button
        type="button"
        class="stream-master-toggle"
        class:active={stream.enabled}
        data-tooltip={
          !stream.outputDeviceId
            ? "Choose a dedicated streaming cable in Mix settings first"
            : stream.enabled
              ? "Stream output is active · Click to turn off"
              : "Stream output is off · Click to turn on"
        }
        aria-pressed={stream.enabled}
        aria-label={`Stream output power toggle (${stream.enabled ? "Active" : "Off"})`}
        disabled={saving}
        onclick={toggleStreamEnabled}
      >
        <span class="stream-toggle-label">{stream.enabled ? "Stream ON" : "Stream OFF"}</span>
        <span class="switch-track" class:on={stream.enabled}>
          <span class="switch-thumb"></span>
        </span>
      </button>
    </div>
  </div>

  {#if error}<p class="error" role="alert">{error}</p>{/if}

  {#if draft}
    <form id="stream-settings" onsubmit={e => e.preventDefault()}>
      <fieldset>
        <legend class="sr-only">Stream mix settings</legend>
        <div class="settings-layout">
          <div class="source-settings">
            <section class="group">
              <header><h3>Channels</h3><span class="hint">Pick what your audience hears</span></header>
              <div class="category-options" role="group" aria-label="Stream source categories">
                {#each categories as bus}
                  <button type="button" class="category" aria-pressed={draft.categories.includes(bus)}
                    style={`--source-color:${BUS_COLORS[bus]}`} onclick={() => toggleCategory(bus)}>
                    <span class="dot" aria-hidden="true"></span>{bus}
                    <svg class="check" width="11" height="11" viewBox="0 0 12 12" fill="none" aria-hidden="true">
                      {#if draft.categories.includes(bus)}<path d="m2 6 2.5 2.5L10 3" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" />{/if}
                    </svg>
                  </button>
                {/each}
              </div>
            </section>

            <section class="group">
              <header>
                <h3>Apps <span class="count">{appNames.length}</span></h3>
                <span class="hint">Override a single app</span>
              </header>
              <input id="stream-search" type="search" aria-label="Find an app" placeholder="Find an app…" bind:value={search} />
              <div class="app-list">
                {#each appNames as exe (exe)}
                  {@const proc = processes.find(p => p.exeName.toLowerCase() === exe)}
                  {@const bus = proc ? (proc.assignedBus ?? "Aux") : null}
                  {@const on = included(exe, proc?.assignedBus ?? null, draft)}
                  {@const mode = draft.appOverrides[exe] === undefined ? "auto" : draft.appOverrides[exe] ? "include" : "exclude"}
                  <div class="app-row" class:idle={!proc}>
                    <AppIcon {proc} exeName={exe} size={30} color={bus ? BUS_COLORS[bus as BusId] : "#8d879a"} />
                    <div class="app-name">
                      <strong title={exe}>{proc?.displayName || exe}</strong>
                      <small>
                        {#if bus}<span class="dot" style={`--source-color:${BUS_COLORS[bus as BusId]}`}></span>{bus}{:else}Not running{/if}
                        · <span class="state" class:on>{on ? "In stream" : "Not in stream"}</span>
                      </small>
                    </div>
                    <div class="seg" role="group" aria-label={`${exe} stream inclusion`}>
                      <button type="button" aria-pressed={mode === "auto"} data-tooltip="Follow its channel" onclick={() => setOverride(exe, "auto")}>Auto</button>
                      <button type="button" aria-pressed={mode === "include"} data-tooltip="Always include" onclick={() => setOverride(exe, "include")}>In</button>
                      <button type="button" aria-pressed={mode === "exclude"} data-tooltip="Always exclude" onclick={() => setOverride(exe, "exclude")}>Out</button>
                    </div>
                  </div>
                {:else}<p class="empty">{search ? "No apps match your search." : "Audio apps appear here when they start playing."}</p>{/each}
              </div>
            </section>
          </div>

          <div class="output-settings">
            <section class="group">
              <header><h3>Recording output</h3></header>
              <label for="stream-destination">Dedicated audio cable</label>
              <StudioSelect id="stream-destination" bind:value={draft.outputDeviceId}>
                <option value={null}>Select an output…</option>
                {#each candidates as endpoint}<option value={endpoint.id}>{endpoint.name}</option>{/each}
              </StudioSelect>
              <div class="cable-action-row">
                <button type="button" class="refresh-devices" disabled={refreshing} onclick={async () => {
                  refreshing = true; try { await onRefreshDevices(); } finally { refreshing = false; }
                }}>{refreshing ? "Refreshing…" : "Refresh devices"}</button>
                {#if onInstallOrRepairCables}
                  <button type="button" class="install-cable-link" disabled={cableInstalling} onclick={() => onInstallOrRepairCables?.()}>
                    {cableInstalling ? "Installing cables…" : "Install / Repair VB-CABLE"}
                  </button>
                {/if}
              </div>
              {#if draft.outputDeviceId && !candidates.some(e => e.id === draft?.outputDeviceId)}
                <p class="error">This output is disconnected or already used elsewhere. Choose another cable.</p>
              {/if}
              {#if candidates.length === 0}<p class="empty">No separate output is available. Install the virtual cables above, then refresh devices.</p>{/if}
              <p class="hint-block">In OBS, add this cable's paired recording endpoint as an audio input.</p>
            </section>

            <section class="group">
              <header><h3>Microphone</h3></header>
              <label for="stream-mic-source">Mic source</label>
              <StudioSelect id="stream-mic-source" value={micSource} onchange={e => { if (draft) draft.micSource = e.currentTarget.value as MicSource; }}>
                <option value="virtual">Virtual mic output (recommended)</option>
                <option value="raw">Mic before fader</option>
              </StudioSelect>
              <p class="hint-block">{micSource === "virtual"
                ? "Your audience hears your mic exactly as the virtual mic sends it: fader, limiter and left/right levels included."
                : "Taken before the Mic fader, so the Mic volume and virtual-mic levels do not change what the stream hears."}</p>
            </section>

            <details class="how">
              <summary>How the stream mix works</summary>
              <p>App audio is captured before playback faders and channel DSP, so headphone volume, solo, mute and ChatMix never affect the stream. Use the Stream fader beside each channel for its send level.</p>
              <p>Apps follow their assigned channel; unassigned apps use Aux. Each app is captured once, up to 32 active apps. Keep this cable separate from the silent routing sink and the virtual mic.</p>
              <p>Stream starts off. Selections are saved and apply to matching apps when they reopen, including while Stellars is in the tray.</p>
            </details>
          </div>
        </div>
        <div class="settings-footer">
          <span class="save-status" data-state={saveState} role="status" aria-live="polite"><i aria-hidden="true"></i>{statusText}</span>
          <div class="footer-actions">
            {#if saveState === "error"}<button type="button" onclick={() => void commit()}>Retry</button>{/if}
            <button class="apply-button" type="button" onclick={() => void closeSettings()}>Done</button>
          </div>
        </div>
      </fieldset>
    </form>
  {/if}
</section>

<style>
 .stream-panel{--stream-color:var(--bus-stream);background:linear-gradient(115deg,#1d2530,#1c1e24 65%);border-radius:16px;padding:22px 23px;min-width:0}.stream-summary{display:grid;grid-template-columns:auto minmax(0,1fr) minmax(160px,210px) auto;align-items:center;gap:22px}.identity{display:flex;align-items:center;gap:10px;color:var(--stream-color)}.identity>svg{width:40px;height:40px;padding:10px;border-radius:12px;background:#9bd6ff12}h2{font-size:16px;font-weight:600;letter-spacing:-.025em}.status{font-size:9px;color:#b0a6bb;background:#ffffff08;border-radius:5px;padding:4px 7px;white-space:nowrap}.status.sending{color:#b6dfff;background:#9bd6ff12}.mix-summary{min-width:0;display:flex;flex-direction:column;gap:8px}.mix-summary strong{font-weight:400;font-size:11px;white-space:nowrap;overflow:hidden;text-overflow:ellipsis;color:#d8cfe3}.mix-summary>span{font-size:10px;color:#b3a6c0}.level-control{min-width:0}.level-control label{display:flex;align-items:center;justify-content:space-between;gap:12px;font-size:10px;color:#baadc9}.level-control strong{font-size:10px;font-weight:500;color:#e0d3ee;font-variant-numeric:tabular-nums}.level-control input{margin:15px 0 12px}.meter{height:10px;display:grid;gap:2px;overflow:hidden;background:#0003;border-radius:5px}.meter span{height:4px;border-radius:2px;background:var(--stream-color);transition:width 80ms linear}.footer-actions{display:flex;align-items:center;gap:8px;flex-wrap:wrap}
  .summary-actions{display:flex;align-items:center;gap:8px;flex-wrap:nowrap;justify-content:flex-end}
  .stream-action-btn{width:32px;height:32px;min-width:32px;min-height:32px;max-width:32px;max-height:32px;padding:0;margin:0;display:inline-flex;align-items:center;justify-content:center;border-radius:8px;background:#2b2930;border:1px solid rgba(255,255,255,0.08);color:#d9cede;cursor:pointer;box-sizing:border-box;flex-shrink:0;transition:background-color .18s ease,border-color .18s ease,color .18s ease,box-shadow .18s ease;user-select:none}
  .stream-action-btn:hover{background:#3a3441;border-color:rgba(155,214,255,0.25);color:#f4f3f7}
  .stream-action-btn:disabled{cursor:not-allowed;opacity:.4}
  .stream-action-btn:focus-visible{outline:2px solid var(--stream-color,#9bd6ff);outline-offset:2px}
  .stream-playback-btn.engaged{background:rgba(155,214,255,0.18);color:var(--stream-color,#9bd6ff);border-color:rgba(155,214,255,0.5);box-shadow:0 0 10px rgba(155,214,255,0.28)}
  .stream-playback-btn.engaged:hover{background:rgba(155,214,255,0.26);border-color:rgba(155,214,255,0.7)}
  .stream-mute-btn.muted{background:rgba(239,68,68,0.18);color:#f87171;border-color:rgba(239,68,68,0.4)}
  .stream-mute-btn.muted:hover{background:rgba(239,68,68,0.26);border-color:rgba(239,68,68,0.6)}
  .settings-button.active{background:rgba(155,214,255,0.15);color:var(--stream-color,#9bd6ff);border-color:rgba(155,214,255,0.4)}
  .settings-button.active:hover{background:rgba(155,214,255,0.22)}
button{min-height:32px;background:#2b2930;border:0;border-radius:8px;color:#d9cede;font-size:10px;padding:8px 12px;cursor:pointer}button:hover{background:#3a3441}button:disabled{cursor:not-allowed;opacity:.4}button.muted{background:#9bd6ff1a;color:#b6dfff}.apply-button{background:#a9d8f8;color:#1e2832}.apply-button:hover{background:#c8e8ff}.settings-button[aria-expanded=true]{color:#b6dfff;background:#9bd6ff12}form{border-top:1px solid #ffffff0a;margin-top:22px;padding-top:22px}fieldset{padding:0;margin:0;border:0;min-width:0}.settings-layout{display:grid;grid-template-columns:minmax(0,1.5fr) minmax(0,1fr);gap:32px}.source-settings,.output-settings{min-width:0}h3{font-size:12px;font-weight:600;margin:0 0 12px}p{font-size:11px;line-height:1.8;color:#b3a6c0;max-width:65ch;margin:0 0 16px}.category-options{display:flex;gap:7px;flex-wrap:wrap;margin:12px 0 16px}.category{display:flex;gap:7px;align-items:center;background:#2b2832;color:#b8acc7}.category[aria-pressed=true]{background:#b8a1fa;color:#21192c}.check{flex-shrink:0}.signal-note{font-size:10px}.search-label,.output-settings>label{display:block;color:#d4c6e3;font-size:11px;margin-bottom:10px}input[type=search],.output-settings :global(.studio-select-trigger){display:block;width:100%;min-width:0;background:#15151a;border:0;color:#e1d6ed;border-radius:8px;padding:10px;font-size:11px;margin-bottom:14px}.output-settings :global(.studio-select-trigger){background:#302837}.app-list{max-height:260px;overflow-y:auto;padding:0 5px 0 0;display:block}.app-row{display:grid;grid-template-columns:minmax(0,1fr) minmax(100px,135px);gap:12px;align-items:center;padding:11px 0;border-bottom:1px solid #ffffff06;cursor:default;transform:none}.app-name{display:flex;flex-direction:column;gap:5px;min-width:0}.app-name strong{font-size:11px;font-weight:500;overflow-wrap:anywhere}.app-name small{font-size:9px;color:#ab9cbb}.app-row :global(.studio-select-trigger){background:#302837;border:0;border-radius:6px;padding:9px 6px;width:100%;min-width:0;color:#d8cae7;font-size:10px}.privacy-note{font-size:10px}.cable-action-row{display:flex;align-items:center;gap:14px;flex-wrap:wrap;margin-bottom:12px}.refresh-devices{background:transparent;color:#acd7f4;padding-left:0}.install-cable-link{background:#b8a1fa18;color:#dfd4ff;padding:6px 11px;border-radius:7px;font-size:10px}.install-cable-link:hover{background:#b8a1fa2e}.settings-footer{display:flex;align-items:center;justify-content:space-between;gap:15px;flex-wrap:wrap;margin-top:20px;padding-top:17px;border-top:1px solid #ffffff0a}.settings-footer>span{font-size:10px;color:#af9dbf}.error{color:#f0a7c6;margin-top:15px;overflow-wrap:anywhere}.empty{padding:13px 0}.sr-only{position:absolute;width:1px;height:1px;padding:0;overflow:hidden;clip-path:inset(50%);white-space:nowrap}
 @media(max-width:1250px){.stream-summary{grid-template-columns:auto minmax(0,1fr);gap:18px}.level-control{max-width:350px}.summary-actions{justify-content:flex-end}}
 @media(max-width:720px){.stream-panel{padding:18px 15px}.settings-layout{grid-template-columns:minmax(0,1fr);gap:24px}.stream-summary{grid-template-columns:minmax(0,1fr);gap:16px}.identity{flex-wrap:wrap}.summary-actions{justify-content:flex-start}.level-control{max-width:none}.settings-footer{align-items:flex-start}.app-row{grid-template-columns:minmax(0,1fr) 120px}.output-settings{border-top:1px solid #ffffff0a;padding-top:20px}}
 @media(prefers-reduced-motion:reduce){.meter span{transition:none}}
  .stream-master-toggle{display:inline-flex;align-items:center;gap:8px;height:32px;min-height:32px;max-height:32px;box-sizing:border-box;margin:0;background:#2b2930;border:1px solid rgba(255,255,255,0.08);border-radius:8px;padding:6px 10px 6px 12px;cursor:pointer;color:#d9cede;transition:background-color .2s,border-color .2s,box-shadow .2s;user-select:none}.stream-master-toggle:hover{background:#3a3441;border-color:rgba(155,214,255,0.25)}.stream-master-toggle.active{background:rgba(155,214,255,0.12);border-color:rgba(155,214,255,0.38);box-shadow:0 0 12px rgba(155,214,255,0.15)}.stream-toggle-label{font-size:10px;font-weight:600;letter-spacing:.02em;color:#b0a6bb;transition:color .2s}.stream-master-toggle.active .stream-toggle-label{color:var(--stream-color);font-weight:700}.switch-track{width:28px;height:16px;border-radius:16px;background:#44404d;padding:2px;display:flex;align-items:center;transition:background-color 200ms ease,box-shadow 200ms ease;flex-shrink:0}.switch-track.on{background:var(--stream-color);box-shadow:0 0 8px rgba(155,214,255,0.45)}.switch-thumb{width:12px;height:12px;border-radius:50%;background:#b5a9c2;transition:transform 220ms cubic-bezier(.22,1,.36,1),background-color 200ms ease}.switch-track.on .switch-thumb{transform:translateX(12px);background:#111a24}

  .group{background:#ffffff05;border:1px solid #ffffff0a;border-radius:12px;padding:16px;margin-bottom:14px}
  .group header{display:flex;align-items:baseline;justify-content:space-between;gap:10px;margin-bottom:12px}
  .group h3{margin:0}.group .count{font-weight:500;font-size:10px;color:#9d93aa;margin-left:4px}
  .hint{font-size:10px;color:#9d93aa}.hint-block{font-size:10px;line-height:1.6;margin:0}
  .group label{display:block;color:#d4c6e3;font-size:11px;margin-bottom:8px}
  .dot{display:inline-block;width:7px;height:7px;border-radius:50%;background:var(--source-color,#8d879a);flex-shrink:0}
  .category-options{margin:0}
  .category{gap:7px;padding:7px 11px;border:1px solid transparent}
  .category .check{width:11px;margin-left:2px}
  .category[aria-pressed=true]{background:color-mix(in srgb,var(--source-color) 20%,#1d1b24);color:#f1ebfa;border-color:color-mix(in srgb,var(--source-color) 60%,transparent)}
  .group input[type=search]{margin-bottom:10px}
  .app-list{max-height:300px}
  .app-row{grid-template-columns:30px minmax(0,1fr) auto;gap:12px;padding:9px 4px;border-radius:8px}
  .app-row:hover{background:#ffffff05}.app-row.idle{opacity:.6}
  .app-name strong{overflow-wrap:normal;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}
  .app-name small{display:flex;align-items:center;gap:6px;white-space:nowrap}
  .state{color:#ab9cbb}.state.on{color:#9bd6ff}
  .seg{display:inline-flex;padding:2px;border-radius:8px;background:#15151a;gap:2px}
  .seg button{min-height:24px;padding:4px 9px;border-radius:6px;background:transparent;color:#a99db7;font-size:10px}
  .seg button:hover{background:#ffffff0d;color:#e6dcf2}
  .seg button[aria-pressed=true]{background:#3a3441;color:#f4f0fa;box-shadow:0 1px 2px #0005}
  .seg button[aria-pressed=true]:nth-child(2){background:#9bd6ff26;color:#b6dfff}
  .seg button[aria-pressed=true]:nth-child(3){background:#f0a7c626;color:#f6bdd5}
  .save-status{display:inline-flex;align-items:center;gap:8px;font-size:10px;color:#af9dbf;min-width:0}
  .save-status i{width:7px;height:7px;border-radius:50%;background:#6f667c;flex-shrink:0}
  .save-status[data-state=pending] i,.save-status[data-state=saving] i{background:#edc990}
  .save-status[data-state=saving] i{animation:save-pulse 1s ease-in-out infinite}
  .save-status[data-state=saved]{color:#9fe0c9}.save-status[data-state=saved] i{background:#7be1d1}
  .save-status[data-state=invalid],.save-status[data-state=error]{color:#f0a7c6}
  .save-status[data-state=invalid] i,.save-status[data-state=error] i{background:#f0a7c6}
  @keyframes save-pulse{50%{opacity:.35}}
  @media(prefers-reduced-motion:reduce){.save-status[data-state=saving] i{animation:none}}
  :global(:root[data-theme="light"]) .save-status{color:#526074}
  :global(:root[data-theme="light"]) .save-status[data-state=saved]{color:#00796b}
  :global(:root[data-theme="light"]) .save-status[data-state=invalid],:global(:root[data-theme="light"]) .save-status[data-state=error]{color:#c2185b}
  .how{margin:0;border-radius:12px;background:#ffffff05;padding:12px 16px}
  .how summary{cursor:pointer;font-size:11px;color:#b8acc7}
  .how p{font-size:10px;margin:10px 0 0}
  :global(:root[data-theme="light"]) .group,:global(:root[data-theme="light"]) .how{background:#f3f6fa;border-color:rgba(35,45,65,.08)}
  :global(:root[data-theme="light"]) .seg{background:#e3e9f1}
  :global(:root[data-theme="light"]) .seg button{background:transparent;border:0;color:#526074}
  :global(:root[data-theme="light"]) .seg button[aria-pressed=true]{background:#ffffff;color:#111a28}
  :global(:root[data-theme="light"]) .category[aria-pressed=true]{background:color-mix(in srgb,var(--source-color) 22%,#fff);color:#111a28}
  :global(:root[data-theme="light"]) .hint,:global(:root[data-theme="light"]) .group .count{color:#6a778a}
  :global(:root[data-theme="light"]) .group label{color:#2b394a}
  @media(max-width:720px){.app-row{grid-template-columns:30px minmax(0,1fr)}.seg{grid-column:1/-1;justify-self:start}}

  :global(:root[data-theme="light"]) .meter {
    background: #cfd9e6;
    box-shadow: inset 0 0 0 1px rgba(35, 45, 65, 0.1);
  }
  :global(:root[data-theme="light"]) .meter span {
    background: #1565c0;
  }
  :global(:root[data-theme="light"]) .stream-panel {
    background: linear-gradient(115deg, #eef4f9, #ffffff 65%);
    border: 1px solid rgba(35, 45, 65, 0.1);
    box-shadow: 0 4px 18px rgba(35, 45, 65, 0.05);
  }
  :global(:root[data-theme="light"]) .identity>svg {
    background: rgba(33, 150, 243, 0.12);
  }
  :global(:root[data-theme="light"]) h2 {
    color: #111a28;
  }
  :global(:root[data-theme="light"]) .status {
    background: #e2eaf3;
    color: #354e68;
  }
  :global(:root[data-theme="light"]) .status.sending {
    background: rgba(0, 137, 123, 0.14);
    color: #00796b;
  }
  :global(:root[data-theme="light"]) .mix-summary strong {
    color: #1a2230;
  }
  :global(:root[data-theme="light"]) .mix-summary>span {
    color: #526074;
  }
  :global(:root[data-theme="light"]) .level-control label {
    color: #526074;
  }
  :global(:root[data-theme="light"]) .level-control strong {
    color: #1a2230;
  }
  :global(:root[data-theme="light"]) button {
    background: #e9eef5;
    color: #2b394a;
    border: 1px solid rgba(35, 45, 65, 0.08);
  }
  :global(:root[data-theme="light"]) button:hover {
    background: #dce4ed;
    color: #111a28;
  }
  :global(:root[data-theme="light"]) .stream-master-toggle {
    background: #e9eef5;
    border: 1px solid rgba(35, 45, 65, 0.1);
    color: #2b394a;
  }
  :global(:root[data-theme="light"]) .stream-master-toggle:hover {
    background: #dce4ed;
  }
  :global(:root[data-theme="light"]) .stream-master-toggle.active {
    background: rgba(33, 150, 243, 0.12);
    border-color: rgba(33, 150, 243, 0.35);
  }
  :global(:root[data-theme="light"]) .stream-toggle-label {
    color: #526074;
  }
  :global(:root[data-theme="light"]) .switch-track {
    background: #cbd6e2;
  }
  :global(:root[data-theme="light"]) .switch-thumb {
    background: #ffffff;
  }
  :global(:root[data-theme="light"]) .category {
    background: #e9eef5;
    color: #3b4b5e;
  }
  :global(:root[data-theme="light"]) .category[aria-pressed=true] {
    background: #1976d2;
    color: #ffffff;
  }
  :global(:root[data-theme="light"]) input[type=search] {
    background: #ffffff;
    color: #111a28;
    border: 1px solid rgba(35, 45, 65, 0.15);
  }
  :global(:root[data-theme="light"]) .output-settings :global(.studio-select-trigger),
  :global(:root[data-theme="light"]) .app-row :global(.studio-select-trigger) {
    background: #e9eef5;
    color: #1a2230;
    border: 1px solid rgba(35, 45, 65, 0.1);
  }
  :global(:root[data-theme="light"]) .app-row {
    border-bottom: 1px solid rgba(35, 45, 65, 0.08);
  }
  :global(:root[data-theme="light"]) .app-name strong {
    color: #111a28;
  }
  :global(:root[data-theme="light"]) .app-name small {
    color: #526074;
  }
  :global(:root[data-theme="light"]) form {
    border-top: 1px solid rgba(35, 45, 65, 0.08);
  }
  :global(:root[data-theme="light"]) .settings-footer {
    border-top: 1px solid rgba(35, 45, 65, 0.08);
    color: #526074;
  }
  
  :global(:root[data-theme="light"]) .stream-action-btn {
    background: #e9eef5;
    border: 1px solid rgba(35, 45, 65, 0.1);
    color: #2b394a;
  }
  :global(:root[data-theme="light"]) .stream-action-btn:hover {
    background: #dce4ed;
    border-color: rgba(33, 150, 243, 0.35);
    color: #111a28;
  }
  :global(:root[data-theme="light"]) .stream-playback-btn.engaged {
    background: rgba(33, 150, 243, 0.15);
    color: #0284c7;
    border-color: rgba(33, 150, 243, 0.5);
    box-shadow: 0 0 8px rgba(33, 150, 243, 0.2);
  }
  :global(:root[data-theme="light"]) .stream-mute-btn.muted {
    background: rgba(239, 68, 68, 0.15);
    color: #dc2626;
    border-color: rgba(239, 68, 68, 0.4);
  }
  :global(:root[data-theme="light"]) .settings-button.active {
    background: rgba(33, 150, 243, 0.15);
    color: #0284c7;
    border-color: rgba(33, 150, 243, 0.5);
  }
  :global(:root[data-theme="light"]) p {
    color: #526074;
  }
</style>
