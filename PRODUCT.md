# Product

<!-- impeccable:product-schema 1 -->

## Platform

web

## Users
Competitive gamers, streamers, and desktop power users on Windows 10 (Build 20348+) and Windows 11 who route application audio across dedicated submixes (`Game`, `Chat`, `Media`, `Aux`) and a low-latency microphone DSP pipeline (`Mic`) in a dimly lit desk or studio environment during active gameplay, voice comms, or broadcast sessions.

## Product Purpose
Stellars is a lightweight, 100% Ring-3 user-mode multi-bus audio router and real-time DSP mixer for Windows. It provides per-application audio capture and routing, 10-band cascaded parametric equalization, noise gating, soft-knee compression, and Game/Chat crossfading without bloated background processes or kernel drivers.

## Positioning
Zero-kernel-driver, anti-cheat-safe (`PROCESS_QUERY_LIMITED_INFORMATION` only) multi-bus audio routing and real-time 48 kHz Float32 DSP mixing powered by `VAD\Process_Loopback` per-PID capture and `IAudioPolicyConfig` silent dummy-sink redirection, running under 15 MB RAM when closed to the Windows system tray.

## Operating Context
Operated on Windows desktops alongside full-screen or borderless games (`cs2.exe`), voice chat (`discord.exe`), and media players (`spotify.exe`). Users open the console to drag-and-drop or quick-assign active processes to buses, balance Game vs. Chat audio via the ChatMix crossfader, solo/mute channels, fine-tune faders with scroll-wheel or double-click reset to unity, and sculpt 10-band parametric EQ curves or dynamics stages with immediate real-time visual feedback.

## Capabilities and Constraints
- **Stream Recording Mix:** A separate 48kHz stereo WASAPI output for a dedicated streaming cable. Include Game/Chat/Media/Aux categories and the processed Mic, with persistent executable-level Include/Exclude overrides. App captures are independent of monitor faders, category DSP, Solo/Mute and ChatMix; Stream has its own level, mute, peak meters and limiter. Disabled by default; supports up to 32 active captured apps. Source discovery remains active in the tray. See `.docs/STREAM_OUTPUT.md` for setup and signal flow.
- **5 Logical Audio Buses:** 4 playback buses (`Game`, `Chat`, `Media`, `Aux`) + 1 hardware microphone pipeline (`Mic`).
- **Real-Time DSP Pipeline:** 48,000 Hz stereo Float32, 10 ms frame period, Pre-Gain (-12 to +12 dB), 10-band RBJ Biquad Parametric EQ (Direct Form II Transposed with 128-frame zipper-free linear ramping), Noise Gate, Soft-Knee Compressor, Post-Gain fader (0% to 150%), Solo/Mute, ChatMix quadratic crossfader, and Master `-0.1 dBFS` soft-limiter.
- **Interactive Routing & QoL:** Bidirectional drag-and-drop process routing across buses and the unrouted pool with drop-zone previews, single-click/keyboard routing buttons, process search/filtering, persistent routing rules inspector, double-click fader/knob reset to unity, scroll-wheel fine adjustment, EQ curve presets (Flat, FPS Footsteps, Voice Clarity, Bass Boost), and a collapsible hardware I/O configuration drawer.
- **Technical Boundaries:** Preserve the existing Tauri v2 IPC command and `Channel<AllBusMetersPayload>` streaming contracts, 100% Ring-3 WASAPI architecture, and WebView2 tray lifecycle.

## Brand Commitments
- Product name: **Stellars** (renamed from Stellars Sonar).

## Evidence on Hand
- Functional Tauri v2 + Svelte 5 codebase (`src/App.svelte`, `src/components/MixerView.svelte`, `src/components/EqCanvas.svelte`, `src/dspMath.ts`, `src/types.ts`, and `src-tauri/`).
- Real-time RBJ biquad magnitude response math (`computeBiquadCoeffs`, `evaluateBandMagnitudeDb`) and ChatMix gain law (`computeChatMixGains`).

## Product Principles
1. **Zero-Friction Routing:** Assigning or unrouting an active Windows audio process takes a single drag gesture or one click, with unmistakable drop targets and active routing rule visibility.
2. **Tactile Precision:** Every fader, EQ node, and crossfader responds immediately with fine-grained control, scroll-wheel trim, and instant double-click reset to unity (0 dB / center).
3. **High-Density Signal Clarity:** Channel strips, stereo Peak/RMS telemetry, and the 10-band parametric frequency response remain scannable at a glance without modal clutter or hidden primary controls.
4. **Uncompromising Runtime Discipline:** Respect the lock-free 48 kHz audio engine and lightweight WebView2 lifecycle with smooth, GPU-friendly UI rendering and zero layout thrashing.

## Accessibility & Inclusion
- Single-pointer and keyboard routing alternatives alongside drag-and-drop (WCAG 2.2 AA Dragging Movements).
- High-contrast text and telemetry readouts (≥4.5:1 body text contrast, tabular numerals, clear focus rings, and `prefers-reduced-motion` support).
