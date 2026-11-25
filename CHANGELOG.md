# Changelog

All notable changes to Stellars are recorded here, newest version first.

## Versioning and release rules

- **One development version is open at a time.** Every change goes into its section below, under the matching heading, as it is made.
- The development version number stays the same until the maintainer asks to **build and release the app to GitHub**. Only then does the version move on.
- When that release happens: the open section is closed with the release date, the build is tagged `vX.Y.Z` and published to GitHub, the version in `package.json`, `src-tauri/Cargo.toml` and `src-tauri/tauri.conf.json` is bumped to the next development version (a patch bump unless told otherwise), and a new empty section for it is opened at the top of this file.
- All changes made after that point are logged in the new section.

---

## [1.0.4] - in development

### Added

- Settings → Audio shows the Windows default output and input, and lets you change them there.
- Double-click the Stream level slider to reset it to 100%.
- A quick device picker: a swap button on Monitor output and on the microphone name in the Mic channel opens a pop-up to choose the headphones/speakers or the microphone without opening Settings. It includes a "Windows default" choice.
- The tray menu shows a live status in its header (a pulsing dot while audio is running, "All muted" / "Mic muted" when muted) and keeps itself up to date while it is open.
- The tray menu has a hint on each row where it helps ("Left click" on Open Stellars, "None saved" when there are no scenes) and groups its actions into clear sections.
- A "System" theme option follows the Windows light or dark setting and changes live with it (Settings → Appearance).
- The tray icon shows a small badge when audio is muted (red for all channels, amber for the microphone only), and its tooltip says so.
- After Stellars updates, a notice offers a "What's new" button that opens the change log.

### Changed

- The Monitor output slider and its stereo level meter are now one control: the meter sits right under the slider and a bar turns red when that channel clips.
- The Monitor output slider now sets the Windows volume of the output device itself (0 to 100%, the same level as the Windows volume mixer) instead of a separate 0 to 150% boost, and follows the volume keys and the Windows mixer. A Device / Gain switch next to the title changes the slider to the old software gain (0 to 150%) and back. When you update, the software gain is reset to 100% once.
- When the output or microphone in use is unplugged, Stellars moves to the Windows default device right away and says so, then switches back by itself when the device returns (needs "Reconnect devices automatically").
- Only one copy of Stellars can run at a time. Starting it again brings the running window to the front instead of opening a second one.
- The tray menu is now a lightweight native Windows menu that follows the app's light or dark theme. It opens instantly and no longer starts a background web view.

### Fixed

- Tooltips no longer get stuck half see-through; they always finish fading in.

### Removed

- The "NVIDIA recording microphone" setting (Settings → Audio). Stellars no longer reads or changes NVIDIA's recording microphone; pick it in the NVIDIA overlay instead.

---

## [1.0.3] - 2026-11-21

### Added

### Changed

### Fixed

- Install and restart now reopens Stellars after the update installs; before, it closed and stayed closed.
- The update notice is cleaner: release notes show as a short list without checksums or stray symbols, and the "ready to install" box lays out its text and button properly.

### Removed

---

## [1.0.2] - 2026-11-21

### Added

- Known games are now routed to the Game category automatically the first time they play sound. Stellars ships with a built-in list of game program names; apps you have already placed in a category are never moved.
- Ctrl+Q closes the Stellars window (following your close setting). It only works while Stellars is focused.

### Changed

### Fixed

### Removed

---

## [1.0.1] - 2026-11-11

### Added

