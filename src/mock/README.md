# Stellars Studio preview

An isolated, clickable console design preview. From the repository root run:

```sh
npm run dev
```

Open the local Vite URL with `/?mock`. The query parameter selects a lazy import of `MockConsole.svelte` from `src/main.ts`; without it the current console loads. The preview does not invoke the engine bridge. Build all frontend assets with `npm run build`.

The preview includes five channel faders and pre-gain, mute/solo, searchable drag/select app routing, removable executable rules and Reset routes, ChatMix, monitor level, and hardware microphone/monitor/silent sink/virtual mic selections. EQ uses shared `dspMath` presets and RBJ summed magnitude responses, draggable frequency/gain nodes, gain sliders and a frequency/Q/type/enabled inspector. Dynamics include gate and compressor controls with advanced timing, threshold, ratio, makeup and knee settings.

The prominent blue Stream panel sits between playback and DSP, with independent simulated stereo meters, level/mute/enable, source and override counts. Its settings stage enabled state, source selections, searchable app overrides with inclusion labels, and a dedicated output. Save/Cancel commits/discards drafts; refresh simulates discovery; validation excludes reserved or unavailable endpoints. All state is in memory and resets on reload.

Meters and device/app data are simulated. The EQ response is calculated from shared biquad math, not measured audio. Dynamics and output controls do not process audio. The unavailable-device state is illustrative. Reduced motion disables CSS animation and transitions; the Pause meters button controls simulated activity, and updates pause while the document is hidden. Rail links scroll to sections; the Mixer current marker is static.

The scoped design reference is [DESIGN.md](./DESIGN.md); [design.json](./design.json) is its schemaVersion 2 extension sidecar. The main console now adopts this Studio visual direction; root design documents govern the integrated app, while this route remains an independent reference prototype.

CafeTako references were inspected and rendered before porting the visual patterns to Svelte: `sys-nav-rail` for the shell, `sys-stat-tiles` for summary styling, `stagger-children` with duration 420ms/distance 8px/stagger 45ms, and `hover-lift` adapted to 2px. The preview has no builder backend dependency. Suivelle project context informed product scope.

Plus Jakarta Sans is bundled locally in `fonts/` at weights 400/500/600/700. The font files came from `fonts.gstatic.com/s/plusjakartasans/v12` through Google Fonts CSS; the OFL license is [fonts/OFL.txt](./fonts/OFL.txt). Icons and the star mark are inline SVG. No raster assets ship in this UI. The user-supplied black healthcare dashboard image was visual inspiration only and is not embedded.

Browser review evidence lives at repository-relative `.impeccable/review/mock-*.png`, including `mock-expanded-desktop.png` and `mock-expanded-mobile.png`. Check results are in `.impeccable/review/mock-checks.json` (16 base checks) and `.impeccable/review/mock-parity-checks.json` (18 parity checks). These files document the reviewed preview; they are not runtime assets or proof of production audio integration. Review corrections include fader tick alignment, narrow Stream wrapping and mobile ChatMix verification.

Shared controls also apply to this route: custom portal dropdowns with keyboard/typeahead, precise numeric entry (double-click/Enter/F2; at most two decimals and whole-number percentages), bounded range hints and contextual tooltips. Chrome is nonselectable; editable/copyable areas retain selection and native context menus. These reuse the main app components and `src/controls.css`.
