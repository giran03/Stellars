<script lang="ts">
  import { untrack } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import StudioIcon from "./StudioIcon.svelte";
  import StudioSelect from "./StudioSelect.svelte";
  import type { AudioEndpointInfo, RouterConfig, VirtualCableStatusPayload } from "../types";

  interface Props {
    /** True when the desktop engine is available (enables NVIDIA lookups). */
    native?: boolean;
    /** True while the tab is visible so NVIDIA status is refreshed on open. */
    active?: boolean;
    config: RouterConfig;
    endpoints: AudioEndpointInfo[];
    cableStatus?: VirtualCableStatusPayload;
    cableInstalling?: boolean;
    cableAutoRouting?: boolean;
    onChangeDevices: (
      masterId: string | null,
      dummyId: string | null,
      micId: string | null,
      virtMicId: string | null,
      streamOutId?: string | null
    ) => void;
    onInstallOrRepairCables?: (forceRepair?: boolean) => void;
    onAutoRouteCables?: () => void;
    onRefreshDevices?: () => void;
    onNotify: (msg: string) => void;
  }

  let {
    native = false,
    active = true,
    config,
    endpoints,
    cableStatus,
    cableInstalling = false,
    cableAutoRouting = false,
    onChangeDevices,
    onInstallOrRepairCables,
    onAutoRouteCables,
    onRefreshDevices,
    onNotify,
  }: Props = $props();

  const renderEndpoints = () => endpoints.filter((e) => e.flow === "Render");
  const captureEndpoints = () => endpoints.filter((e) => e.flow === "Capture");

  // NVIDIA overlay / ShadowPlay recording microphone (only shown when NVIDIA recording is enabled)
  interface NvidiaRecordingStatus {
    available: boolean;
    micDeviceId: string | null;
    micDeviceName: string | null;
  }
  let nvidia: NvidiaRecordingStatus | null = $state(null);
  let nvidiaSaving = $state(false);

  async function loadNvidiaStatus() {
    if (!native) return;
    try {
      nvidia = await invoke<NvidiaRecordingStatus>("get_nvidia_recording_status");
    } catch {
      nvidia = null;
    }
  }

  async function changeNvidiaMic(deviceId: string) {
    nvidiaSaving = true;
    try {
      nvidia = await invoke<NvidiaRecordingStatus>("set_nvidia_recording_mic", {
        deviceId: deviceId || null,
      });
      onNotify("NVIDIA recording microphone updated. Reopen the NVIDIA overlay if it still shows the old device.");
    } catch (err) {
      onNotify(`Could not update NVIDIA microphone: ${String(err)}`);
      await loadNvidiaStatus();
    } finally {
      nvidiaSaving = false;
    }
  }

  $effect(() => {
    if (active) untrack(() => void loadNvidiaStatus());
  });
</script>

