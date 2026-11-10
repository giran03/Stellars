<div align="center">

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="brand-kit/stellars-lockup-horizontal-dark.svg">
  <img src="brand-kit/stellars-lockup-horizontal-color.svg" alt="Stellars" width="420">
</picture>

# Stellars

**Give every app its own mix. Shape each one in real time.**

![Platform](https://img.shields.io/badge/platform-Windows%2010%20%7C%2011-b8a1fa?style=flat-square)
![Version](https://img.shields.io/badge/version-1.0.0-87ddcf?style=flat-square)
![Built with](https://img.shields.io/badge/built%20with-Rust%20%C2%B7%20Tauri%20%C2%B7%20Svelte-9bd6ff?style=flat-square)

</div>

Stellars is a multi-bus audio router and mixer for Windows. It sorts your apps into five channels (Game, Chat, Media, Aux and Mic), lets you balance and process each one live, and can send a separate mix to your stream or recording software. It runs entirely in user mode and installs no kernel driver of its own.

<p align="center">
  <img src="docs/media/stellars-showcase-16x9.gif" alt="Stellars in action: dragging Discord into the Chat channel, balancing Game and Chat with ChatMix, switching the EQ to FPS Footsteps, turning on the noise gate and the Stream mix" width="100%">
</p>

## Features

### Routing
- **Five channels:** Game, Chat, Media and Aux for your apps, plus a Mic channel for your microphone.
- **Drag, drop or click:** drag any running app onto a channel, or pick a channel from the app's menu. Drag it back to the pool to restore Windows' default.
- **Per-app control:** each app has its own volume, and routing rules are remembered between sessions.
- **Scenes and output profiles:** save a full mix and bring it back in one click. Scenes can apply automatically when a chosen app comes into focus.
- **ChatMix:** a single crossfader to balance Game against Chat.

### Sound shaping
- **10-band parametric EQ** on every channel, with presets, A/B comparison and custom presets.
- **Per-channel processing:** noise gate, compressor, de-esser and limiter, with live gain-reduction readouts and transfer curves.
- **AI noise suppression** for Mic and Chat, with a fast engine (RNNoise) and a high-quality engine (DeepFilterNet3).
- **Auto-ducking:** lower chosen channels automatically while another is active.
- **Quick setup recipes:** one-click starting points such as Streaming voice, Noisy room and Hear footsteps.
- **Live metering:** peak, RMS and LUFS meters with clip indicators on every channel, plus a master limiter.

### Stream mix
- **A separate recording mix** sent to its own virtual audio cable, so your stream hears what you choose without affecting what you hear.
- **Choose what goes in:** include whole channels or individual apps, with per-app overrides, its own level, mute and limiter.
- **Solo headphones:** listen to the stream mix on your monitor output without changing your saved mix.

### Everyday use
- **Simple and Advanced modes:** start with the essentials and reveal every control when you want it.
- **Light and dark themes** with a tactile switch, or press `Alt+T`.
- **Lives in the system tray:** closing the window releases the interface and leaves only the audio engine running. The tray menu opens beside the icon with quick actions, including mute-all.
- **Resource saver:** meters slow down when the window is in the background and stop while it is hidden. Audio is never affected.
- **Reconnects devices automatically** when a headset, microphone or cable comes back.
- **Automatic backups** of your settings and presets, with one-click restore.
- **Built-in diagnostics and logs** for audio load, underruns and device state.

## Requirements

- Windows 11, or Windows 10 build 20348 or later (64-bit)
- Microsoft Edge WebView2 (installed automatically if missing)
- A virtual audio cable for the Stream and Mic routes. The installer sets up VB-CABLE for you, and Stellars can repair or re-route it later from **Hardware I/O**.

## Download and install

1. Download the latest installer from the [Releases page](https://github.com/giran03/Stellars/releases/latest).
2. Run it and follow the prompts.
3. Open Stellars. The first-run wizard helps you choose an interface mode and set up your devices.

## Quick start

1. **Play something.** Start a game, a call or some music so the app shows up in the app list.
2. **Route it.** Drag the app onto a channel, or open its menu and choose one.
3. **Balance it.** Move the channel fader, press **S** to solo or the speaker to mute, and open **Edit channel** to shape the EQ and processing.
4. **Add a stream mix** (optional). Open **Stream**, choose what to include, pick your virtual cable as the output and switch it on. Then select that cable as the audio source in your streaming software.
5. **Close to the tray.** Stellars keeps routing in the background. Left-click the tray icon to reopen the window, right-click for quick actions.

---

<div align="center">

Designed and built by **CafeTako Team**.

</div>
