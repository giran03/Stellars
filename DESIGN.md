---
name: Stellars Studio
description: A rounded charcoal audio console with violet controls, teal output and dot-matrix activity.
colors:
  canvas: "#111113"
  surface: "#1c1c20"
  ink: "#f4f3f7"
  muted: "#a2a1ac"
  violet: "#b8a1fa"
  teal: "#87ddcf"
  stream-blue: "#9bd6ff"
  stream-status: "#b6dfff"
  stream-save-ink: "#1e2832"
  stream-save: "#a9d8f8"
  game: "#b7a3ff"
  chat: "#7be1d1"
  media: "#a1baff"
  aux: "#edc990"
  mic: "#f0a7c6"
  quiet: "#29282e"
  quiet-ink: "#d5d2df"
  save: "#cdb4f3"
  save-ink: "#231a2d"
  control-fill: "#2b2632"
  control-ink: "#dfd4eb"
  control-hover: "#383040"
  menu-fill: "#29242f"
  menu-ink: "#eee5f7"
  help-fill: "#302936"
  help-ink: "#e6dced"
  precise-fill: "#32283f"
  precise-ink: "#f0e7fa"
  precise-error: "#ffc4d7"
typography:
  headline:
    fontFamily: "Plus Jakarta Sans, sans-serif"
    fontSize: "29px"
    fontWeight: 500
    lineHeight: 1.3
    letterSpacing: "-0.04em"
  title:
    fontFamily: "Plus Jakarta Sans, sans-serif"
    fontSize: "15px"
    fontWeight: 600
    letterSpacing: "-0.025em"
  body:
    fontFamily: "Plus Jakarta Sans, sans-serif"
    fontSize: "12px"
    fontWeight: 400
  label:
    fontFamily: "Plus Jakarta Sans, sans-serif"
    fontSize: "10px"
    fontWeight: 400
  readout:
    fontFamily: "Plus Jakarta Sans, sans-serif"
    fontSize: "24px"
    fontWeight: 400
    letterSpacing: "-0.04em"
  control-label:
    fontFamily: "Plus Jakarta Sans, sans-serif"
    fontSize: "11px"
    fontWeight: 400
    lineHeight: 1.5
  option:
    fontFamily: "Plus Jakarta Sans, sans-serif"
    fontSize: "12px"
    fontWeight: 500
    lineHeight: 1.5
  tooltip:
    fontFamily: "Plus Jakarta Sans, sans-serif"
    fontSize: "11px"
    fontWeight: 400
    lineHeight: 1.65
rounded:
  precise: "5px"
  menu: "12px"
  thumb: "4px"
  field: "6px"
  control: "7px"
  quiet: "8px"
  search: "9px"
  app: "10px"
  nav: "14px"
  panel: "16px"
  channel: "18px"
spacing:
  compact: "8px"
  channel-gap: "11px"
  related: "12px"
  inset: "16px"
  panel-gap: "22px"
  workspace: "35px"
components:
  select-trigger:
    backgroundColor: "{colors.control-fill}"
    textColor: "{colors.control-ink}"
    rounded: "{rounded.quiet}"
    padding: "9px 11px"
    typography: "{typography.control-label}"
  select-menu:
    backgroundColor: "{colors.menu-fill}"
    textColor: "{colors.menu-ink}"
    rounded: "{rounded.menu}"
    padding: "6px"
  tooltip:
    backgroundColor: "{colors.help-fill}"
    textColor: "{colors.help-ink}"
    rounded: "{rounded.app}"
    padding: "11px 13px"
    typography: "{typography.tooltip}"
  button-primary:
    backgroundColor: "{colors.save}"
    textColor: "{colors.save-ink}"
    rounded: "{rounded.control}"
    padding: "9px 14px"
    typography: "{typography.label}"
  button-quiet:
    backgroundColor: "{colors.quiet}"
    textColor: "{colors.quiet-ink}"
    rounded: "{rounded.quiet}"
    padding: "9px 12px"
    typography: "{typography.label}"
  panel:
    backgroundColor: "{colors.surface}"
    rounded: "{rounded.panel}"
  button-stream-save:
    backgroundColor: "{colors.stream-save}"
    textColor: "{colors.stream-save-ink}"
    rounded: "{rounded.quiet}"
    padding: "8px 12px"
    typography: "{typography.label}"
  rail-button:
    rounded: "{rounded.nav}"
    size: "44px"
---

# Design System: Stellars Studio

## Overview

Stellars Studio is the adopted design system for the main console: rounded charcoal surfaces, soft violet interactions, teal monitor output and a prominent sky-blue Stream panel. It keeps five audio buses and their routing, meters, EQ and dynamics directly accessible during desktop operation.

