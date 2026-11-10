<script lang="ts">
  import { untrack } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import StudioIcon from "./StudioIcon.svelte";

  interface DeviceHealth {
    slot: string;
    deviceId: string | null;
    name: string | null;
    present: boolean;
    mixRateHz: number | null;
  }

  interface DiagnosticsReport {
    appVersion: string;
    engineRateHz: number;
    uptimeSecs: number;
    masterRunning: boolean;
    micRunning: boolean;
    callbackCount: number;
    underrunCount: number;
    cpuLoadPercent: number;
    worstBlockPercent: number;
    deepFilter: { bus: string; state: "loading" | "ready" | "failed" }[];
    devices: DeviceHealth[];
    dataDir: string;
    logPath: string | null;
  }

  interface Props {
    native: boolean;
    /** True while this tab is visible; polling only runs then. */
    active: boolean;
    onNotify: (msg: string) => void;
  }

  let { native, active, onNotify }: Props = $props();

  const POLL_MS = 1500;
  const LOG_EVERY_N_POLLS = 4;
  const SLOT_LABELS: Record<string, string> = {
    monitor: "Monitor output",
    microphone: "Microphone",
    silentSink: "Silent routing sink",
    virtualMic: "Virtual mic cable",
    stream: "Stream cable",
  };

  let report = $state<DiagnosticsReport | null>(null);
  let error = $state<string | null>(null);
  let logLines = $state<string[]>([]);
  let callbacksPerSec = $state<number | null>(null);
  let underrunBaseline = $state<number | null>(null);

  const underrunsSinceOpen = $derived(
    report && underrunBaseline !== null ? Math.max(0, report.underrunCount - underrunBaseline) : 0
  );

  interface Finding {
    level: "ok" | "warn" | "info";
    text: string;
  }

  const findings = $derived.by<Finding[]>(() => {
    if (!report) return [];
    const out: Finding[] = [];
    if (!report.masterRunning) out.push({ level: "warn", text: "The audio engine isn’t running. Check your monitor output in the Audio tab." });
    for (const d of report.devices) {
      const label = SLOT_LABELS[d.slot] ?? d.slot;
      if (!d.present) {
        out.push({ level: "warn", text: `${label} is configured but not connected. Plug it back in or pick another device.` });
      } else if (d.mixRateHz && d.mixRateHz !== report.engineRateHz) {
        out.push({
          level: "info",
          text: `${label} runs at ${formatRate(d.mixRateHz)}; Stellars works at ${formatRate(report.engineRateHz)}. Audio is converted, which can add latency. Set the device to ${formatRate(report.engineRateHz)} in Windows Sound settings for the cleanest path.`,
        });
      }
    }
    if (underrunsSinceOpen > 0) {
      out.push({ level: "warn", text: `${underrunsSinceOpen} audio underrun${underrunsSinceOpen === 1 ? "" : "s"} since you opened this tab. Crackling usually means the PC is busy or a device is at a different sample rate.` });
    }
    if (report.worstBlockPercent >= 80) {
      out.push({ level: "warn", text: `The audio engine used ${Math.round(report.worstBlockPercent)}% of its time budget in the busiest moment. Close heavy apps or switch noise suppression back to Fast to avoid crackling.` });
    }
    for (const d of report.deepFilter) {
      if (d.state === "failed") out.push({ level: "warn", text: `High-quality noise suppression could not start on ${d.bus}; the Fast engine is being used instead.` });
      else if (d.state === "loading") out.push({ level: "info", text: `High-quality noise suppression is still loading on ${d.bus}; the Fast engine covers until it is ready.` });
    }
    if (out.length === 0) out.push({ level: "ok", text: "Everything looks healthy." });
    return out;
  });

  function formatRate(hz: number): string {
    return `${hz / 1000} kHz`;
  }

  function formatUptime(secs: number): string {
    const h = Math.floor(secs / 3600);
    const m = Math.floor((secs % 3600) / 60);
    return h > 0 ? `${h}h ${m}m` : m > 0 ? `${m}m ${secs % 60}s` : `${secs}s`;
  }

  let lastSample: { callbacks: number; at: number } | null = null;

  async function poll(withLog: boolean) {
    try {
      const next = await invoke<DiagnosticsReport>("get_diagnostics");
      const now = performance.now();
      if (lastSample && now > lastSample.at) {
        callbacksPerSec = Math.round(((next.callbackCount - lastSample.callbacks) * 1000) / (now - lastSample.at));
      }
      lastSample = { callbacks: next.callbackCount, at: now };
      if (underrunBaseline === null) underrunBaseline = next.underrunCount;
      report = next;
      error = null;
      if (withLog) logLines = await invoke<string[]>("get_log_tail", { lines: 60 });
    } catch (err) {
      error = String(err);
    }
  }

  $effect(() => {
    if (!active || !native) return;
    untrack(() => {
      lastSample = null;
      underrunBaseline = null;
      callbacksPerSec = null;
    });
    let n = 0;
    void poll(true);
    const timer = setInterval(() => void poll(++n % LOG_EVERY_N_POLLS === 0), POLL_MS);
    return () => clearInterval(timer);
  });

  function buildReport(): string {
    if (!report) return "";
    const lines = [
      `Stellars ${report.appVersion} diagnostics`,
      `Uptime: ${formatUptime(report.uptimeSecs)}`,
      `Engine: master ${report.masterRunning ? "running" : "stopped"}, mic ${report.micRunning ? "running" : "standby"}, ${formatRate(report.engineRateHz)}`,
      `Audio callbacks: ${report.callbackCount} (${callbacksPerSec ?? "?"}/s) · Underruns: ${report.underrunCount}`,
      `Audio load: ${Math.round(report.cpuLoadPercent)}% average, ${Math.round(report.worstBlockPercent)}% worst block`,
      ...(report.deepFilter.length ? [`High-quality noise suppression: ${report.deepFilter.map((d) => `${d.bus} ${d.state}`).join(", ")}`] : []),
      "",
      "Devices:",
      ...report.devices.map(
        (d) =>
          `  ${SLOT_LABELS[d.slot] ?? d.slot}: ${d.name ?? "not connected"}${d.mixRateHz ? ` · ${formatRate(d.mixRateHz)}` : ""}${d.present ? "" : " · MISSING"}`
      ),
      "",
      "Findings:",
      ...findings.map((f) => `  [${f.level}] ${f.text}`),
      "",
      "Recent log:",
      ...logLines,
    ];
    return lines.join("\n");
  }

  async function copyReport() {
    try {
      await navigator.clipboard.writeText(buildReport());
      onNotify("Diagnostics report copied");
    } catch {
      onNotify("Could not copy the report");
    }
  }

  async function openLogs() {
    try {
      await invoke("open_logs_folder");
    } catch (err) {
      error = String(err);
    }
  }
