(() => {
  const VIEWS = [
    {
      id: "mixer",
      label: "Five-Bus Mixer",
      color: "#b8a1fa",
      image: "media/mixer.webp",
      alt: "Stellars five-channel mixer with live dot-matrix meters, faders, ChatMix and Monitor output",
      portrait: false,
      hotspots: [
        {
          id: "channels",
          x: 14,
          y: 30,
          align: "right",
          color: "#b7a3ff",
          tag: "5 Logical Buses",
          title: "Game, Chat, Media, Aux & Mic",
          body: "Four playback submixes plus a dedicated microphone pipeline. Each bus carries its own color across routing, meters, and EQ."
        },
        {
          id: "meters",
          x: 31,
          y: 41,
          align: "right",
          color: "#7be1d1",
          tag: "Live Telemetry",
          title: "Dot-matrix Peak, RMS & LUFS",
          body: "Real-time stereo activity with clip indicators. Meters automatically throttle in the background and pause when minimized to save resources."
        },
        {
          id: "pregain",
          x: 27,
          y: 55,
          align: "right",
          color: "#a1baff",
          tag: "Input Trim",
          title: "Pre-gain & precise dB readouts",
          body: "Trim input gain from -12 to +12 dB before EQ and dynamics. Double-click or press F2 on any numeric readout to type an exact value."
        },
        {
          id: "faders",
          x: 14,
          y: 73,
          align: "right",
          color: "#b7a3ff",
          tag: "Tactile Control",
          title: "0–150% Faders, Solo & Mute",
          body: "Fine-tune with the scroll wheel, double-click to snap back to unity (0 dB), solo a bus to isolate it, or click Edit channel to open its DSP rack."
        },
        {
          id: "chatmix",
          x: 25,
          y: 89,
          align: "top",
          color: "#b8a1fa",
          tag: "Balance",
          title: "ChatMix crossfader",
          body: "One quadratic crossfader to tilt between Game and Chat on the fly—ideal when comms heat up mid-match."
        },
        {
          id: "monitor",
          x: 58,
          y: 89,
          align: "top",
          color: "#87ddcf",
          tag: "Headphone Output",
          title: "Monitor output + -0.1 dBFS limiter",
          body: "Master level for your headphones or speakers, protected by a transparent -0.1 dBFS soft-limiter."
        },
        {
          id: "routing-rail",
          x: 86,
          y: 52,
          align: "left",
          color: "#edc990",
          tag: "Process Routing",
          title: "Live Windows app pool",
          body: "Every active audio process appears here. Drag any app onto a bus or pick a destination from its menu—no kernel drivers required."
        }
      ]
    },
    {
      id: "routing",
      label: "App Routing",
      color: "#7be1d1",
      image: "media/routing.webp",
      alt: "Stellars app routing panel showing running Windows processes and channel pickers",
      portrait: true,
      hotspots: [
        {
          id: "search-rules",
          x: 50,
          y: 20,
          align: "right",
          color: "#7be1d1",
          tag: "Instant Filtering",
          title: "Search, Unrouted & Rules tabs",
          body: "Search running processes by name, filter to unrouted apps, or inspect and manage saved routing rules."
        },
        {
          id: "per-app",
          x: 68,
          y: 40,
          align: "left",
          color: "#b7a3ff",
          tag: "Persistent Rules",
          title: "Per-app destination & volume",
          body: "Assign discord.exe to Chat or cs2.exe to Game in one click. Rules are remembered automatically across sessions."
        },
        {
          id: "dnd-restore",
          x: 50,
          y: 86,
          align: "top",
          color: "#edc990",
          tag: "Zero Friction",
          title: "Drag to route, drag back to reset",
          body: "Drag an app onto any channel strip to capture it via Process Loopback, or drag it back to the pool to restore Windows default output."
        }
      ]
    },
    {
      id: "equalizer",
      label: "10-Band EQ & DSP",
      color: "#a1baff",
      image: "media/equalizer.webp",
      alt: "Stellars 10-band parametric EQ with FPS footsteps curve, noise gate and compressor",
      portrait: false,
      hotspots: [
        {
          id: "eq-presets",
          x: 56,
          y: 54,
          align: "bottom",
          color: "#b8a1fa",
          tag: "Quick Presets",
          title: "Presets, A/B compare & custom curves",
          body: "Switch between Flat, FPS Footsteps, Voice Clarity, and Bass Boost—or save your own presets and A/B test changes."
        },
        {
          id: "eq-canvas",
          x: 35,
          y: 69,
          align: "top",
          color: "#a1baff",
          tag: "48 kHz Float32 DSP",
          title: "10-band RBJ parametric EQ",
          body: "Drag frequency nodes directly on the response curve. Direct Form II Transposed biquads with 128-frame zipper-free ramping."
        },
        {
          id: "band-sliders",
          x: 42,
          y: 80,
          align: "top",
          color: "#87ddcf",
          tag: "Surgical Shaping",
          title: "Frequency, Q factor & filter type",
          body: "Open Band controls below the curve to dial exact Hz, Q bandwidth, and filter shape per band."
        },
        {
          id: "dynamics",
          x: 86,
          y: 72,
          align: "left",
          color: "#f0a7c6",
          tag: "Dynamics Rack",
          title: "Noise gate, compressor, de-esser & limiter",
          body: "Shape dynamics per channel with live gain-reduction readouts, plus RNNoise or DeepFilterNet3 AI noise suppression for Mic and Chat."
        }
      ]
    },
    {
      id: "stream",
      label: "Stream Mix",
      color: "#9bd6ff",
      image: "media/stream.webp",
      alt: "Stellars Stream mix card with dedicated output level, sources and mix settings",
      portrait: false,
      hotspots: [
        {
          id: "stream-cable",
          x: 20,
          y: 50,
          align: "bottom",
          color: "#9bd6ff",
          tag: "Broadcast Output",
          title: "Independent 48 kHz Stream Mix",
          body: "Routes a separate stereo mix to your virtual cable for OBS or Discord—your audience hears what you pick without changing your headphones."
        },
        {
          id: "stream-sources",
          x: 63,
          y: 48,
          align: "bottom",
          color: "#87ddcf",
          tag: "Granular Control",
          title: "Bus includes + per-app overrides",
          body: "Include whole buses or add executable-level Include/Exclude overrides for up to 32 active captured apps."
        },
        {
          id: "stream-level",
          x: 83,
          y: 48,
          align: "left",
          color: "#b8a1fa",
          tag: "Solo Preview",
          title: "Dedicated level, mute & monitor solo",
          body: "Adjust stream volume independently with its own limiter, or solo the stream mix into your headphones to verify what viewers hear."
        }
      ]
    }
  ];

  const tabsEl = document.getElementById("tour-tabs");
  const canvasEl = document.getElementById("stage-canvas");
  const imageEl = document.getElementById("stage-image");
  const hotspotsLayerEl = document.getElementById("hotspots-layer");
  const listEl = document.getElementById("tour-list");
  const dockTitleEl = document.getElementById("dock-title");
  const dockDescEl = document.getElementById("dock-desc");
  const dockCounterEl = document.getElementById("dock-counter");
  const prevBtn = document.getElementById("tour-prev");
  const nextBtn = document.getElementById("tour-next");
  const showAllBtn = document.getElementById("toggle-all-bubbles");

  if (!tabsEl || !canvasEl || !imageEl || !hotspotsLayerEl) return;

  let activeViewIndex = 0;
  let activeHotspotIndex = 0;
  let showAll = false;

  function renderTabs() {
    tabsEl.innerHTML = "";
    VIEWS.forEach((view, idx) => {
      const btn = document.createElement("button");
      btn.type = "button";
      btn.className = "tour-tab";
      btn.role = "tab";
      btn.setAttribute("aria-selected", String(idx === activeViewIndex));
      btn.innerHTML = `<span class="tour-tab-dot" style="background:${view.color}"></span><span>${view.label}</span>`;
      btn.addEventListener("click", () => {
        activeViewIndex = idx;
        activeHotspotIndex = 0;
        renderView();
      });
      tabsEl.appendChild(btn);
    });
  }

  function renderView() {
    const view = VIEWS[activeViewIndex];
    renderTabs();

    imageEl.src = view.image;
    imageEl.alt = view.alt;
    imageEl.classList.toggle("portrait-frame", Boolean(view.portrait));

    canvasEl.classList.toggle("show-all", showAll);
    hotspotsLayerEl.innerHTML = "";
    listEl.innerHTML = "";

    view.hotspots.forEach((spot, idx) => {
      const isActive = idx === activeHotspotIndex;

      // Hotspot + Floating Text Bubble
      const holder = document.createElement("div");
      holder.className = `hotspot${isActive ? " is-active" : ""}`;
      holder.style.left = `${spot.x}%`;
      holder.style.top = `${spot.y}%`;
      holder.style.setProperty("--pin-color", spot.color);

      const pin = document.createElement("button");
      pin.type = "button";
      pin.className = "hotspot-pin";
      pin.textContent = String(idx + 1);
      pin.setAttribute("aria-label", `${spot.title}: ${spot.body}`);
      pin.setAttribute("aria-pressed", String(isActive));
      pin.addEventListener("click", () => selectHotspot(idx));

      const bubble = document.createElement("div");
      bubble.className = "bubble-card";
      bubble.dataset.align = spot.align || "right";
      bubble.innerHTML = `
        <span class="bubble-tag">${spot.tag}</span>
        <h3 class="bubble-title">${spot.title}</h3>
        <p class="bubble-body">${spot.body}</p>
      `;

      holder.appendChild(pin);
      holder.appendChild(bubble);
      hotspotsLayerEl.appendChild(holder);

      // Bottom clickable index item
      const itemBtn = document.createElement("button");
      itemBtn.type = "button";
      itemBtn.className = "tour-list-btn";
      if (isActive) itemBtn.setAttribute("aria-current", "true");
      itemBtn.innerHTML = `
        <span class="tour-list-num" style="background:${spot.color}">${idx + 1}</span>
        <span class="tour-list-label">${spot.title}</span>
      `;
      itemBtn.addEventListener("click", () => selectHotspot(idx));
      listEl.appendChild(itemBtn);
    });

    updateDock();
  }

  function selectHotspot(idx) {
    activeHotspotIndex = idx;
    const holders = hotspotsLayerEl.querySelectorAll(".hotspot");
    const listBtns = listEl.querySelectorAll(".tour-list-btn");

    holders.forEach((el, i) => {
      const active = i === idx;
      el.classList.toggle("is-active", active);
      const pin = el.querySelector(".hotspot-pin");
      if (pin) pin.setAttribute("aria-pressed", String(active));
    });

    listBtns.forEach((btn, i) => {
      if (i === idx) btn.setAttribute("aria-current", "true");
      else btn.removeAttribute("aria-current");
    });

    updateDock();
  }

  function updateDock() {
    const view = VIEWS[activeViewIndex];
    const spot = view.hotspots[activeHotspotIndex];
    if (!spot) return;

    dockTitleEl.textContent = `${activeHotspotIndex + 1}. ${spot.title}`;
    dockDescEl.textContent = spot.body;
    dockCounterEl.textContent = `${activeHotspotIndex + 1} / ${view.hotspots.length}`;
  }

  prevBtn?.addEventListener("click", () => {
    const len = VIEWS[activeViewIndex].hotspots.length;
    selectHotspot((activeHotspotIndex - 1 + len) % len);
  });

  nextBtn?.addEventListener("click", () => {
    const len = VIEWS[activeViewIndex].hotspots.length;
    selectHotspot((activeHotspotIndex + 1) % len);
  });

  showAllBtn?.addEventListener("click", () => {
    showAll = !showAll;
    showAllBtn.setAttribute("aria-pressed", String(showAll));
    canvasEl.classList.toggle("show-all", showAll);
  });

  document.addEventListener("keydown", (event) => {
    const tourSection = document.getElementById("tour");
    if (!tourSection) return;
    const rect = tourSection.getBoundingClientRect();
    const inView = rect.top < window.innerHeight && rect.bottom > 0;
    if (!inView) return;
    if (event.target instanceof HTMLInputElement || event.target instanceof HTMLTextAreaElement) return;

    if (event.key === "ArrowRight") {
      const len = VIEWS[activeViewIndex].hotspots.length;
      selectHotspot((activeHotspotIndex + 1) % len);
    } else if (event.key === "ArrowLeft") {
      const len = VIEWS[activeViewIndex].hotspots.length;
      selectHotspot((activeHotspotIndex - 1 + len) % len);
    }
  });

  renderView();

  // Resolve direct .exe download URL from GitHub Releases API (falls back to /releases/latest)
  (async function resolveLatestExe() {
    const links = document.querySelectorAll("a[data-download-exe]");
    if (!links.length) return;
    try {
      const res = await fetch("https://api.github.com/repos/giran03/Stellars/releases/latest", {
        headers: { Accept: "application/vnd.github+json" }
      });
      if (!res.ok) return;
      const release = await res.json();
      const assets = Array.isArray(release.assets) ? release.assets : [];
      const exe =
        assets.find((a) => typeof a.name === "string" && /x64-setup\.exe$/i.test(a.name)) ||
        assets.find((a) => typeof a.name === "string" && /\.exe$/i.test(a.name));
      if (!exe || !exe.browser_download_url) return;

      links.forEach((link) => {
        link.href = exe.browser_download_url;
        link.removeAttribute("target");
        link.setAttribute("download", exe.name);
        const baseLabel = link.getAttribute("data-download-label");
        if (baseLabel && release.tag_name) {
          link.textContent = `${baseLabel} (${release.tag_name})`;
        }
      });
    } catch {
      // Keep fallback link to GitHub Releases page
    }
  })();
})();
