# Stream recording mix

Stream sends a separate stereo recording mix to a dedicated virtual audio cable. Stellars includes a one-click WHQL VB-CABLE + Hi-Fi Cable installer/repair helper and NSIS post-install hook, plus automated 3-slot routing across **Silent Routing Sink**, **Mic Virtual Cable**, and **Stream Virtual Cable**.

## Setup

1. Open **Hardware I/O & Virtual Cable Routing** (or **Stream → Mix settings**) and click **One-click Install / Repair VB-CABLE** (or **Auto-route 3 slots**) to configure non-conflicting virtual cables automatically.
2. Select the categories to include in **Stream → Mix settings**. Defaults are Game, Chat and Mic; Stream starts disabled.
3. Use **Always include** or **Always exclude** for individual apps. **Follow category** removes the override. Turn off every category to build a mix entirely from app inclusions.
4. Choose a dedicated cable's playback endpoint. It must be different from the headphone output, silent routing sink and virtual mic output.
5. Enable Stream and save.
6. In OBS or your recording application, capture that cable's paired recording endpoint (automatically renamed system-wide by Stellars to `Stellars - Stream Mix (Stellars Hi-Fi Virtual Cable)` for OBS and `Stellars - Microphone (Stellars Virtual Audio Cable)` for Discord / games). Capture the Stream mix once and avoid also recording the same desktop/app sources.

The independent Stream mute and level affect the recording output only. Cable changes restart only the Stream renderer. Refresh devices after connecting a cable; saving settings retries a failed Stream endpoint.

## Signal flow

- Apps: separate process-loopback captures → per-source adaptive resampler → Stream sum.
- Mic: existing Mic DSP / level / mute / limiter → independent SPSC tap → adaptive resampler → Stream sum.
- Stream sum → independent level and mute → -0.1 dBFS ceiling soft limiter → stereo peak/RMS meters → 48kHz Float32 shared WASAPI render endpoint.

App audio is captured before monitor/category faders and category DSP. Monitor volume, solo, mute, and Game/Chat balance do not alter app audio in Stream. The mic follows its existing processed signal and mic controls. Stream does not capture the master monitor endpoint, redirect selected apps, or add itself to the playback routing enum.

## Source selection and lifecycle

`RouterConfig.stream` stores `enabled`, `outputDeviceId`, `volume`, `muted`, `categories`, and `appOverrides`. Overrides are keyed by normalized executable name and apply to all matching running instances. An explicit app boolean overrides category membership. Selection deduplicates by PID and excludes Stellars itself. Up to 32 active app PIDs are captured, in PID order. The mic does not consume an app slot.

Source discovery runs about every two seconds while the engine is alive, including when the WebView is closed to the tray. The render callback uses fixed storage, lock-free rings and atomic controls; capture worker setup and selection reconciliation occur on the control thread. Selected categories track current playback assignment. Persisted overrides for stopped apps remain editable in settings.

Older configs deserialize with Stream disabled and no output. Stream never silently falls back to the Windows default device. Output initialization/device-removal failures appear in the Stream panel through `streamError`; meters expose `streamRunning`. The meter subscription resolves the current engine after hardware changes rather than retaining an obsolete engine instance.

## Validation

Rust tests cover old config compatibility, override precedence, normalization, config roundtrip, self-exclusion, deduplication and the capture limit. Browser checks cover missing-cable validation, saving/reopening overrides, level/mute controls and responsive settings. A live recording through an installed dedicated cable remains a hardware integration check.
