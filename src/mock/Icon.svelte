<script lang="ts">
  export let name = "mix";
  export let size = 20;
  const paths: Record<string, string> = {
    mix: 'M5 3v18M12 3v18M19 3v18M2 8h6M9 16h6M16 8h6',
    route: 'M3 5h18M18 2l3 3-3 3M9 5a4 4 0 0 1 4 4v6a4 4 0 0 0 4 4h4M18 16l3 3-3 3',
    wave: 'M2 12h3l3-7 4 14 4-14 3 7h3',
    settings: 'M4 5h16M4 12h16M4 19h16M14 2v6M8 9v6M16 16v6',
    game: 'M6 8h12a4 4 0 0 1 4 3l1 6a2 2 0 0 1-3 2l-3-3H9l-3 3a2 2 0 0 1-3-2l1-6a4 4 0 0 1 4-3zM8 10v4M6 12h4M17 10h.01M17 14h.01M15 12h.01M19 12h.01',
    chat: 'M7.9 20A9 9 0 1 0 4 16.1L2 22zM8 12h.01M12 12h.01M16 12h.01',
    music: 'M9 18V5l11-2v13M9 8l11-2M9 18a3 3 0 1 1-3-3c1.6 0 3 1.3 3 3ZM20 16a3 3 0 1 1-3-3c1.6 0 3 1.3 3 3Z',
    aux: 'M12 2v3M10 5h4v4.5h-4zM10 7.25h4M8 9.5h8v4.5a2 2 0 0 1-2 2h-4a2 2 0 0 1-2-2zM12 16v2.5a3.5 3.5 0 0 0 3.5 3.5h2',
    mic: 'M9 5a3 3 0 0 1 6 0v7a3 3 0 0 1-6 0ZM5 10v2a7 7 0 0 0 14 0v-2M12 19v3M8 22h8',
    headphones: 'M3 14v-3a9 9 0 0 1 18 0v3M3 13h4v8H5a2 2 0 0 1-2-2ZM21 13h-4v8h2a2 2 0 0 0 2-2Z',
    broadcast: 'M4.9 19.1C1 15.2 1 8.8 4.9 4.9M7.8 16.2c-2.3-2.3-2.3-6.1 0-8.5M16.2 7.8c2.3 2.3 2.3 6.1 0 8.5M19.1 4.9C23 8.8 23 15.1 19.1 19M12 11a1.5 1.5 0 1 0 0-3 1.5 1.5 0 0 0 0 3zM12 12.5v9.5',
    chevron: 'm9 5 7 7-7 7',
    down: 'm6 9 6 6 6-6',
    up: 'm6 15 6-6 6 6',
    plus: 'M12 5v14M5 12h14',
    search: 'M21 21l-5-5M18 10a8 8 0 1 1-16 0 8 8 0 0 1 16 0Z',
    arrow: 'M5 12h14m-5-5 5 5-5 5',
    volume: 'M4 9h4l5-5v16l-5-5H4ZM16 9a5 5 0 0 1 0 6M19.4 5.6a9 9 0 0 1 0 12.8',
    mute: 'M4 9h4l5-5v16l-5-5H4ZM16 9l6 6M22 9l-6 6',
    play: 'm8 4 12 8-12 8Z',
    pause: 'M8 5v14M16 5v14',
    check: 'm5 12 4 4L19 6',
    reset: 'M3 12a9 9 0 1 0 9-9 9.75 9.75 0 0 0-6.74 2.74L3 8M3 3v5h5',
    close: 'm6 6 12 12M18 6 6 18',
    grid: 'M4 3h5a1 1 0 0 1 1 1v5a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1V4a1 1 0 0 1 1-1zM15 3h5a1 1 0 0 1 1 1v5a1 1 0 0 1-1 1h-5a1 1 0 0 1-1-1V4a1 1 0 0 1 1-1zM4 14h5a1 1 0 0 1 1 1v5a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1v-5a1 1 0 0 1 1-1zM15 14h5a1 1 0 0 1 1 1v5a1 1 0 0 1-1 1h-5a1 1 0 0 1-1-1v-5a1 1 0 0 1 1-1z',
    info: 'M12 11v6M12 7h.01M22 12a10 10 0 1 1-20 0 10 10 0 0 1 20 0Z',
  };
</script>

<svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.65" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d={paths[name] || paths.mix}/></svg>
