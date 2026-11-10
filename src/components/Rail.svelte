<script lang="ts">
  import { onMount } from "svelte";
  import StudioIcon from "./StudioIcon.svelte";

  interface Props {
    streamEnabled: boolean;
    streamRunning: boolean;
    /** Number of apps currently producing audio. */
    appCount: number;
    settingsOpen: boolean;
    onOpenSettings: (tab?: string) => void;
    onScrollTop: (e?: MouseEvent) => void;
  }

  let { streamEnabled, streamRunning, appCount, settingsOpen, onOpenSettings, onScrollTop }: Props = $props();

  const ITEMS = [
    { id: "mixer", label: "Mixer", icon: "mix", tip: "Mixer · channel faders and meters" },
    { id: "routing", label: "Apps", icon: "route", tip: "App routing · drag apps onto channels" },
    { id: "stream", label: "Stream", icon: "broadcast", tip: "Stream mix · what your audience hears" },
    { id: "equalizer", label: "EQ", icon: "wave", tip: "Equalizer and fine-tune" },
  ] as const;

  let active = $state<string>("mixer");
  let navEl = $state<HTMLElement>();
  let indicator = $state({ y: 0, h: 0, ready: false });
  /** After a click, keep that item lit until the reader scrolls well away from where it landed. */
  let pinnedId: string | null = null;
  let pinnedY: number | null = null;
  let spyPausedUntil = 0;
  const PIN_RELEASE_PX = 160;

  function measure() {
    const el = navEl?.querySelector<HTMLElement>('[data-id="' + active + '"]');
    if (el) indicator = { y: el.offsetTop, h: el.offsetHeight, ready: true };
  }

  $effect(() => {
    void active;
    measure();
  });

  function spy() {
    if (performance.now() < spyPausedUntil) return;
    if (pinnedId) {
      if (pinnedY === null || Math.abs(window.scrollY - pinnedY) < PIN_RELEASE_PX) {
        if (active !== pinnedId) active = pinnedId;
        return;
      }
      pinnedId = null;
      pinnedY = null;
    }
    const line = window.innerHeight * 0.3;
    // The section whose top is closest above the reading line; side-by-side panels tie, so the wider one wins.
    let best: { id: string; top: number; width: number } | null = null;
    for (const item of ITEMS) {
      const el = document.getElementById(item.id);
      if (!el) continue;
      const rect = el.getBoundingClientRect();
      if (rect.top > line) continue;
      if (!best || rect.top > best.top + 40 || (Math.abs(rect.top - best.top) <= 40 && rect.width > best.width)) {
        best = { id: item.id, top: rect.top, width: rect.width };
      }
    }
    // Short pages cannot scroll the last section up to the line: use it once the page bottom is reached.
    const atBottom = window.innerHeight + window.scrollY >= document.documentElement.scrollHeight - 4;
    const current = atBottom ? ITEMS[ITEMS.length - 1].id : best?.id ?? ITEMS[0].id;
    if (current !== active) active = current;
  }

  function go(e: MouseEvent, id: string) {
    e.preventDefault();
    const target = document.getElementById(id);
    if (!target) return;
    active = id;
    pinnedId = id;
    pinnedY = null;
    spyPausedUntil = performance.now() + 800;
    const reduced = document.documentElement.dataset.motion === "reduced";
    target.scrollIntoView({ behavior: reduced ? "auto" : "smooth", block: "start" });
    history.replaceState(null, "", "#" + id);
    // Remember where the smooth scroll settled; scrolling well away from there releases the pin.
    setTimeout(() => {
      if (pinnedId === id) pinnedY = window.scrollY;
    }, 900);
  }

  onMount(() => {
    let frame = 0;
    const onScroll = () => {
      cancelAnimationFrame(frame);
      frame = requestAnimationFrame(spy);
    };
    const onResize = () => {
      measure();
      onScroll();
    };
    window.addEventListener("scroll", onScroll, { passive: true });
    window.addEventListener("resize", onResize);
    const ro = new ResizeObserver(measure);
    if (navEl) ro.observe(navEl);
    spy();
    measure();
    return () => {
      cancelAnimationFrame(frame);
      window.removeEventListener("scroll", onScroll);
      window.removeEventListener("resize", onResize);
      ro.disconnect();
    };
  });

  const streamBadge = $derived(!streamEnabled ? null : streamRunning ? "live" : "warn");
</script>