<div class="hardware-settings" aria-label="Hardware and virtual cable configuration">
  <div class="hardware-toolbar">
    <div class="hardware-badges">
      <span class="cable-badge" class:ready={cableStatus?.vbCableInstalled}>
        <span class="badge-dot"></span>
        {cableStatus?.vbCableInstalled ? "VB-CABLE Active" : "VB-CABLE Missing"}
      </span>
      <span class="cable-badge" class:ready={cableStatus?.hifiCableInstalled}>
        <span class="badge-dot"></span>
        {cableStatus?.hifiCableInstalled ? "Hi-Fi Cable Active" : "Hi-Fi Cable Optional"}
      </span>
    </div>
    <div class="hardware-actions">
      <button
        type="button"
        class="cable-install-button"
        disabled={cableInstalling}
        data-tooltip="Downloads and installs or repairs official WHQL-signed VB-CABLE + Hi-Fi Cable drivers (Administrator UAC) and auto-routes Silent Sink, Mic, and Stream."
        onclick={() => onInstallOrRepairCables?.(Boolean(cableStatus?.vbCableInstalled && cableStatus?.hifiCableInstalled))}
      >
        <StudioIcon name="plus" size={14} />
        {cableInstalling
          ? "Installing / Repairing…"
          : cableStatus?.vbCableInstalled && cableStatus?.hifiCableInstalled
            ? "One-click Repair VB-CABLE"
            : "One-click Install / Repair VB-CABLE"}
      </button>
      <button
        type="button"
        class="quiet-button"
        disabled={cableInstalling || cableAutoRouting}
        data-tooltip="Automatically assign non-conflicting virtual cables across Silent Routing Sink, Virtual Mic, and Stream Mix."
        onclick={() => onAutoRouteCables?.()}
      >
        <StudioIcon name="route" size={14} />
        {cableAutoRouting ? "Routing…" : "Auto-route 3 slots"}
      </button>
      <button type="button" class="quiet-button" aria-label="Refresh audio devices" onclick={() => onRefreshDevices?.()}>
        <StudioIcon name="reset" size={13} />
        Refresh
      </button>
    </div>
  </div>

  <div class="hardware-physical-grid">
    <label class="io-field">
      <span class="io-field-head">
        <strong>Monitor output</strong>
        <small>Physical headphones / speakers</small>
      </span>
      <StudioSelect
        aria-label="Monitor output"
        value={config.masterOutputDeviceId ?? ""}
        onchange={(e) =>
          onChangeDevices(
            e.currentTarget.value || null,
            config.dummySinkDeviceId,
            config.physicalMicDeviceId,
            config.virtualMicOutputDeviceId,
            config.stream.outputDeviceId
          )}
      >
        <option value="">System default playback</option>
        {#each renderEndpoints() as ep}
          <option value={ep.id}>{ep.name}</option>
        {/each}
      </StudioSelect>
    </label>

    <label class="io-field">
      <span class="io-field-head">
        <strong>Microphone input</strong>
        <small>Physical hardware microphone</small>
      </span>
      <StudioSelect
        aria-label="Microphone input"
        value={config.physicalMicDeviceId ?? ""}
        onchange={(e) =>
          onChangeDevices(
            config.masterOutputDeviceId,
            config.dummySinkDeviceId,
            e.currentTarget.value || null,
            config.virtualMicOutputDeviceId,
            config.stream.outputDeviceId
          )}
      >
        <option value="">System default microphone</option>
        {#each captureEndpoints() as ep}
          <option value={ep.id}>{ep.name}</option>
        {/each}
      </StudioSelect>
    </label>
  </div>

  {#if nvidia?.available}
    <div class="hardware-physical-grid nvidia-section" role="group" aria-label="NVIDIA recording audio">
      <label class="io-field">
        <span class="io-field-head">
          <strong>NVIDIA recording microphone</strong>
          <small>ShadowPlay / Instant Replay / overlay recording</small>
        </span>
        <StudioSelect
          aria-label="NVIDIA recording microphone"
          disabled={nvidiaSaving}
          value={captureEndpoints().find((ep) => ep.id === nvidia?.micDeviceId)?.id ?? ""}
          onchange={(e) => changeNvidiaMic(e.currentTarget.value)}
        >
          <option value="">System default microphone</option>
          {#each captureEndpoints() as ep}
            <option value={ep.id}>{ep.name}</option>
          {/each}
        </StudioSelect>
      </label>
    </div>
  {/if}

  <div class="virtual-routes-grid" role="group" aria-label="Virtual cable routing slots">
    <div class="route-card">
      <div class="route-card-head">
        <span class="route-dot sink-dot"></span>
        <strong>Silent routing sink</strong>
        <span
          class="route-pill"
          class:ready={cableStatus?.silentSink.status === "ready"}
          class:conflict={cableStatus?.silentSink.status === "conflict" || cableStatus?.silentSink.status === "missing"}
        >
          {cableStatus?.silentSink.status === "ready"
            ? "Active"
            : cableStatus?.silentSink.status === "conflict"
              ? "Conflict"
              : cableStatus?.silentSink.status === "missing"
                ? "Missing"
                : "Unassigned"}
        </span>
      </div>
      <label class="route-card-select">
        <span class="sr-only">Silent routing sink</span>
        <StudioSelect
          aria-label="Silent routing sink"
          value={config.dummySinkDeviceId ?? ""}
          onchange={(e) =>
            onChangeDevices(
              config.masterOutputDeviceId,
              e.currentTarget.value || null,
              config.physicalMicDeviceId,
              config.virtualMicOutputDeviceId,
              config.stream.outputDeviceId
            )}
        >
          <option value="">None · dry audio not muted</option>
          {#each renderEndpoints() as ep}
            <option value={ep.id}>{ep.name}</option>
          {/each}
        </StudioSelect>
      </label>
      <p
        class="route-hint"
        class:conflict={cableStatus?.silentSink.status === "conflict" || cableStatus?.silentSink.status === "missing"}
      >
        {cableStatus?.silentSink.hint ?? "Silences raw dry app audio for Game, Chat, Media, and Aux."}
      </p>
    </div>

    <div class="route-card">
      <div class="route-card-head">
        <span class="route-dot mic-dot"></span>
        <strong>Mic virtual cable</strong>
        <span
          class="route-pill"
          class:ready={cableStatus?.virtualMic.status === "ready"}
          class:conflict={cableStatus?.virtualMic.status === "conflict" || cableStatus?.virtualMic.status === "missing"}
        >
          {cableStatus?.virtualMic.status === "ready"
            ? "Routed"
            : cableStatus?.virtualMic.status === "conflict"
              ? "Conflict"
              : cableStatus?.virtualMic.status === "missing"
                ? "Missing"
                : "Disabled"}
        </span>
      </div>
      <label class="route-card-select">
        <span class="sr-only">Virtual microphone output</span>
        <StudioSelect
          aria-label="Virtual microphone output"
          value={config.virtualMicOutputDeviceId ?? ""}
          onchange={(e) =>
            onChangeDevices(
              config.masterOutputDeviceId,
              config.dummySinkDeviceId,
              config.physicalMicDeviceId,
              e.currentTarget.value || null,
              config.stream.outputDeviceId
            )}
        >
          <option value="">Disabled</option>
          {#each renderEndpoints() as ep}
            <option value={ep.id}>{ep.name}</option>
          {/each}
        </StudioSelect>
      </label>
      <p
        class="route-hint"
        class:conflict={cableStatus?.virtualMic.status === "conflict" || cableStatus?.virtualMic.status === "missing"}
      >
        {cableStatus?.virtualMic.hint ?? "Feeds processed Mic (Gate + EQ + Comp) into Discord & games."}
      </p>
    </div>

    <div class="route-card">
      <div class="route-card-head">
        <span class="route-dot stream-dot"></span>
        <strong>Stream virtual cable</strong>
        <span
          class="route-pill"
          class:ready={cableStatus?.streamMix.status === "ready"}
          class:conflict={cableStatus?.streamMix.status === "conflict" || cableStatus?.streamMix.status === "missing"}
        >
          {cableStatus?.streamMix.status === "ready"
            ? config.stream.enabled
              ? "Sending"
              : "Ready"
            : cableStatus?.streamMix.status === "conflict"
              ? "Conflict"
              : cableStatus?.streamMix.status === "missing"
                ? "Missing"
                : "Unassigned"}
        </span>
      </div>
      <label class="route-card-select">
        <span class="sr-only">Stream virtual cable output</span>
        <StudioSelect
          aria-label="Stream virtual cable output"
          value={config.stream.outputDeviceId ?? ""}
          onchange={(e) =>
            onChangeDevices(
              config.masterOutputDeviceId,
              config.dummySinkDeviceId,
              config.physicalMicDeviceId,
              config.virtualMicOutputDeviceId,
              e.currentTarget.value || null
            )}
        >
          <option value="">Select stream cable…</option>
          {#each renderEndpoints() as ep}
            <option value={ep.id}>{ep.name}</option>
          {/each}
        </StudioSelect>
      </label>
      <p
        class="route-hint"
        class:conflict={cableStatus?.streamMix.status === "conflict" || cableStatus?.streamMix.status === "missing"}
      >
        {cableStatus?.streamMix.hint ?? "Feeds independent Stream recording mix into OBS Studio."}
      </p>
    </div>
  </div>
</div>
