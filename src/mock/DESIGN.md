---
name: Stellars Studio Preview
description: A rounded charcoal audio console with violet controls, teal output and dot-matrix activity.
colors:
  canvas: "#111113"
  surface: "#1c1c20"
  ink: "#f4f3f7"
  muted: "#a2a1ac"
  violet: "#b8a1fa"
  teal: "#87ddcf"
  stream-blue: "#9bd6ff"
  stream-title: "#b6dfff"
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
    textColor: "{colors.save-ink}"
    rounded: "{rounded.control}"
    padding: "9px 14px"
    typography: "{typography.label}"
  rail-button:
    rounded: "{rounded.nav}"
    size: "44px"
---
# Design System: Stellars Studio Preview

## Overview

This document describes only the isolated `?mock` prototype in `src/mock`. The live console retains the root design system until a separate integration decision.
The approved direction pairs rounded charcoal surfaces with violet and teal, dense audio controls, dot-matrix activity and restrained animation. The user-supplied black healthcare dashboard informed the visual composition; the shipped screen expresses Stellars' five audio buses and independent Stream mix. This is an interactive design preview with simulated data.
**Key Characteristics:**
- Rounded, tonally separated surfaces with subtle light at their upper edges.
- Quiet typography, tabular audio readouts and compact labels.
- Persistent channel colors connecting routing, selection and EQ.
- Small entrance and hover movements, with reduced-motion support.

## Colors

### Primary

Violet identifies the general interaction accent. The lighter save color provides the committed action in Stream settings; channel-specific colors remain separate from this general accent.

### Secondary

Teal marks status and keyboard focus. The monitor card uses a darker teal gradient to distinguish listening output from channel editing. Sky blue identifies Stream: stream-blue colors its icon, meter bars and enabled switch; stream-title colors its heading; stream-save colors its Save action. Its broad blue-charcoal gradient makes the independent output easy to find.

### Tertiary

Game, Chat, Media, Aux and Mic each have a stable color. Those colors connect channel icons, meter dots, route labels, active controls and the selected EQ curve. App initials have their own muted badge colors and do not redefine bus identity.

### Neutral

Canvas is the page foundation; surface is the panel layer; ink is primary text; muted is supporting text. White alpha borders divide only where useful. Quiet buttons use their own raised neutral fill.
**The Channel Identity Rule.** Keep a bus color consistent across its meter, route label and EQ preview.

## Typography

Use locally bundled Plus Jakarta Sans with a sans-serif fallback. Available weights are 400, 500, 600 and 700. The headline is medium weight with tight tracking; section titles are semibold; channel and master readouts are regular weight with tabular numerals.
The frontmatter records desktop roles. Supporting prose is typically (12px); section titles (15px); controls and helper labels (8–11px). The wordmark is bold (26px). Small labels are part of the shipped dense prototype, not an established accessibility certification. At narrow widths the headline steps down to (27px), (25px) and (23px). Wide channel readouts increase to (27px).

## Layout

The desktop shell reserves a sticky (76px) navigation rail. The workspace is centered with maximum width (1760px), horizontal padding (35px), and header height (88px). Its main grid pairs a flexible mixer/EQ column with a (302px) routing/dynamics column and a (22px) gap. Five channel strips use equal columns with an (11px) gap. Stream spans the grid between playback/routing and DSP sections.
At (1600px) and wider, workspace padding becomes (45px), the right column (340px), and grid gap (26px). At (1250px) and below the right column contracts to (265px). At (1050px) and below the console becomes one column and routing uses two app columns. At (720px) and below the rail narrows to (54px), workspace padding becomes (16px), strips form three columns, app rows form one column, ChatMix and monitor stack, and Stream controls wrap. At (410px) and below strips form two columns with Mic spanning the row and workspace padding (12px).
The EQ retains ten equal band columns. Stream settings change from a two-column grid to one column on mobile. Follow the implemented responsive overrides rather than treating desktop density as fixed.

## Elevation & Depth

Tonal gradients carry most depth. Channel strips have a faint ambient shadow (`0 7px 16px #00000012`); the selected channel combines a violet upper glow with a fine inset highlight and a darker shadow. The monitor card uses an inset highlight. The fader track is recessed (`inset 1px 1px 3px #0008`) and the metallic thumb casts a small shadow (`0 3px 5px #0008`). Toasts float above content (`0 8px 28px #0005`).
Motion is brief: channel entrance (420ms), displacement (8px), stagger (45ms); app-row hover lift (2px over 180ms); drawers (250ms) and ordinary state changes (160–250ms). The easing curve is `cubic-bezier(.22,1,.36,1)`. Reduced motion removes CSS transitions and animations and turns off smooth scrolling. Simulated activity updates on a (110ms) timer, pauses when the page is hidden, and has an explicit Pause meters control.

## Shapes

Channels have the broadest card corners; standard panels are slightly tighter. Compact controls use small rounded corners; switches and slider knobs are circular or pill-shaped. Inline icons use a (24×24) viewBox, rounded caps and joins, and (1.65px) stroke. The star mark and all icons are inline SVG. Meter dots are small squares with subtly softened corners, arranged as twelve columns by eighteen rows.

## Components

### Buttons