</script>

{#if !native}
  <p class="diag-empty">Diagnostics read the live audio engine, so they’re only available in the desktop app.</p>
{:else}
  <div class="diag">
    {#if error}<div class="diag-alert" role="alert">{error}</div>{/if}

    <div class="diag-stats" aria-live="off">
      <div><span>Engine</span><strong class:good={report?.masterRunning} class:bad={report && !report.masterRunning}>{report ? (report.masterRunning ? "Running" : "Stopped") : "…"}</strong></div>
      <div><span>Microphone</span><strong>{report ? (report.micRunning ? "Active" : "Standby") : "…"}</strong></div>
      <div><span>Callbacks / s</span><strong>{callbacksPerSec ?? "…"}</strong></div>
      <div><span>Audio load</span><strong class:bad={!!report && report.worstBlockPercent >= 80}>{report ? `${Math.round(report.cpuLoadPercent)}% · peak ${Math.round(report.worstBlockPercent)}%` : "…"}</strong></div>
      <div><span>Underruns</span><strong class:bad={underrunsSinceOpen > 0}>{report ? report.underrunCount : "…"}</strong></div>
      <div><span>Uptime</span><strong>{report ? formatUptime(report.uptimeSecs) : "…"}</strong></div>
    </div>

    <section class="diag-section" aria-label="Findings">
      <h4>Health</h4>
      <ul class="findings">
        {#each findings as f}
          <li class={f.level}><span class="dot"></span>{f.text}</li>
        {/each}
      </ul>
    </section>

    <section class="diag-section" aria-label="Devices">
      <h4>Devices</h4>
      <ul class="device-list">
        {#each report?.devices ?? [] as d}
          <li class:missing={!d.present}>
            <span class="slot">{SLOT_LABELS[d.slot] ?? d.slot}</span>
            <span class="name">{d.name ?? "Not connected"}</span>
            <span class="rate" class:mismatch={d.mixRateHz && report && d.mixRateHz !== report.engineRateHz}>{d.mixRateHz ? formatRate(d.mixRateHz) : "—"}</span>
          </li>
        {/each}
      </ul>
    </section>

    <section class="diag-section" aria-label="Log">
      <h4>Recent log</h4>
      <pre class="log" tabindex="0">{logLines.length ? logLines.join("\n") : "No log entries yet."}</pre>
    </section>

    <div class="diag-actions">
      <button type="button" class="cable-install-button" disabled={!report} onclick={copyReport}>
        <StudioIcon name="link" size={14} /> Copy report
      </button>
      <button type="button" class="quiet-button" onclick={openLogs}>
        <StudioIcon name="folder" size={14} /> Open log folder
      </button>
    </div>
  </div>
{/if}

<style>
  .diag { display: flex; flex-direction: column; gap: 20px; }
  .diag-empty { margin: 0; padding: 14px; border-radius: 11px; font-size: 12px; color: var(--text-muted); background: var(--panel-raised); border: 1px solid var(--line); }
  .diag-alert { padding: 10px 12px; border-radius: 9px; font-size: 11px; color: #ffc4d7; background: #f0a7c61a; border: 1px solid #f0a7c633; }
  :global(:root[data-theme="light"]) .diag-alert { color: #9c2f58; }

  .diag-stats { display: grid; grid-template-columns: repeat(auto-fit, minmax(110px, 1fr)); gap: 8px; }
  .diag-stats div { display: flex; flex-direction: column; gap: 4px; padding: 12px; border-radius: 11px; background: var(--panel-raised); border: 1px solid var(--line); }
  .diag-stats span { font-size: 10px; color: var(--text-muted); }
  .diag-stats strong { font-size: 15px; font-weight: 600; letter-spacing: -0.02em; font-variant-numeric: tabular-nums; }
  .diag-stats strong.good { color: var(--teal); }
  .diag-stats strong.bad { color: #f08c7f; }

  .diag-section h4 { margin: 0 0 8px; font-size: 10px; font-weight: 600; letter-spacing: 0.08em; text-transform: uppercase; color: var(--text-muted); }
  .findings, .device-list { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 6px; }
  .findings li { display: flex; gap: 10px; align-items: flex-start; padding: 10px 12px; border-radius: 10px; font-size: 11.5px; line-height: 1.55; background: var(--panel-raised); border: 1px solid var(--line); }
  .findings .dot { flex-shrink: 0; width: 7px; height: 7px; margin-top: 6px; border-radius: 50%; background: var(--text-muted); }
  .findings .ok .dot { background: var(--teal); }
  .findings .warn .dot { background: #f08c7f; }
  .findings .info .dot { background: #edc990; }

  .device-list li { display: grid; grid-template-columns: 150px minmax(0, 1fr) auto; gap: 12px; align-items: center; padding: 10px 12px; border-radius: 10px; font-size: 11.5px; background: var(--panel-raised); border: 1px solid var(--line); }
  .device-list .slot { color: var(--text-muted); }
  .device-list .name { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .device-list .rate { font-variant-numeric: tabular-nums; color: var(--text-secondary); }
  .device-list .rate.mismatch { color: #edc990; }
  .device-list li.missing .name { color: #f08c7f; }

  .log { margin: 0; max-height: 190px; overflow: auto; padding: 12px; border-radius: 10px; font: 10.5px/1.6 ui-monospace, "Cascadia Mono", Consolas, monospace; white-space: pre-wrap; overflow-wrap: anywhere; color: var(--text-secondary); background: var(--panel-inset); border: 1px solid var(--line); }
  .diag-actions { display: flex; gap: 8px; flex-wrap: wrap; }

  @media (max-width: 760px) { .device-list li { grid-template-columns: 1fr auto; } .device-list .slot { grid-column: 1 / -1; } }
</style>