The production surface is `src/App.svelte` with `MixerView`, `EqCanvas`, `StreamPanel`, `StudioIcon` and `src/studio.css`. The user's approved prototype remains at `/?mock` as a visual reference. The previous Nocturnal system is archived at `.impeccable/archive/pre-studio/DESIGN.md`; this root document governs new work.

**Key Characteristics:**

- Rounded tonal layers with subtle upper-edge light.
- Compact controls and tabular numeric readouts.
- Stable channel colors linking routing, telemetry and EQ.
- Restrained motion with reduced-motion support.

## Colors

### Primary

Violet is the general interaction accent. The light save fill suits committed actions; channel colors remain distinct from that general accent.

### Secondary

Teal marks focus and engine status; the monitor card uses a dark teal gradient. Sky blue identifies independent Stream output, including its icon, heading and meters. Stream status uses the lighter status blue and its Save action uses stream-save with stream-save-ink. Stream's background is `linear-gradient(115deg,#1d2530,#1c1e24 65%)`.

### Tertiary

Game, Chat, Media, Aux and Mic have stable identities across channel icons, meter dots, app assignments and selected EQ response. App badges inherit assigned bus identity. Unrouted apps use a neutral badge.

### Neutral

Canvas grounds the shell; surface supports panels; ink is primary text; muted supports explanatory text. White alpha borders provide quiet separation. Compatibility custom properties in `studio.css` map the retained component contracts onto this world; their legacy names do not retain the archived visual identity.

**The Channel Identity Rule.** Keep a bus color consistent across its meter, route label and EQ response.

## Typography

Use locally hosted Plus Jakarta Sans from `src/assets/fonts`, weights (400/500/600/700), with sans-serif fallback and the accompanying OFL license. Headline, section title, body, label and readout roles are normative in frontmatter. The wordmark is bold (26px). Readouts use tabular numerals throughout the live console.

The headline reduces from (29px) to (27px), (25px), then (23px) at narrower widths. Wide channel values reach (27px). Dense helper labels range roughly (8–11px); these observed sizes do not constitute an accessibility certification. Preserve readable labels and visible focus when extending controls.

## Layout

Desktop uses a sticky navigation rail (76px), centered workspace maximum (1760px), horizontal padding (35px), and header (88px). A flexible mixer/EQ column sits beside a routing/dynamics column (302px), with gap (22px). Five equal strips have gap (11px). Stream spans the grid between playback/routing and EQ/dynamics.

At minimum width (1600px), workspace padding grows to (45px), the side column to (340px), and the gap to (26px). At maximum width (1250px), the side column narrows to (265px). At (1050px), panels become one column. At (720px), the rail becomes (54px), padding (16px), strips form three columns, monitor and ChatMix stack, and routing uses a single app list. At (410px), padding becomes (12px), strips form two columns and Mic spans the row.

Production app lists scroll within their panel; at smaller layouts their height is capped (340px). Stream summary changes from four columns to two at (1250px), then one at (720px); settings also become one column. EQ's ten tabs remain visible; its inspector shifts from five columns to three at (1200px), then two at (720px). Its canvas is (210px) high, reducing to (180px) on mobile.

## Elevation & Depth

Tonal gradients provide most depth. Channels use `0 7px 16px #00000012`; selection uses `inset 0 1px 0 #d0b5ff3d,0 7px 16px #0002`. Fader tracks are recessed (`inset 1px 1px 3px #0008`) and metallic thumbs cast a small shadow (`0 3px 5px #0008`). Toasts float above content (`0 8px 28px #0005`).

Channel entrance uses duration (420ms), displacement (8px) and stagger (45ms). App hover lifts (2px over 180ms). Drawer entrance takes (250ms); ordinary state changes take (160–250ms). Standard easing is `cubic-bezier(.22,1,.36,1)`. Stream meter widths transition over (80ms linear). Reduced motion disables CSS animation/transitions and smooth scrolling. Pause preview affects browser simulation; it is not an audio-engine pause control.

## Shapes

Channels use (18px) corners, standard panels (16px), rail buttons (14px), and compact controls (6–10px). Knobs and switches use circular or pill forms. Studio icons are inline SVG with a (24×24) viewBox, rounded caps/joins and (1.65px) stroke; Stream uses its own inline display icon. No raster imagery is required by the console.

## Components

### Buttons and chips

Quiet actions use neutral fills that brighten on hover. Solo/mute use the bus accent when engaged. Source chips show checked state and violet fill; filters use neutral selected fill. Most controls use teal focus outlines (2px with 4px offset); EQ scoped controls use offset (3px). Disabled buttons fade to 40% opacity. Stream's Save stream mix button uses the sky-blue action fill, rounded corners (8px), padding (8px 12px) and a lighter hover fill (`#c8e8ff`).

### Inputs and routing