Quiet buttons are neutral and brighten on hover. Save settings uses the light violet primary treatment, overridden with sky blue inside Stream. Icon buttons have a (30×30px) target and an alpha hover fill. Solo and mute adopt the channel color when engaged. Keyboard focus uses a teal outline (2px) with offset (4px); disabled buttons fade to 40% opacity. Native buttons and `aria-pressed` carry toggle state.

### Chips

Stream source chips show an optional check and violet fill when included. App filters use a subtle neutral selected fill. Selection is expressed in text/state as well as color.

### Cards / Containers

Panels share the surface fill and panel radius. Channels use a slight gradient; selection changes their tint and edge highlight. A dashed, channel-colored outline marks a drag target. Keep large fills quiet enough for controls and telemetry to carry hierarchy.

### Inputs / Fields

Search sits in a translucent dark inset with an inline search icon. Shared custom dropdowns use dark violet-neutral fills; route triggers remain visually quiet. Range inputs remain native controls with custom tracks and thumbs. Channel faders range from 0 to 150%, display logarithmic dB, support wheel trim and double-click reset to unity. Their visible ticks are +3, 0 and negative infinity, aligned to the actual range geometry.

### Navigation

The rail links to page sections. Mixer has the persistent filled current appearance; section scrolling does not update that appearance. Hardware controls open an inline drawer rather than changing the page. The footer links back to the current console.

### Dot-matrix meters and EQ

Dot matrices, small stereo bars and active channel colors provide the signature audio texture. Values are simulated; channel mute/solo affects the simulated activity. The EQ uses shared `dspMath` presets and sums RBJ biquad magnitude responses on a logarithmic frequency axis. Nodes support pointer dragging for frequency and gain; native gain sliders and a band inspector provide alternatives. The inspector edits frequency, Q, filter type and enabled state. The calculated response is a preview, not measured audio. Pre-gain is available on every channel. Collapsible advanced dynamics expose gate ratio/attack/release and compressor threshold/attack/release/makeup/knee alongside the primary gate threshold and compressor ratio.

### Independent Stream settings

The blue Stream panel has independent stereo activity, percentage/dB level, mute, enabled status, source count and override count. Its drawer stages source chips, enabled state, searchable per-app Always include/Always exclude/Follow category overrides with effective inclusion labels, and a simulated recording destination. Save commits the draft; Cancel discards it. Device refresh simulates a busy state. Destination choices exclude the monitor, silent routing sink and virtual mic endpoints; saving enabled Stream requires an available dedicated destination. These actions change local preview state only.

Routing includes removable executable rules and Reset routes. Hardware I/O exposes monitor, microphone, silent sink and virtual mic selections. ChatMix uses the shared gain law for the simulated Game/Chat meters. No controls invoke the engine bridge.

### Shared selection, editing and help

All visible dropdowns use `StudioSelect`, including the reference mock. The native select remains hidden as the value/change adapter; a custom combobox trigger opens a body-portal listbox. Menus stay within viewport margins (8px), flip above when needed, use width (190–480px) bounded by viewport, and maximum height (320px). Arrow keys move among enabled options; Home/End jump; Enter/Space select; Escape/Tab dismiss; typeahead resets after (600ms). Outside pointer, focus departure and resize close the menu. Scrolling repositions it or closes it when the trigger leaves view.

`EditableValue` opens numeric editing on double-click, Enter or F2. Enter commits; Escape cancels. Invalid, empty, non-finite or out-of-field-bound values are rejected. Readouts (including custom display strings), edit seeds and committed entries use at most two decimal places; percentages round to whole numbers. Typed entries are not quantized to slider step. ArrowUp/Down use the field step with a minimum nudge of 0.01, or 1 for percentages. Internal values remain unchanged until edited. Blur commits a valid draft and cancels an invalid one. Eligible dB level fields accept `-inf`, `-infinity`, `−∞` or `-∞` for silence. A bounded portal hint shows each field's range and inline validation; keep bounds specific to the underlying field.

The console disables selection for interface chrome. Text fields, textareas, contenteditable areas, `[data-selectable]`, errors, preformatted text and code remain selectable; editable/copyable areas retain native context menus. Range controls and ordinary chrome keep the application context-menu behavior.

Tooltips use the shared portal layer on pointer hover and keyboard focus, connect through `aria-describedby`, dismiss on Escape/interaction/scroll/resize and avoid competing with an open dropdown or active numeric editor. CafeTako `interaction-tooltip` informed their maximum width (240px), offset (8px), delay (80ms), entrance (150ms ease-out), soft shadow and (8px) arrow; Studio colors and Plus Jakarta Sans remain authoritative. Reduced motion removes these entrance animations and control transitions.

**The Precise Entry Rule.** Numeric readouts and edited values use at most two decimal places; percentages round to whole numbers. Field bounds remain authoritative, and slider step does not quantize typed entries.

## Do's and Don'ts

- **Do** preserve channel identity across routing, meters and EQ.
- **Do** use shared keyboard controls and visible focus alongside drag routing.
- **Do** retain responsive wrapping, reduced-motion behavior and explicit meter pause.
- **Do** label illustrative activity and device names as demo data.
- **Don't** present the preview curve or meter values as measured audio.
- **Don't** apply this prototype's tokens to the live console without a separate integration decision.
- **Don't** embed the inspiration screenshot or review screenshots in the product UI.
