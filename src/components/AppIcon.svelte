<script lang="ts">
  import type { ActiveAudioProcess } from '../types';

  type Props = {
    proc?: Partial<ActiveAudioProcess> & { name?: string };
    exeName?: string;
    displayName?: string;
    icon?: string;
    size?: number;
    color?: string;
  };

  let {
    proc,
    exeName = proc?.exeName ?? '',
    displayName = proc?.displayName ?? '',
    icon = proc?.icon,
    size = 24,
    color = '#b7a3ff'
  }: Props = $props();

  let imgFailed = $state(false);

  // Normalize exe name and display name for signature matching
  const key = $derived((exeName || displayName || '').toLowerCase());

  // Determine application identity
  const brand = $derived.by(() => {
    if (key.includes('systemsounds') || key.includes('system sounds') || key.includes('audiosrv')) return 'systemsounds';
    if (key.includes('discord')) return 'discord';
    if (key.includes('spotify')) return 'spotify';
    if (key.includes('steam')) return 'steam';
    if (key.includes('chrome')) return 'chrome';
    if (key.includes('edge') || key.includes('msedge')) return 'edge';
    if (key.includes('firefox')) return 'firefox';
    if (key.includes('brave')) return 'brave';
    if (key.includes('zen')) return 'zen';
    if (key.includes('wavee')) return 'wavee';
    if (key.includes('wallpaper')) return 'wallpaper';
    if (key.includes('obs')) return 'obs';
    if (key.includes('vlc')) return 'vlc';
    if (key.includes('itunes') || key.includes('music') || key.includes('apple')) return 'applemusic';
    if (key.includes('foobar')) return 'foobar';
    if (key.includes('telegram')) return 'telegram';
    if (key.includes('slack')) return 'slack';
    if (key.includes('valorant') || key.includes('riot') || key.includes('league') || key.includes('cs2') || key.includes('csgo') || key.includes('game')) return 'game';
    return null;
  });
</script>

<span
  class="app-icon-badge"
  style={`--app-accent:${color}; width:${size}px; height:${size}px; min-width:${size}px; min-height:${size}px;`}
  aria-hidden="true"