Search is a dark inset with compact text. Shared custom dropdowns provide keyboard and single-pointer routing alongside drag/drop. The process pool supports name, executable and PID search, unrouted filtering, executable rules removal and refresh. Dropping into the pool restores system playback. Faders range from zero to 150%, display logarithmic dB, support wheel trim and double-click reset to unity. Pre-gain, ChatMix and master retain fine adjustment/reset behavior. Keep the +3, unity and negative-infinity detents aligned to fader geometry.

### Meters and engine state

Twelve-by-eighteen dot matrices map live left/right RMS and peak values, with held-peak highlights; adjacent bars show stereo peaks. Monitor and Stream also have stereo meters. Native mode consumes existing engine telemetry. Browser fallback is explicitly labeled and simulates data with a preview pause control. Engine status, microphone state and underruns remain visible. Never imply browser fallback is connected hardware.

### EQ and dynamics

The production canvas retains shared RBJ magnitude-response calculations, logarithmic frequency placement, summed and individual response visualization, draggable band nodes, wheel Q adjustment, double-click gain reset, presets and an enabled/type/frequency/gain/Q inspector. Its computed filter response is not a measured acoustic response. Gate and compressor controls edit the selected bus through existing callbacks; gate threshold/ratio/attack/release and compressor threshold/ratio/attack/release/makeup/knee remain accessible. This overhaul introduces no DSP backend.

### Independent Stream output

Stream has its own level, mute, stereo telemetry and status, selected-app/microphone summary and override count. Settings stage category selections, executable Include/Exclude/Follow category overrides, enabled state and a dedicated output. Search includes remembered overrides for apps that are not running. Save commits through the existing configuration callback; Cancel discards the draft. Refresh updates endpoints. Reserved monitor, silent sink and virtual mic outputs are excluded; unavailable destinations and save errors remain explicit.

App captures feed Stream before playback faders, category DSP, mute/solo and ChatMix; microphone uses its processed signal. Native persistence, tray behavior and existing Tauri command/meter contracts remain authoritative. Browser mode is an interactive fallback; `?mock` is the independent in-memory reference prototype.

### Navigation and hardware

Rail links scroll to Mixer, routing, EQ and Stream; Mixer retains a static current marker. Hardware buttons open an inline drawer for monitor, silent routing sink, physical microphone and virtual microphone output. The footer reports desktop connection or browser preview.

### Shared selection, editing and help

All visible dropdowns use `StudioSelect`, including the reference mock. The native select remains hidden as the value/change adapter; a custom combobox trigger opens a body-portal listbox. Menus stay within viewport margins (8px), flip above when needed, use width (190–480px) bounded by viewport, and maximum height (320px). Arrow keys move among enabled options; Home/End jump; Enter/Space select; Escape/Tab dismiss; typeahead resets after (600ms). Outside pointer, focus departure and resize close the menu. Scrolling repositions it or closes it when the trigger leaves view.

`EditableValue` opens numeric editing on double-click, Enter or F2. Enter commits; Escape cancels. Invalid, empty, non-finite or out-of-field-bound values are rejected. Readouts (including custom display strings), edit seeds and committed entries use at most two decimal places; percentages round to whole numbers. Typed entries are not quantized to slider step. ArrowUp/Down use the field step with a minimum nudge of 0.01, or 1 for percentages. Internal values remain unchanged until edited. Blur commits a valid draft and cancels an invalid one. Eligible dB level fields accept `-inf`, `-infinity`, `−∞` or `-∞` for silence. A bounded portal hint shows each field's range and inline validation; keep bounds specific to the underlying field.

The console disables selection for interface chrome. Text fields, textareas, contenteditable areas, `[data-selectable]`, errors, preformatted text and code remain selectable; editable/copyable areas retain native context menus. Range controls and ordinary chrome keep the application context-menu behavior.

Tooltips use the shared portal layer on pointer hover and keyboard focus, connect through `aria-describedby`, dismiss on Escape/interaction/scroll/resize and avoid competing with an open dropdown or active numeric editor. CafeTako `interaction-tooltip` informed their maximum width (240px), offset (8px), delay (80ms), entrance (150ms ease-out), soft shadow and (8px) arrow; Studio colors and Plus Jakarta Sans remain authoritative. Reduced motion removes these entrance animations and control transitions.

**The Precise Entry Rule.** Numeric readouts and edited values use at most two decimal places; percentages round to whole numbers. Field bounds remain authoritative, and slider step does not quantize typed entries.

## Do's and Don'ts

- **Do** preserve channel identity across routing, meters and EQ.
- **Do** keep keyboard alternatives and visible focus alongside pointer interactions.
- **Do** retain responsive wrapping and reduced-motion behavior.
- **Do** distinguish connected telemetry from browser preview data.
- **Don't** replace production callbacks or Tauri contracts with prototype state.
- **Don't** label calculated EQ response as measured audio.
- **Don't** embed inspiration or review screenshots in the console UI.