- Uninstalling Stellars now also removes the VB-CABLE and Hi-Fi Cable drivers that Stellars installed (cables you already had are left alone) and restores your audio to how it was before Stellars: your original default speakers and microphone, every app's output back to the Windows default, and any output Stellars had muted unmuted. Updating Stellars keeps the cables.
- **Restore Windows audio** (Settings → Diagnostics → Repair, and the tray menu): stops Stellars' routing and puts Windows audio back as it was before Stellars. Every app plays to its normal device again, a muted silent sink is unmuted and your original default speakers and microphone return. Your assignments are kept, and one click resumes routing.
- **Crash recovery:** if Stellars did not close properly last time, app audio routing is reset at the next start so no app stays silent, and you are told about it.
- **Setup checks** on the Diagnostics page and in the setup wizard (new "Check" step): cables, silent sink, virtual mic, stream output and whether a virtual cable has taken over your Windows default devices, each with what to do about it.
- **Delay estimate** on the Diagnostics page.
- **Export support file** (Diagnostics): one file with diagnostics, settings and the recent log, with Windows user names removed, ready to attach to a bug report.
- **Import and export of scenes, output profiles and saved sound presets** (Settings → Presets → Share). Imported items are added next to yours and never replace them; output devices from another PC are left unset.
- **Tray quick actions:** mute or unmute the microphone, switch to the next scene, and restore Windows audio.
- **Automatic updates.** Stellars now checks for a new version shortly after it starts and every few hours, and tells you when one is available. In Settings → Updates you can download the update (it is checked against the release's checksum), then choose Install and restart. Windows asks for permission, Stellars closes, updates and reopens by itself.
- **Automatic updates setting** (Settings → Updates): Off, Notify me (the default) or Automatic, which also installs a new version once your PC has been idle for ten minutes. If you had automatic checks turned off before, they stay off.

### Changed

### Fixed

### Removed

---

## [1.0.0] - 2026-11-11

### Added

**Mixer and channels**
- Stream mix with its own per-channel send faders, mute/included switches and app overrides; stream mic source defaults to the virtual mic output, with a dropdown for the raw mic.
- Stream playback ("solo headphones") that follows each channel's Stream fader and the stream master level, for all five channels including Mic. It is live only and never changes the saved monitor mix.
- Per-channel meters for the stream: when the stream mix is on, each channel card shows a Monitor meter and a Stream meter side by side, each with Left and Right columns.
- Channel processing panel under the EQ showing the signal chain (pre-gain, noise cut, EQ, de-esser, gate, compressor, fader, limiter) with live gain-reduction readouts and gate/compressor transfer curves.
- Limiter, de-esser and AI noise suppression per channel, plus LUFS meters and gain-reduction telemetry.
- Auto-ducking, scenes and presets, backups, and persistent logging.

**Noise suppression**
- Second engine for Mic and Chat: **High quality (DeepFilterNet3)** next to the existing **Fast (RNNoise)**. It runs on its own worker thread, falls back to RNNoise while loading or if it fails, and adds about 45 ms of delay. Choose it with the new Engine switch.

**Fine-tune and presets**
- One-click Quick setup recipes for each channel (for example Streaming voice, Noisy room, Hear footsteps, Plain) and per-section presets for noise suppression, gate, compressor, de-esser and limiter, plus saved user presets (included in backups).
- "Plain (no effects)" also turns the EQ off.
- Fine-tune is a full-width section below the EQ, with one card per effect.

**EQ**
- Channel EQ A/B comparison, custom EQ presets and a Simple-mode tone control with three sliders (Bass, Voice clarity, Treble).
- EQ editing is locked while the EQ is bypassed.

**Interface**
- Simple and Advanced interface modes: new installs start in Simple, with a first-run wizard step to choose and a side prompt if the wizard is skipped. Settings > Appearance can change it later.
- Redesigned sidebar with labels, a sliding selection indicator, scroll tracking and badges; sidebar jumps leave a 24px gap above the section.
- Back-to-top button at the left of the footer.
- "Edit channel" on a channel card now also scrolls to that channel's Fine-tune section.
- Change log button in Settings > Updates: opens this changelog inside the app, newest version first.
- Diagnostics: audio load (average and worst block), working underrun counter, and the state of the high-quality noise engine.
- Save resources in the background (Settings > General > Performance, on by default): the live meters slow down after the window has been unfocused for 5 seconds, stop while it is minimized or hidden in the tray, and decorative animations pause. Audio is never affected, and everything returns to full speed the moment the window is used again. Silent meters are no longer re-sent while the window is in use.
- Click-and-drag scrolling: hold the left mouse button on empty space and drag up or down to scroll the page or any scrollable list, with smooth damping and a short momentum glide on release. Buttons, sliders, graphs and draggable items are unaffected.

### Changed
- A spare output you don't listen to, such as a monitor, can now be the silent sink. The Stream cable card offers this when both virtual cables are in use: pick the output and Stellars mutes it, uses it as the silent sink and moves the freed cable to the Stream Mix. Stellars keeps that output muted, and unmutes it when it stops being the silent sink.
- Stream settings redesigned with app icons, a calmer layout, and autosave with status feedback.
- Default window width 1280.
- Settings changed with faders, mutes and the EQ are saved once, shortly after the last change, and again when the app quits, so a crash can no longer leave a damaged settings file.
- App volume, Stream level and EQ sliders share one consistent slider style.
- Light theme: higher-contrast Left/Right meters on Monitor output and Stream level, and theme-aware styling for all new additions.
- The version shown in Settings now always matches the real app version (1.0.0).
- Strong noise-gate preset now uses a -18 dB threshold.
- Fine-tune heading and padding match Channel processing.
- Tray icon right-click menu is now a styled popup that follows the app's light/dark theme, opens beside the tray icon (above the taskbar, like Steam or Discord) without covering it, closes when you click elsewhere or press Esc, and shows whether all channels are muted.
- Text in labels, descriptions and other interface text can no longer be highlighted; text fields, error messages and log/code blocks remain selectable.
- Check for updates (Settings > Updates) now looks at the latest release published on the Stellars GitHub page, and the Download button opens that release.
- About page: the developer description now reads "Designed and built by CafeTako Team".
- Fine-tune effect cards size to their content and pack tightly, with the same gap between cards in both directions.
- Cleaner interface: removed taglines and repeated explanations (page slogan, "Made for the way you listen", section notes, routing and Fine-tune intro lines, duplicate EQ hints, "Not used on this channel" and the Plain setup descriptions, and the placeholder note in Updates).

### Fixed
- The virtual cable setup now only accepts the VB-Audio driver packages it expects: downloads must use HTTPS and match a known checksum, the plain-HTTP fallback mirror is gone, and an installer only runs when it carries VB-Audio's valid digital signature.
- Stack overflow at startup (the main thread now has an 8 MiB stack).
- Window dragging from the custom title bar.
- Stream playback ignoring the Stream faders; overlapping updates while dragging could leave a stale level.
- The audio underrun counter never counted; it now counts real gaps.
- Left-clicking the tray icon no longer opens the context menu (it opens the window; right-click shows the menu).
- The Stream badge in the sidebar now disappears when the Stream mix is turned off and returns when it is turned on.
- The installer now also names the virtual audio cables it installs, after the role each one gets on first launch (for example Stellars - Microphone), so the names in Discord and OBS are right from the start and do not change when Stellars opens.
- Installing the virtual cables no longer takes over your speakers or microphone: if Windows switches the default playback or recording device to a new cable, Stellars puts your own device back.
- The window no longer freezes while One-click Install / Repair downloads and installs the cables, or while Auto-route reassigns them.
- No console window flashes during virtual cable setup.
- When the cable drivers need a restart before Windows shows them, the installer now offers to restart on its last page.
- The cable cards now update right after you change a device, instead of showing the old status.
- The Stream cable card now explains when there is no spare virtual cable for a Stream Mix (both cables already carry the silent sink and the mic) instead of suggesting a cable that would only cause a conflict.

### Removed
- The redundant Audio Settings button in the header.
- The Audio button at the bottom of the sidebar; audio settings are still under Settings > Audio.
- The Virtual mic output Left/Right level feature. Old config files that still contain those values load normally.
