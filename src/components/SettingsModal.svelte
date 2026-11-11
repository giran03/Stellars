<script module lang="ts">
  export type SettingsTabId =
    | "general"
    | "audio"
    | "presets"
    | "mixing"
    | "appearance"
    | "shortcuts"
    | "diagnostics"
    | "updates"
    | "advanced"
    | "about";
</script>

<script lang="ts">
  import { onMount, tick, type Snippet } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import StudioIcon from "./StudioIcon.svelte";
  import ToggleRow from "./ToggleRow.svelte";
  import DiagnosticsPanel from "./DiagnosticsPanel.svelte";
  import ChangelogView from "./ChangelogView.svelte";
  import BackupsPanel from "./BackupsPanel.svelte";
  import PresetsPanel from "./PresetsPanel.svelte";
  import MixingPanel from "./MixingPanel.svelte";
  import type { DuckingConfig, RouterConfig } from "../types";
  import {
    appSettings,
    resetAppSettings,
    updateAppSettings,
    type AppSettings,
  } from "../appSettings.svelte";

  type Theme = "dark" | "light";

  interface Props {
    open: boolean;
    tab: SettingsTabId;
    native: boolean;
    theme: Theme;
    audio: Snippet;
    onSetTheme: (theme: Theme) => void;
    onClose: () => void;
    onNotify: (msg: string) => void;
    /** Reload router state after a backup was restored. */
    onRestored: () => void;
    onRunWizard: () => void;
    ducking: DuckingConfig;
    duckGain: number;
    onChangeDucking: (next: DuckingConfig) => void;
    /** A scene or output profile was applied; `config` is the engine's resulting config. */
    onPresetApplied: (config: RouterConfig) => void;
  }

  let {
    open,
    tab = $bindable(),
    native,
    theme,
    audio,
    onSetTheme,
    onClose,
    onNotify,
    onRestored,
    onRunWizard,
    ducking,
    duckGain,
    onChangeDucking,
    onPresetApplied,
  }: Props = $props();

  const DEVELOPER = "CafeTako";
  const SUPPORT_EMAIL = "support@cafetako.com";
  const DONATION_URL = "https://buymeacoffee.com/cafetako";
  /** Updates are read from the latest GitHub release of this repository. */
  const RELEASES_REPO = "giran03/Stellars";
  const LATEST_RELEASE_URL = `https://api.github.com/repos/${RELEASES_REPO}/releases/latest`;
  const RELEASE_PAGE_PREFIX = `https://github.com/${RELEASES_REPO}/`;
  /** Version from package.json (injected at build time); the desktop app replaces it with the native one. */
  const FALLBACK_VERSION = __APP_VERSION__;

  const TABS: { id: SettingsTabId; label: string; icon: string; hint: string }[] = [
    { id: "general", label: "General", icon: "settings", hint: "Startup, window and notification behavior" },
    { id: "audio", label: "Audio", icon: "headphones", hint: "Devices and virtual cables" },
    { id: "presets", label: "Presets", icon: "mix", hint: "Scenes, auto-switching and output profiles" },
    { id: "mixing", label: "Mixing", icon: "volume", hint: "Auto-ducking between channels" },
    { id: "appearance", label: "Appearance", icon: "sun", hint: "Theme, tooltips and motion" },
    { id: "shortcuts", label: "Shortcuts", icon: "keyboard", hint: "Keyboard reference" },
    { id: "diagnostics", label: "Diagnostics", icon: "wave", hint: "Engine health, devices and logs" },
    { id: "updates", label: "Updates", icon: "download", hint: "Version and update checks" },
    { id: "advanced", label: "Advanced", icon: "wrench", hint: "Backups, setup, data folder and quit" },
    { id: "about", label: "About", icon: "info", hint: "Developer, support and donations" },
  ];

  const SHORTCUTS: { keys: string[]; action: string }[] = [
    { keys: ["1", "–", "5"], action: "Select Game, Chat, Media, Aux or Mic channel" },
    { keys: ["M"], action: "Mute / unmute the selected channel" },
    { keys: ["S"], action: "Solo the selected channel" },
    { keys: ["I"], action: "Open audio settings" },
    { keys: [","], action: "Open settings" },
    { keys: ["Alt", "T"], action: "Toggle dark / light theme" },
    { keys: ["Esc"], action: "Close this dialog or any open menu" },
  ];

  let dialogEl = $state<HTMLDivElement>();
  let appVersion = $state(FALLBACK_VERSION);
  let showChangelog = $state(false);
  let dataDir = $state<string | null>(null);
  let saveError = $state<string | null>(null);
  let armed = $state<"reset" | "quit" | null>(null);
  let armTimer: ReturnType<typeof setTimeout> | undefined;

  type UpdateState =
    | { kind: "idle" }
    | { kind: "checking" }
    | { kind: "current" }
    | { kind: "available"; version: string; url?: string; notes?: string }
    | { kind: "error"; message: string };
  let update = $state<UpdateState>({ kind: "idle" });

  // Diagnostics is a support tool; Simple mode keeps everyone else's tabs.
  const visibleTabs = $derived(appSettings.interfaceMode === "simple" ? TABS.filter((t) => t.id !== "diagnostics") : TABS);
  const activeTab = $derived(visibleTabs.find((t) => t.id === tab) ?? visibleTabs[0]);

  $effect(() => {
    if (!visibleTabs.some((t) => t.id === tab)) tab = "general";
  });

  // The change log is a sub-view of Updates; leaving the tab returns to the normal view.
  $effect(() => {
    if (tab !== "updates") showChangelog = false;
  });

  async function save(patch: Partial<AppSettings>) {
    saveError = null;
    try {
      await updateAppSettings(patch);
    } catch (err) {
      saveError = `Could not save that setting: ${String(err)}`;
    }
  }

  async function openLink(url: string) {
    if (!url) return;
    if (native) {
      try {
        await invoke("open_external", { url });
      } catch (err) {
        onNotify(`Could not open link: ${String(err)}`);
      }
      return;
    }
    if (url.startsWith("mailto:")) window.location.href = url;
    else window.open(url, "_blank", "noopener,noreferrer");
  }

  async function copyEmail() {
    try {
      await navigator.clipboard.writeText(SUPPORT_EMAIL);
      onNotify("Support email copied");
    } catch {
      onNotify(SUPPORT_EMAIL);
    }
  }

  function arm(kind: "reset" | "quit") {
    armed = kind;
    clearTimeout(armTimer);
    armTimer = setTimeout(() => (armed = null), 4000);
  }

  async function confirmReset() {
    if (armed !== "reset") return arm("reset");
    armed = null;
    saveError = null;
    try {
      await resetAppSettings();
      onNotify("App settings restored to defaults");
    } catch (err) {
      saveError = `Could not reset settings: ${String(err)}`;
    }
  }

  async function confirmQuit() {
    if (armed !== "quit") return arm("quit");
    armed = null;
    try {
      await invoke("quit_application");
    } catch (err) {
      saveError = `Could not quit: ${String(err)}`;
    }
  }

  async function openDataFolder() {
    try {
      await invoke("open_data_folder");
    } catch (err) {
      saveError = `Could not open the data folder: ${String(err)}`;
    }
  }

  /** Compares dotted numeric versions; true when `next` is newer than `current`. */
  function isNewer(next: string, current: string): boolean {
    const parse = (v: string) => v.replace(/^v/i, "").split(/[.+-]/).map((n) => parseInt(n, 10) || 0);
    const a = parse(next);
    const b = parse(current);
    for (let i = 0; i < Math.max(a.length, b.length); i++) {
      const diff = (a[i] ?? 0) - (b[i] ?? 0);
      if (diff !== 0) return diff > 0;
    }
    return false;
  }

  type GithubRelease = { tag_name?: string; html_url?: string; body?: string };

  /** The repository's latest published release, or null when none exists. */
  async function fetchLatestRelease(): Promise<GithubRelease | null> {
    const res = await fetch(LATEST_RELEASE_URL, {
      cache: "no-store",
      headers: { Accept: "application/vnd.github+json" },
    });
    if (res.status === 404) return null;
    if (!res.ok) throw new Error(`GitHub answered ${res.status}`);
    return ((await res.json()) as GithubRelease) ?? null;
  }

  /** Plain, short version of the release notes for the status line. */
  function summarizeNotes(body: string | undefined): string | undefined {
    const text = (body ?? "").replace(/<[^>]*>/g, " ").replace(/[#*_`>\[\]]/g, "").replace(/\s+/g, " ").trim();
    if (!text) return undefined;
    return text.length > 220 ? `${text.slice(0, 217)}…` : text;
  }

  async function checkForUpdates() {
    if (update.kind === "checking") return;
    update = { kind: "checking" };
    try {
      const release = await fetchLatestRelease();
      if (!release?.tag_name) {
        update = { kind: "error", message: "No published release was found." };
        return;
      }
      const url = release.html_url?.startsWith(RELEASE_PAGE_PREFIX) ? release.html_url : `${RELEASE_PAGE_PREFIX}releases/latest`;
      update = isNewer(release.tag_name, appVersion)
        ? { kind: "available", version: release.tag_name.replace(/^v/i, ""), url, notes: summarizeNotes(release.body) }
        : { kind: "current" };
      await save({ lastUpdateCheck: new Date().toISOString() });
    } catch (err) {
      update = { kind: "error", message: err instanceof Error ? err.message : String(err) };
    }
  }

  function formatChecked(iso: string | null): string {
    if (!iso) return "Never";
    const d = new Date(iso);
    return Number.isNaN(d.getTime()) ? "Never" : d.toLocaleString();
  }

  function focusables(): HTMLElement[] {
    if (!dialogEl) return [];
    return [
      ...dialogEl.querySelectorAll<HTMLElement>(
        'button:not([disabled]), [href], input:not([disabled]), select:not([disabled]), [tabindex]:not([tabindex="-1"])'
      ),
    ].filter((el) => el.offsetParent !== null);
  }

  function handleKeydown(e: KeyboardEvent) {
    if (!open) return;
    if (e.key === "Escape") {
      // Let an open dropdown close itself first.
      if (document.querySelector(".studio-select-menu")) return;
      e.preventDefault();
      e.stopPropagation();
      onClose();
      return;
    }
    if (e.key === "Tab") {
      const items = focusables();
      if (items.length === 0) return;
      const first = items[0];
      const last = items[items.length - 1];
      const current = document.activeElement;
      if (e.shiftKey && (current === first || !dialogEl?.contains(current))) {
        e.preventDefault();
        last.focus();
      } else if (!e.shiftKey && current === last) {
        e.preventDefault();
        first.focus();
      }
    }
  }

  function handleTabListKey(e: KeyboardEvent) {
    const step = e.key === "ArrowDown" || e.key === "ArrowRight" ? 1 : e.key === "ArrowUp" || e.key === "ArrowLeft" ? -1 : 0;
    if (!step) return;
    e.preventDefault();
    const idx = visibleTabs.findIndex((t) => t.id === tab);
    const next = visibleTabs[(idx + step + visibleTabs.length) % visibleTabs.length];
    tab = next.id;
    void tick().then(() => dialogEl?.querySelector<HTMLElement>(`#settings-tab-${next.id}`)?.focus());
  }

  function handleBackdrop(e: MouseEvent) {
    if (e.target === e.currentTarget) onClose();
  }

  $effect(() => {
    if (!open) return;
    const previouslyFocused = document.activeElement as HTMLElement | null;
    document.body.classList.add("has-active-modal");
    void tick().then(() => dialogEl?.querySelector<HTMLElement>(`#settings-tab-${tab}`)?.focus());
    return () => {
      document.body.classList.remove("has-active-modal");
      clearTimeout(armTimer);
      armed = null;
      previouslyFocused?.focus?.();
    };
  });

  onMount(() => {
    window.addEventListener("keydown", handleKeydown, true);
    void (async () => {
      if (!native) return;
      try {
        const info = await invoke<{ version: string; dataDir: string }>("get_app_info");
        appVersion = info.version;
        dataDir = info.dataDir;
      } catch {
        /* keep fallback version */
      }
    })();
    return () => window.removeEventListener("keydown", handleKeydown, true);
  });
</script>

{#snippet toggle(key: keyof AppSettings, title: string, desc: string, desktopOnly: boolean = false)}
  {@const disabled = desktopOnly && !native}
  <ToggleRow
    {title}
    {desc}
    checked={appSettings[key] === true}
    {disabled}
    note={disabled ? "Desktop app only" : undefined}
    onchange={(next) => save({ [key]: next } as Partial<AppSettings>)}
  />
{/snippet}

{#snippet segmented(label: string, desc: string, options: { value: string; label: string }[], current: string, onPick: (v: string) => void, desktopOnly: boolean = false)}
  {@const disabled = desktopOnly && !native}
  <div class="setting-row" class:is-disabled={disabled}>
    <div class="setting-copy">
      <strong>{label}</strong>
      <small>{desc}{#if disabled} · Desktop app only{/if}</small>
    </div>
    <div class="seg" role="radiogroup" aria-label={label}>
      {#each options as opt}
        <button
          type="button"
          role="radio"
          aria-checked={current === opt.value}
          class:on={current === opt.value}
          {disabled}
          onclick={() => onPick(opt.value)}
        >{opt.label}</button>
      {/each}
    </div>
  </div>
{/snippet}

{#if open}
  <div class="settings-backdrop" role="presentation" onclick={handleBackdrop}>
    <div
      bind:this={dialogEl}
      class="settings-modal"
      role="dialog"
      aria-modal="true"
      aria-labelledby="settings-title"
      tabindex="-1"
    >
      <aside class="settings-nav">
        <h2 id="settings-title">Settings</h2>
        <div class="settings-tablist" role="tablist" aria-orientation="vertical" tabindex="-1" onkeydown={handleTabListKey}>
          {#each visibleTabs as t}
            <button
              type="button"
              role="tab"
              id={`settings-tab-${t.id}`}
              aria-selected={tab === t.id}
              aria-controls="settings-panel"
              tabindex={tab === t.id ? 0 : -1}
              class="settings-tab"
              class:active={tab === t.id}
              onclick={() => (tab = t.id)}
            >
              <StudioIcon name={t.icon} size={16} />
              <span>{t.label}</span>
            </button>
          {/each}
        </div>
        <p class="settings-nav-foot">Stellars v{appVersion}</p>
      </aside>

      <section class="settings-content">
        <header class="settings-head">
          <div>
            <h3>{activeTab.label}</h3>
            <p>{activeTab.hint}</p>
          </div>
          <button type="button" class="icon-button settings-close" aria-label="Close settings" onclick={onClose}>
            <StudioIcon name="close" size={16} />
          </button>
        </header>

        <div
          class="settings-body"
          id="settings-panel"
          role="tabpanel"
          aria-labelledby={`settings-tab-${tab}`}
          tabindex="-1"
        >
          {#if saveError}
            <div class="settings-alert" role="alert">{saveError}</div>
          {/if}

          {#if tab === "general"}
            <div class="settings-group">
              <h4>Startup</h4>
              {@render toggle("launchOnStartup", "Launch when Windows starts", "Start Stellars automatically after you sign in so your routing is ready.", true)}
              {@render toggle("startMinimized", "Start minimized to tray", "Open silently in the system tray instead of showing the window.", true)}
            </div>
            <div class="settings-group">
              <h4>Window</h4>
              {@render segmented(
                "When the close button is pressed",
                "Keeping it in the tray leaves the audio engine and your virtual cables running.",
                [{ value: "tray", label: "Keep in tray" }, { value: "quit", label: "Quit Stellars" }],
                appSettings.closeBehavior,
                (v) => save({ closeBehavior: v as AppSettings["closeBehavior"] }),
                true
              )}
              {@render toggle("minimizeToTray", "Minimize to tray", "Hide the window from the taskbar when minimized. Click the tray icon to bring it back.", true)}
              {@render toggle("alwaysOnTop", "Keep window on top", "Float the mixer above other windows, handy while gaming.", true)}
            </div>
            <div class="settings-group">
              <h4>Devices</h4>
              {@render toggle("autoReconnectDevices", "Reconnect devices automatically", "When a headset, microphone or cable that Stellars uses is unplugged and returns, bind to it again.", true)}
            </div>
            <div class="settings-group">
              <h4>Performance</h4>
              {@render toggle("saveResourcesInBackground", "Save resources in the background", "Slow the live meters when the window is not focused and stop them while it is minimized or in the tray. Audio is never affected.", true)}
            </div>
            <div class="settings-group">
              <h4>Feedback</h4>
              {@render toggle("showNotifications", "Show notifications", "Brief confirmations such as “Aux muted” or “Faders linked”.")}
              {@render toggle("confirmBeforeReset", "Confirm before resetting routes", "Ask before clearing every saved app-to-channel rule.")}
            </div>

          {:else if tab === "audio"}
            {@render audio()}

          {:else if tab === "presets"}
            <PresetsPanel {native} onApplied={onPresetApplied} {onNotify} />

          {:else if tab === "mixing"}
            <MixingPanel {ducking} {duckGain} onChange={onChangeDucking} />

          {:else if tab === "appearance"}
            <div class="settings-group">
              <h4>Interface</h4>
              {@render segmented(
                "Interface mode",
                "Simple shows presets and a few friendly controls. Advanced shows every graph, slider and number. Your sound settings are never changed by switching.",
                [{ value: "simple", label: "Simple" }, { value: "advanced", label: "Advanced" }],
                appSettings.interfaceMode,
                (v) => save({ interfaceMode: v as AppSettings["interfaceMode"], interfaceModeChosen: true })
              )}
            </div>
            <div class="settings-group">
              <h4>Look &amp; feel</h4>
              {@render segmented(
                "Theme",
                "Switch the whole console between dark and light. Alt+T toggles it anywhere.",
                [{ value: "dark", label: "Dark" }, { value: "light", label: "Light" }],
                theme,
                (v) => onSetTheme(v as Theme)
              )}
              {@render toggle("showTooltips", "Show tooltips", "Explain controls when you hover or focus them.")}
              {@render toggle("reduceMotion", "Reduce motion", "Minimize animations and transitions across the app.")}
              {@render toggle("compactMode", "Compact layout", "Hide the page heading, status line and footer to fit the mixer on smaller screens.")}
            </div>

          {:else if tab === "shortcuts"}
            <div class="settings-group">
              <h4>Keyboard</h4>
              <ul class="shortcut-list">
                {#each SHORTCUTS as s}
                  <li>
                    <span>{s.action}</span>
                    <span class="keys">
                      {#each s.keys as k}
                        {#if k === "–"}<em>to</em>{:else}<kbd>{k}</kbd>{/if}
                      {/each}
                    </span>
                  </li>
                {/each}
              </ul>
              <p class="settings-note">Shortcuts are ignored while you are typing in a field.</p>
            </div>

          {:else if tab === "diagnostics"}
            <DiagnosticsPanel {native} active={open && tab === "diagnostics"} {onNotify} />

          {:else if tab === "updates" && showChangelog}
            <ChangelogView onBack={() => (showChangelog = false)} />

          {:else if tab === "updates"}
            <div class="settings-group">
              <h4>Version</h4>
              <div class="update-card">
                <div>
                  <strong>Stellars {appVersion}</strong>
                  <small>Last checked: {formatChecked(appSettings.lastUpdateCheck)}</small>
                </div>
                <div class="update-actions">
                  <button type="button" class="quiet-button" onclick={() => (showChangelog = true)}>
                    <StudioIcon name="info" size={14} /> Change log
                  </button>
                  <button type="button" class="cable-install-button" disabled={update.kind === "checking"} onclick={checkForUpdates}>
                    <StudioIcon name="reset" size={14} />
                    {update.kind === "checking" ? "Checking…" : "Check for updates"}
                  </button>
                </div>
              </div>
              {#if update.kind === "current"}
                <p class="update-status ok" role="status">You’re up to date.</p>
              {:else if update.kind === "available"}
                <div class="update-status ok" role="status">
                  <strong>Version {update.version} is available.</strong>
                  {#if update.notes}<span>{update.notes}</span>{/if}
                  {#if update.url}
                    <button type="button" class="quiet-button" onclick={() => openLink(update.kind === "available" ? (update.url ?? "") : "")}>
                      <StudioIcon name="external" size={13} /> Download
                    </button>
                  {/if}
                </div>
              {:else if update.kind === "error"}
                <p class="update-status bad" role="alert">Couldn’t check for updates: {update.message}</p>
              {/if}
            </div>
            <div class="settings-group">
              <h4>Preferences <span class="soon-badge">Coming soon</span></h4>
              {@render toggle("autoCheckUpdates", "Check for updates automatically", "Look for a new version each time Stellars starts.")}
            </div>

          {:else if tab === "advanced"}
            <div class="settings-group">
              <h4>Backup &amp; restore</h4>
              <BackupsPanel {native} {onNotify} {onRestored} />
            </div>
            <div class="settings-group">
              <h4>Setup</h4>
              <div class="setting-row">
                <div class="setting-copy">
                  <strong>Run the setup wizard again</strong>
                  <small>Walk through devices, virtual cables and startup preferences.</small>
                </div>
                <button type="button" class="quiet-button" onclick={onRunWizard}>
                  <StudioIcon name="play" size={13} /> Start wizard
                </button>
              </div>
            </div>
            <div class="settings-group">
              <h4>Data</h4>
              <div class="setting-row" class:is-disabled={!native}>
                <div class="setting-copy">
                  <strong>Configuration folder</strong>
                  <small>{dataDir ?? "Stores your routing, EQ and app settings"}{#if !native} · Desktop app only{/if}</small>
                </div>
                <button type="button" class="quiet-button" disabled={!native} onclick={openDataFolder}>
                  <StudioIcon name="folder" size={14} /> Open folder
                </button>
              </div>
              <div class="setting-row">
                <div class="setting-copy">
                  <strong>Reset app settings</strong>
                  <small>Restores the options in this window to their defaults. Mixer, EQ and routing are not touched.</small>
                </div>
                <button type="button" class="quiet-button" class:danger={armed === "reset"} onclick={confirmReset}>
                  <StudioIcon name="reset" size={14} />
                  {armed === "reset" ? "Click again to confirm" : "Reset settings"}
                </button>
              </div>
            </div>
            <div class="settings-group">
              <h4>Session</h4>
              <div class="setting-row" class:is-disabled={!native}>
                <div class="setting-copy">
                  <strong>Quit Stellars</strong>
                  <small>Stops the audio engine and removes the tray icon. Apps routed through Stellars return to normal playback.{#if !native} · Desktop app only{/if}</small>
                </div>
                <button type="button" class="quiet-button" class:danger={armed === "quit"} disabled={!native} onclick={confirmQuit}>
                  <StudioIcon name="power" size={14} />
                  {armed === "quit" ? "Click again to quit" : "Quit"}
                </button>
              </div>
            </div>

          {:else if tab === "about"}
            <div class="about-hero">
              <svg viewBox="0 0 256 256" width="56" height="56" fill="none" aria-hidden="true"><path fill-rule="evenodd" fill="#b8a1fa" d="M128 20C128 82 174 128 236 128C174 128 128 174 128 236C128 174 82 128 20 128C82 128 128 82 128 20ZM128 86L170 128 128 170 86 128Z"/><polygon points="128,108 148,128 128,148 108,128" fill="#edc990"/><path fill="#87ddcf" d="M174 62A96 96 0 0 1 194 82L185 91A84 84 0 0 0 165 71ZM188 48A116 116 0 0 1 208 68L199 77A104 104 0 0 0 179 57ZM82 194A96 96 0 0 1 62 174L71 165A84 84 0 0 0 91 185ZM68 208A116 116 0 0 1 48 188L57 179A104 104 0 0 0 77 199Z"/><circle cx="70" cy="70" r="7" fill="#9bd6ff"/><circle cx="186" cy="186" r="7" fill="#9bd6ff"/></svg>
              <div>
                <h4 class="about-name">Stellars</h4>
                <p>Multi-bus audio router &amp; DSP mixer · Version {appVersion}</p>
              </div>
            </div>
            <div class="settings-group">
              <h4>Developer</h4>
              <div class="setting-row">
                <div class="setting-copy"><strong>{DEVELOPER}</strong><small>Designed and built by CafeTako Team</small></div>
              </div>
              <div class="setting-row">
                <div class="setting-copy"><strong>Support</strong><small>{SUPPORT_EMAIL}</small></div>
                <div class="row-actions">
                  <button type="button" class="quiet-button" onclick={copyEmail}>Copy</button>
                  <button type="button" class="quiet-button" onclick={() => openLink(`mailto:${SUPPORT_EMAIL}`)}>
                    <StudioIcon name="mail" size={14} /> Email
                  </button>
                </div>
              </div>
            </div>
            <div class="settings-group">
              <h4>Support development</h4>
              <div class="donate-card">
                <div class="setting-copy">
                  <strong>Enjoying Stellars?</strong>
                  <small>It’s built and maintained independently. A coffee keeps the updates coming.</small>
                </div>
                <button type="button" class="donate-button" onclick={() => openLink(DONATION_URL)}>
                  <StudioIcon name="heart" size={14} /> Buy me a coffee
                </button>
              </div>
              <p class="settings-note">{DONATION_URL.replace("https://", "")}</p>
            </div>
            <p class="settings-note about-foot">© {new Date().getFullYear()} {DEVELOPER}. All rights reserved.</p>
          {/if}
        </div>
      </section>
    </div>
  </div>
{/if}

<style>
  .settings-backdrop {
    position: fixed;
    inset: 0;
    z-index: 9998;
    display: grid;
    place-items: center;
    padding: 24px;
    background: rgba(8, 7, 12, 0.7);
    backdrop-filter: blur(8px);
    -webkit-backdrop-filter: blur(8px);
    animation: fade 160ms ease-out;
  }
  :global(:root[data-theme="light"]) .settings-backdrop {
    background: rgba(30, 24, 45, 0.42);
  }
  :global(:root[data-motion="reduced"]) .settings-backdrop,
  :global(:root[data-motion="reduced"]) .settings-modal {
    animation: none;
  }

  .settings-modal {
    display: grid;
    grid-template-columns: 208px minmax(0, 1fr);
    width: min(940px, 100%);
    height: min(660px, 100%);
    overflow: hidden;
    border-radius: 18px;
    background: var(--panel);
    color: var(--text);
    border: 1px solid var(--line-strong);
    box-shadow: 0 28px 72px rgba(0, 0, 0, 0.55);
    animation: pop 200ms cubic-bezier(0.22, 1, 0.36, 1);
    outline: none;
  }
  @keyframes fade { from { opacity: 0; } to { opacity: 1; } }
  @keyframes pop { from { opacity: 0; transform: scale(0.97) translateY(6px); } to { opacity: 1; transform: none; } }

  /* Sidebar */
  .settings-nav {
    display: flex;
    flex-direction: column;
    gap: 14px;
    padding: 22px 12px 14px;
    background: var(--panel-inset);
    border-right: 1px solid var(--line);
    min-height: 0;
  }
  .settings-nav h2 {
    margin: 0 10px;
    font-size: 15px;
    font-weight: 600;
    letter-spacing: -0.025em;
  }
  .settings-tablist { display: flex; flex-direction: column; gap: 2px; flex: 1; overflow-y: auto; }
  .settings-tab {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 9px 10px;
    border: 0;
    border-radius: 9px;
    background: transparent;
    color: var(--text-muted);
    font: inherit;
    font-size: 12px;
    font-weight: 500;
    text-align: left;
    cursor: pointer;
    transition: background-color 140ms ease, color 140ms ease;
  }
  .settings-tab:hover { background: var(--control); color: var(--text); }
  .settings-tab.active { background: color-mix(in srgb, var(--violet) 18%, transparent); color: var(--text); }
  .settings-tab.active :global(svg) { color: var(--violet); }
  .settings-tab:focus-visible, .seg button:focus-visible, .settings-close:focus-visible, .donate-button:focus-visible {
    outline: 2px solid var(--violet);
    outline-offset: 2px;
  }
  .settings-nav-foot { margin: 0 10px; font-size: 10px; color: var(--text-muted); }

  /* Content */
  .settings-content { display: flex; flex-direction: column; min-width: 0; min-height: 0; }
  .settings-head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 16px;
    padding: 22px 26px 14px;
    border-bottom: 1px solid var(--line);
  }
  .settings-head h3 { margin: 0; font-size: 18px; font-weight: 600; letter-spacing: -0.03em; }
  .settings-head p { margin: 4px 0 0; font-size: 11px; color: var(--text-muted); }
  .settings-close { flex-shrink: 0; position: static; }
  .settings-body {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 18px 26px 26px;
    display: flex;
    flex-direction: column;
    gap: 22px;
    outline: none;
  }

  .settings-alert {
    padding: 10px 12px;
    border-radius: 9px;
    font-size: 11px;
    color: #ffc4d7;
    background: #f0a7c61a;
    border: 1px solid #f0a7c633;
  }
  :global(:root[data-theme="light"]) .settings-alert { color: #9c2f58; }

  .settings-group { display: flex; flex-direction: column; gap: 2px; }
  .settings-group h4 {
    margin: 0 0 6px;
    font-size: 10px;
    font-weight: 600;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--text-muted);
  }
  .soon-badge {
    margin-left: 8px;
    padding: 2px 7px;
    border-radius: 5px;
    font-size: 9px;
    letter-spacing: 0.04em;
    color: #edc990;
    background: #edc99018;
    text-transform: none;
  }
  .placeholder-note { margin: 0 0 8px; }
  .settings-note { margin: 6px 0 0; font-size: 10.5px; color: var(--text-muted); }

  .setting-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 20px;
    padding: 13px 14px;
    border-radius: 11px;
    background: var(--panel-raised);
    border: 1px solid var(--line);
    margin-bottom: 6px;
  }
  .setting-row.is-disabled { opacity: 0.55; }
  .setting-copy { display: flex; flex-direction: column; gap: 3px; min-width: 0; }
  .setting-copy strong { font-size: 12px; font-weight: 600; }
  .setting-copy small { font-size: 10.5px; line-height: 1.5; color: var(--text-muted); overflow-wrap: anywhere; }
  .row-actions { display: flex; gap: 8px; flex-shrink: 0; }


  /* Segmented control */
  .seg {
    display: inline-flex;
    flex-shrink: 0;
    padding: 3px;
    gap: 2px;
    border-radius: 10px;
    background: var(--panel-inset);
    border: 1px solid var(--line);
  }
  .seg button {
    padding: 6px 12px;
    border: 0;
    border-radius: 7px;
    background: transparent;
    color: var(--text-muted);
    font: inherit;
    font-size: 11px;
    font-weight: 500;
    cursor: pointer;
    white-space: nowrap;
    transition: background-color 140ms ease, color 140ms ease;
  }
  .seg button:hover:not(:disabled) { color: var(--text); }
  .seg button.on { background: var(--violet); color: var(--on-violet); font-weight: 600; }
  .seg button:disabled { cursor: not-allowed; }

  /* Shortcuts */
  .shortcut-list { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 6px; }
  .shortcut-list li {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    padding: 11px 14px;
    border-radius: 11px;
    background: var(--panel-raised);
    border: 1px solid var(--line);
    font-size: 12px;
  }
  .keys { display: inline-flex; align-items: center; gap: 5px; }
  .keys em { font-style: normal; font-size: 10px; color: var(--text-muted); }
  kbd {
    min-width: 24px;
    padding: 3px 8px;
    border-radius: 6px;
    background: var(--control);
    border: 1px solid var(--line-strong);
    border-bottom-width: 2px;
    font: inherit;
    font-size: 11px;
    font-weight: 600;
    text-align: center;
    color: var(--text);
  }

  /* Updates */
  .update-card, .donate-card {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 18px;
    padding: 16px;
    border-radius: 12px;
    background: var(--panel-raised);
    border: 1px solid var(--line);
  }
  .update-card > div:first-child { display: flex; flex-direction: column; gap: 4px; }
  .update-actions { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; justify-content: flex-end; }
  .update-card strong { font-size: 14px; letter-spacing: -0.02em; }
  .update-card small { font-size: 10.5px; color: var(--text-muted); }
  .update-status {
    margin: 8px 0 0;
    padding: 11px 14px;
    border-radius: 10px;
    font-size: 11.5px;
    line-height: 1.55;
    color: var(--text-secondary);
    background: var(--panel-inset);
    border: 1px solid var(--line);
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 8px;
  }
  .update-status.ok { color: var(--teal); border-color: color-mix(in srgb, var(--teal) 35%, transparent); }
  .update-status.bad { color: #ffc4d7; border-color: #f0a7c633; }
  :global(:root[data-theme="light"]) .update-status.bad { color: #9c2f58; }

  .setting-row :global(.quiet-button.danger) { background: #f08c7f26; color: #ffb4aa; }
  :global(:root[data-theme="light"]) .setting-row :global(.quiet-button.danger) { color: #a3342a; }

  /* About */
  .about-hero { display: flex; align-items: center; gap: 16px; padding: 4px 2px; }
  .about-name { margin: 0; font-size: 22px; font-weight: 600; letter-spacing: -0.04em; text-transform: none; color: var(--text); }
  .about-hero p { margin: 3px 0 0; font-size: 11.5px; color: var(--text-muted); }
  .donate-button {
    display: inline-flex;
    align-items: center;
    gap: 7px;
    flex-shrink: 0;
    padding: 9px 15px;
    border: 0;
    border-radius: 9px;
    background: #edc990;
    color: #2b2012;
    font: inherit;
    font-size: 11px;
    font-weight: 600;
    cursor: pointer;
    transition: filter 150ms ease, transform 120ms ease;
  }
  .donate-button:hover { filter: brightness(1.08); transform: translateY(-1px); }
  .about-foot { text-align: center; margin-top: auto; }

  /* Audio tab (HardwareSettings) lives inside the modal body */
  .settings-body :global(.hardware-settings) { display: flex; flex-direction: column; gap: 18px; }
  .settings-body :global(.hardware-toolbar) { display: flex; flex-wrap: wrap; align-items: center; justify-content: space-between; gap: 12px; }
  .settings-body :global(.hardware-badges) { display: flex; gap: 8px; flex-wrap: wrap; }
  .settings-body :global(.hardware-actions) { display: flex; flex-wrap: wrap; gap: 8px; }
  .settings-body :global(.hardware-settings label) { display: flex; }
  .settings-body :global(.virtual-routes-grid) { grid-template-columns: 1fr; }

  @media (max-width: 760px) {
    .settings-modal { grid-template-columns: 1fr; grid-template-rows: auto minmax(0, 1fr); }
    .settings-nav { flex-direction: row; align-items: center; padding: 10px; overflow-x: auto; border-right: 0; border-bottom: 1px solid var(--line); }
    .settings-nav h2, .settings-nav-foot { display: none; }
    .settings-tablist { flex-direction: row; }
    .settings-tab span { display: none; }
    .setting-row, .update-card, .donate-card { flex-wrap: wrap; }
  }
</style>