>
  {#if icon && !imgFailed}
    <img
      src={icon}
      alt={displayName || exeName}
      class="native-app-icon"
      width={size}
      height={size}
      onerror={() => (imgFailed = true)}
    />
  {:else if brand === 'discord'}
    <svg viewBox="0 0 24 24" width={size * 0.72} height={size * 0.72} fill="#5865F2">
      <path d="M20.317 4.37a19.791 19.791 0 0 0-4.885-1.515.074.074 0 0 0-.079.037c-.21.375-.444.864-.608 1.25a18.27 18.27 0 0 0-5.487 0 12.64 12.64 0 0 0-.617-1.25.077.077 0 0 0-.079-.037A19.736 19.736 0 0 0 3.677 4.37a.07.07 0 0 0-.032.027C.533 9.046-.32 13.58.099 18.057a.082.082 0 0 0 .031.057 19.9 19.9 0 0 0 5.993 3.03.078.078 0 0 0 .084-.028c.462-.63.874-1.295 1.226-1.994.021-.041.001-.09-.041-.106a13.107 13.107 0 0 1-1.872-.892.077.077 0 0 1-.008-.128 10.2 10.2 0 0 0 .372-.292.074.074 0 0 1 .077-.01c3.929 1.793 8.18 1.793 12.061 0a.074.074 0 0 1 .078.01c.12.098.246.198.373.292a.077.077 0 0 1-.006.127 12.299 12.299 0 0 1-1.873.893.077.077 0 0 0-.041.107c.36.698.772 1.362 1.225 1.993a.076.076 0 0 0 .084.028 19.839 19.839 0 0 0 6.002-3.03.077.077 0 0 0 .032-.054c.5-5.177-.838-9.674-3.549-13.66a.061.061 0 0 0-.031-.028zM8.02 15.33c-1.183 0-2.157-1.085-2.157-2.419 0-1.333.956-2.419 2.157-2.419 1.21 0 2.176 1.096 2.157 2.42 0 1.333-.956 2.418-2.157 2.418zm7.975 0c-1.183 0-2.157-1.085-2.157-2.419 0-1.333.955-2.419 2.157-2.419 1.21 0 2.176 1.096 2.157 2.42 0 1.333-.946 2.418-2.157 2.418z"/>
    </svg>
  {:else if brand === 'spotify'}
    <svg viewBox="0 0 24 24" width={size * 0.72} height={size * 0.72} fill="#1DB954">
      <path d="M12 0C5.373 0 0 5.373 0 12s5.373 12 12 12 12-5.373 12-12S18.627 0 12 0zm5.494 17.307a.747.747 0 0 1-1.03.248c-2.822-1.724-6.375-2.115-10.56-1.158a.75.75 0 1 1-.334-1.462c4.58-1.045 8.508-.598 11.676 1.342.348.213.46.666.248 1.03zm1.466-3.26a.936.936 0 0 1-1.288.309c-3.23-1.986-8.156-2.56-11.977-1.4a.938.938 0 1 1-.548-1.794c4.37-1.326 9.794-.686 13.504 1.597.41.253.54.789.309 1.288zm.126-3.41c-3.873-2.3-10.258-2.512-13.961-1.388a1.124 1.124 0 1 1-.65-2.15c4.254-1.29 11.306-1.039 15.776 1.614a1.124 1.124 0 1 1-1.165 1.924z"/>
    </svg>
  {:else if brand === 'steam'}
    <svg viewBox="0 0 24 24" width={size * 0.72} height={size * 0.72} fill="#66c0f4">
      <path d="M11.979 0C5.678 0 .511 4.86.022 11.037l6.432 2.658c.545-.371 1.203-.59 1.912-.59.063 0 .125.004.188.006l2.861-4.142V8.91c0-2.495 2.028-4.524 4.524-4.524 2.494 0 4.524 2.029 4.524 4.524s-2.03 4.524-4.524 4.524h-.105l-4.076 2.911c0 .052.005.105.005.159 0 1.875-1.515 3.396-3.395 3.396-1.535 0-2.83-1.018-3.255-2.421L.59 15.42C2.196 20.354 6.812 24 12.23 24c6.627 0 12-5.373 12-12S18.607 0 11.979 0zM7.545 14.81l-1.46-.604c.264-.469.721-.818 1.267-.932.195.348.455.656.766.903-.178.197-.378.397-.573.633zm8.396-8.118c1.378 0 2.496 1.117 2.496 2.495s-1.118 2.495-2.496 2.495-2.495-1.117-2.495-2.495 1.117-2.495 2.495-2.495zm-7.575 9.873c-.92 0-1.666-.746-1.666-1.666 0-.309.085-.6.233-.848l1.503.621c-.019.076-.03.155-.03.237 0 .548.444.992.991.992.115 0 .224-.021.328-.056l.635 1.538c-.596.118-1.25.182-1.994.182z"/>
    </svg>
  {:else if brand === 'chrome'}
    <svg viewBox="0 0 24 24" width={size * 0.72} height={size * 0.72}>
      <circle cx="12" cy="12" r="10" fill="#4285F4"/>
      <circle cx="12" cy="12" r="4.5" fill="#ffffff"/>
      <circle cx="12" cy="12" r="3.2" fill="#1a73e8"/>
    </svg>
  {:else if brand === 'firefox'}
    <svg viewBox="0 0 24 24" width={size * 0.72} height={size * 0.72} fill="#FF7139">
      <path d="M12 2a10 10 0 1 0 10 10A10 10 0 0 0 12 2zm6.2 6.8c-.3 1.2-1 2.3-2 3.1a5.6 5.6 0 0 1-3.6 1.2c-2.4 0-4.4-1.8-4.6-4.2a4.9 4.9 0 0 1 2-4.1c1.3-.9 2.9-1.2 4.4-.8a7.8 7.8 0 0 1 3.8 4.8z"/>
    </svg>
  {:else if brand === 'edge'}
    <svg viewBox="0 0 24 24" width={size * 0.72} height={size * 0.72} fill="#0078D7">
      <path d="M19.4 14.5c-.3 3.6-3.4 6.5-7.3 6.5-4.2 0-7.6-3.4-7.6-7.6 0-3.3 2.1-6.1 5.1-7.1-.4 1.2-.4 2.6.2 3.8.7 1.4 2.1 2.4 3.7 2.4 2 0 3.8-1.3 4.4-3.2.9 1.5 1.5 3.3 1.5 5.2z"/>
    </svg>
  {:else if brand === 'zen'}
    <!-- Zen browser circular icon -->
    <svg viewBox="0 0 24 24" width={size * 0.72} height={size * 0.72} fill="none" stroke="#b8a1fa" stroke-width="2.2" stroke-linecap="round">
      <circle cx="12" cy="12" r="8" stroke-dasharray="38 8"/>
      <circle cx="12" cy="12" r="2.5" fill="#b8a1fa"/>
    </svg>
  {:else if brand === 'wavee'}
    <!-- Wavee sound waves icon -->
    <svg viewBox="0 0 24 24" width={size * 0.72} height={size * 0.72} fill="none" stroke="#7be1d1" stroke-width="2" stroke-linecap="round">
      <path d="M3 12h2M7 8v8M11 5v14M15 9v6M19 7v10M21 12h1"/>
    </svg>
  {:else if brand === 'wallpaper'}
    <!-- Wallpaper Engine monitor gear -->
    <svg viewBox="0 0 24 24" width={size * 0.72} height={size * 0.72} fill="none" stroke="#1ea7fd" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
      <rect x="2" y="3" width="20" height="14" rx="2"/>
      <line x1="8" y1="21" x2="16" y2="21"/>
      <line x1="12" y1="17" x2="12" y2="21"/>
      <circle cx="12" cy="10" r="3"/>
    </svg>
  {:else if brand === 'obs'}
    <!-- OBS Studio 3-pinwheel icon -->
    <svg viewBox="0 0 24 24" width={size * 0.72} height={size * 0.72} fill="none" stroke="#ffffff" stroke-width="2" stroke-linecap="round">
      <circle cx="12" cy="12" r="9"/>
      <circle cx="12" cy="9" r="2" fill="#fff"/>
      <circle cx="9.5" cy="14" r="2" fill="#fff"/>
      <circle cx="14.5" cy="14" r="2" fill="#fff"/>
    </svg>
  {:else if brand === 'vlc'}
    <!-- VLC traffic cone -->
    <svg viewBox="0 0 24 24" width={size * 0.72} height={size * 0.72} fill="#FF8800">
      <path d="M12 2L6 18h12L12 2zm-3.6 13l1.1-3h5l1.1 3H8.4zM4 20h16v2H4v-2z"/>
    </svg>
  {:else if brand === 'systemsounds'}
    <!-- Windows System Sounds icon: speaker with broadcast waves -->
    <svg viewBox="0 0 24 24" width={size * 0.72} height={size * 0.72} fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
      <polygon points="11 5 6 9 2 9 2 15 6 15 11 19 11 5" fill="currentColor" fill-opacity="0.2"/>
      <path d="M15.54 8.46a5 5 0 0 1 0 7.07"/>
      <path d="M19.07 4.93a10 10 0 0 1 0 14.14"/>
    </svg>
  {:else if brand === 'game'}
    <!-- Gaming controller -->
    <svg viewBox="0 0 24 24" width={size * 0.72} height={size * 0.72} fill="none" stroke="#b7a3ff" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
      <path d="M6 12h4m-2-2v4m7-2h.01m2-2h.01M6 6h12a4 4 0 0 1 4 4v4a4 4 0 0 1-4 4l-3-2H9l-3 2a4 4 0 0 1-4-4v-4a4 4 0 0 1 4-4z"/>
    </svg>
  {:else}
    <!-- Official Stellars Pulsar Prism placeholder -->
    <svg viewBox="0 0 256 256" width={size * 0.75} height={size * 0.75} fill="none" class="stellars-placeholder-star" aria-hidden="true">
      <path fill-rule="evenodd" fill="currentColor" d="M128 20C128 82 174 128 236 128C174 128 128 174 128 236C128 174 82 128 20 128C82 128 128 82 128 20ZM128 86L170 128 128 170 86 128Z"/>
      <polygon points="128,108 148,128 128,148 108,128" fill="#edc990"/>
      <path fill="#87ddcf" d="M174 62A96 96 0 0 1 194 82L185 91A84 84 0 0 0 165 71ZM188 48A116 116 0 0 1 208 68L199 77A104 104 0 0 0 179 57ZM82 194A96 96 0 0 1 62 174L71 165A84 84 0 0 0 91 185ZM68 208A116 116 0 0 1 48 188L57 179A104 104 0 0 0 77 199Z"/>
      <circle cx="70" cy="70" r="7" fill="#9bd6ff"/>
      <circle cx="186" cy="186" r="7" fill="#9bd6ff"/>
    </svg>
  {/if}
</span>

<style>
  .app-icon-badge {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    border-radius: 8px;
    background: color-mix(in srgb, var(--app-accent, #b7a3ff) 14%, #18161d);
    border: 1px solid color-mix(in srgb, var(--app-accent, #b7a3ff) 22%, transparent);
    color: var(--app-accent, #b7a3ff);
    flex-shrink: 0;
    overflow: hidden;
    position: relative;
    user-select: none;
    box-shadow: 0 2px 6px rgba(0, 0, 0, 0.22);
    transition: transform 180ms cubic-bezier(0.34, 1.56, 0.64, 1), box-shadow 180ms ease-out;
  }
  .app-icon-badge:hover {
    transform: scale(1.08);
    box-shadow: 0 4px 12px rgba(0, 0, 0, 0.35);
  }
  .native-app-icon {
    width: 100%;
    height: 100%;
    object-fit: contain;
    border-radius: 7px;
  }
  .stellars-placeholder-star {
    filter: drop-shadow(0 1px 3px rgba(0, 0, 0, 0.4));
  }
  :global(:root[data-theme="light"]) .app-icon-badge {
    background: color-mix(in srgb, var(--app-accent, #b7a3ff) 16%, #ffffff);
    border: 1px solid color-mix(in srgb, var(--app-accent, #b7a3ff) 36%, #d8d4e4);
    box-shadow: 0 2px 6px rgba(35, 25, 55, 0.08);
  }
  :global(:root[data-theme="light"]) .stellars-placeholder-star {
    filter: drop-shadow(0 1px 2px rgba(35, 25, 55, 0.15));
  }
</style>