<aside class="rail" aria-label="Console navigation">
  <a class="brand-mark" href="#top" aria-label="Scroll to top of console" data-placement="right" data-tooltip="Scroll to top" onclick={onScrollTop}><svg viewBox="0 0 256 256" width="32" height="32" fill="none" aria-hidden="true"><path fill-rule="evenodd" fill="#b8a1fa" d="M128 20C128 82 174 128 236 128C174 128 128 174 128 236C128 174 82 128 20 128C82 128 128 82 128 20ZM128 86L170 128 128 170 86 128Z"/><polygon points="128,108 148,128 128,148 108,128" fill="#edc990"/><path fill="#87ddcf" d="M174 62A96 96 0 0 1 194 82L185 91A84 84 0 0 0 165 71ZM188 48A116 116 0 0 1 208 68L199 77A104 104 0 0 0 179 57ZM82 194A96 96 0 0 1 62 174L71 165A84 84 0 0 0 91 185ZM68 208A116 116 0 0 1 48 188L57 179A104 104 0 0 0 77 199Z"/><circle cx="70" cy="70" r="7" fill="#9bd6ff"/><circle cx="186" cy="186" r="7" fill="#9bd6ff"/></svg></a>

  <nav bind:this={navEl} aria-label="Sections">
    <span class="indicator" class:ready={indicator.ready} style:transform={"translateY(" + indicator.y + "px)"} style:height={indicator.h + "px"} aria-hidden="true"></span>
    {#each ITEMS as item (item.id)}
      <a href={"#" + item.id} class="rail-item" class:active={active === item.id} data-id={item.id}
        aria-current={active === item.id ? "location" : undefined} data-placement="right" data-tooltip={item.tip}
        onclick={(e) => go(e, item.id)}>
        <span class="icon">
          <StudioIcon name={item.icon} size={20} />
          {#if item.id === "stream" && streamBadge}<i class={"dot " + streamBadge} aria-label={streamBadge === "live" ? "Stream live" : "Stream on but not sending"}></i>{/if}
          {#if item.id === "routing" && appCount > 0}<b class="count" aria-label={appCount + " apps playing audio"}>{appCount > 9 ? "9+" : appCount}</b>{/if}
        </span>
        <span class="label">{item.label}</span>
      </a>
    {/each}
  </nav>

  <div class="bottom">
    <button type="button" class="rail-item" class:chosen={settingsOpen} aria-label="Settings" aria-haspopup="dialog" aria-expanded={settingsOpen}
      data-placement="right" data-tooltip="Settings (,)" onclick={() => onOpenSettings()}>
      <span class="icon"><StudioIcon name="settings" size={20} /></span><span class="label">Settings</span>
    </button>
  </div>
</aside>

<style>
  .rail { padding: 22px 8px 18px; gap: 26px; }
  nav { position: relative; display: grid; gap: 6px; width: 100%; }
  .bottom { margin-top: auto; display: grid; gap: 6px; width: 100%; }

  .indicator {
    position: absolute; left: 0; right: 0; top: 0; border-radius: 14px; pointer-events: none; opacity: 0;
    background: color-mix(in srgb, var(--violet, #b8a1fa) 14%, transparent);
    box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--violet, #b8a1fa) 22%, transparent);
    transition: transform .42s cubic-bezier(.22, 1, .36, 1), height .42s cubic-bezier(.22, 1, .36, 1), opacity .2s;
  }
  .indicator.ready { opacity: 1; }
  .indicator::before {
    content: ""; position: absolute; left: -8px; top: 24%; bottom: 24%; width: 3px; border-radius: 0 3px 3px 0;
    background: var(--violet, #b8a1fa); box-shadow: 0 0 10px color-mix(in srgb, var(--violet, #b8a1fa) 60%, transparent);
  }

  .rail-item {
    position: relative; z-index: 1; width: 100%; height: 56px; padding: 0; border: 0; border-radius: 14px; background: transparent;
    display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 5px;
    color: var(--text-muted, #85848f); text-decoration: none; cursor: pointer; transition: color .2s, background .2s;
  }
  .rail-item:hover { color: var(--text, #e5ddfc); background: color-mix(in srgb, var(--text, #fff) 6%, transparent); }
  .rail-item.active { color: var(--violet, #b8a1fa); background: transparent; }
  .rail-item.chosen { color: var(--text, #e5ddfc); background: color-mix(in srgb, var(--text, #fff) 8%, transparent); }
  .rail-item:focus-visible { outline: 2px solid var(--violet, #b8a1fa); outline-offset: -2px; }
  .icon { position: relative; display: grid; place-items: center; transition: transform .35s cubic-bezier(.22, 1, .36, 1); }
  .rail-item.active .icon { transform: translateY(-1px) scale(1.08); }
  .label { font-size: 9.5px; font-weight: 600; letter-spacing: .01em; line-height: 1; }

  .dot { position: absolute; top: -3px; right: -5px; width: 8px; height: 8px; border-radius: 50%; border: 2px solid var(--page, #111113); background: #6f667c; box-sizing: content-box; }
  .dot.live { background: var(--teal, #7be1d1); box-shadow: 0 0 8px color-mix(in srgb, var(--teal, #7be1d1) 70%, transparent); }
  .dot.warn { background: var(--amber, #edc990); }
  .count { position: absolute; top: -6px; right: -10px; min-width: 15px; height: 15px; padding: 0 4px; border-radius: 8px; display: grid; place-items: center; font-size: 9px; font-weight: 700; color: var(--on-violet, #1b1426); background: var(--violet, #b8a1fa); font-variant-numeric: tabular-nums; }

  @media (max-width: 720px) {
    .rail { padding: 22px 5px; }
    .rail-item { height: 40px; }
    .label { display: none; }
  }
  @media (prefers-reduced-motion: reduce) {
    .indicator, .icon { transition: none; }
  }
  :global(:root[data-motion="reduced"]) .indicator, :global(:root[data-motion="reduced"]) .icon { transition: none; }
</style>
