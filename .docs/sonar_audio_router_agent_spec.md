# AGENT SPECIFICATION: Lightweight Rust + Tauri Multi-Bus Audio Router (Sonar Clone)

## 0. CORE CONSTRAINTS, STACK & ANTI-CHEAT SAFETY

* **Tech Stack:**
  * **Audio Engine & OS Interop:** Rust (`edition = "2021"`, `windows` crate v0.58+, `rtrb` lock-free SPSC ring buffer, `serde`).
  * **UI & IPC Shell:** Tauri v2 (Lightweight Frontend + Rust Core; destroy/suspend WebView2 window when minimized to System Tray so background RAM stays under $15\text{ MB}$).
* **Target OS:** Windows 10 (Build $20348+$) / Windows 11 ($64\text{-bit}$) with Secure Boot ON and Driver Signature Enforcement (DSE) ON.
* **Kernel & Anti-Cheat Safety Law ($100\%$ Ring-3 User-Mode):**
  * Zero custom `.sys` kernel drivers.
  * **NEVER** call `OpenProcess` with `PROCESS_VM_READ`, `PROCESS_VM_WRITE`, or `PROCESS_ALL_ACCESS` on target apps. Use max `PROCESS_QUERY_LIMITED_INFORMATION` ($0\text{x}1000$) solely to read executable paths/names.
  * All audio capture and routing MUST go through `Audiosrv` / `audiodg.exe` via `ActivateAudioInterfaceAsync` and `IAudioPolicyConfig` (identical to OBS Studio and Windows 11 Volume Mixer).
* **Scope Exclusions:** Do **NOT** implement AI Noise Suppression.
* **Audio Pipeline Standard:** $48000\text{ Hz}$, $32\text{-bit}$ IEEE Float (`WAVE_FORMAT_IEEE_FLOAT`), $2\text{ Channels}$ (Interleaved Stereo), $10\text{ ms}$ frame period ($480\text{ frames}$ per callback).
* **Real-Time Threading Law:** Zero heap allocations (`Box`, `Vec::push`, `String`), zero mutex locks (`std::sync::Mutex`), and zero syscalls inside WASAPI audio callbacks. Use `rtrb::RingBuffer` for audio and `std::sync::atomic` (`Ordering::Relaxed` / `Ordering::Acquire`) for DSP parameter updates. Promote audio threads via `AvSetMmThreadCharacteristicsW("Pro Audio")`.

## 1. SYSTEM ARCHITECTURE & SIGNAL FLOW

```
[App PIDs: Game / Chat / Media / Aux]
   │ (Routed via IAudioPolicyConfig COM to Silent WHQL Dummy Sink so dry audio is muted)
   ▼
[Per-Process WASAPI Loopback Capture (VIRTUAL_AUDIO_DEVICE_PROCESS_LOOPBACK)]
   │ (Writes interleaved f32 stereo frames to per-PID rtrb::Producer<f32>)
   ▼
[Adaptive Fractional Resampler (Clock-Drift Compensation)]
   │ (Pulls from rtrb::Consumer<f32> & sums assigned PIDs into Logical Bus Buffers)
   ▼
┌───────────────────┬───────────────────┬───────────────────┬───────────────────┐
│     GAME BUS      │     CHAT BUS      │     MEDIA BUS     │      AUX BUS      │
│ 1. Pre-Gain       │ 1. Pre-Gain       │ 1. Pre-Gain       │ 1. Pre-Gain       │
│ 2. 10-Band PEQ    │ 2. 10-Band PEQ    │ 2. 10-Band PEQ    │ 2. 10-Band PEQ    │
│ 3. Dynamics/Comp  │ 3. Gate + Comp    │ 3. Dynamics/Comp  │ 3. Dynamics/Comp  │
│ 4. Post-Gain/Mute │ 4. Post-Gain/Mute │ 4. Post-Gain/Mute │ 4. Post-Gain/Mute │
└─────────┬─────────┴─────────┬─────────┴─────────┬─────────┴─────────┬─────────┘
          └─────────► [ChatMix Crossfader] ◄──────┘                   │
                              │                                       │
                              └───────────────┬───────────────────────┘
                                              ▼
                                 [Master Summing & Soft Limiter]
                                              │
                                              ▼
                       [Master Clock: Physical Headphones WASAPI Render]

[Physical Mic WASAPI Capture] ──► [MIC BUS: Gate -> 10-Band PEQ -> Comp -> Gain] ──► [Signed Virtual Mic Render]
```

### 1.1 The "Silent Dummy Sink" Pattern
To prevent double-audio (hearing raw Windows app audio on top of DSP-processed audio) without writing a kernel driver:
1. Detect an existing WHQL-signed virtual playback endpoint (e.g., `VB-Audio Virtual Cable` or `Steam Streaming Speakers`).
2. Route managed application PIDs to this dummy sink via `IAudioPolicyConfig::SetPersistedDefaultAudioEndpoint`. Nobody listens to the dummy sink's capture side; it acts strictly as a mute sink.
3. Capture each managed application's pre-mix audio independently by PID from `audiodg.exe` using `ActivateAudioInterfaceAsync` (`AUDIOCLIENT_PROCESS_LOOPBACK_PARAMS`).

## 2. RUST WIN32 / COM INTEROP SPECIFICATIONS (`windows` crate)

### 2.1 Per-App Endpoint Routing (`IAudioPolicyConfig`)
Undocumented Windows COM interface used by Windows 11 Volume Mixer (`ms-settings:apps-volume`) to redirect a `ProcessId` to a target `IMMDevice` endpoint.

* **Activation:** Call `RoGetActivationFactory(h!("Windows.Media.Internal.AudioPolicyConfig"), &IID_IAUDIO_POLICY_CONFIG_FACTORY)` from `Windows::Win32::System::WinRT`.
* **Interface IID (Win10 20H1+ / Win11):** `ab3d4648-e242-459f-b02f-541c70306324`
* **Endpoint ID HSTRING Formatting:** Convert `IMMDevice::GetId()` (`{0.0.0.00000000}.{guid}`) before passing:
  * **Render (Playback):** `\\?\SWD#MMDEVAPI#{MMDeviceId}#{e6327cad-dcec-4949-ae8a-991e976a79d2}`
  * **Capture (Recording):** `\\?\SWD#MMDEVAPI#{MMDeviceId}#{2eef81be-33fa-4800-9670-1cd474972c3f}`

```rust
use windows::core::{interface, GUID, HRESULT, HSTRING};
use windows::Win32::Media::Audio::{EDataFlow, ERole};

// IAudioPolicyConfig derives from IInspectable (6 base vtable slots: IUnknown(3) + IInspectable(3))
// SetPersistedDefaultAudioEndpoint sits at slot index 25 (19 dummy slots after IInspectable)
#[interface("ab3d4648-e242-459f-b02f-541c70306324")]
pub unsafe trait IAudioPolicyConfig: windows::core::IInspectable {
    // Slots 6..=24: Reserved vtable padding (19 usize function pointers)
    fn _pad6(&self) -> HRESULT; fn _pad7(&self) -> HRESULT; fn _pad8(&self) -> HRESULT;
    fn _pad9(&self) -> HRESULT; fn _pad10(&self) -> HRESULT; fn _pad11(&self) -> HRESULT;
    fn _pad12(&self) -> HRESULT; fn _pad13(&self) -> HRESULT; fn _pad14(&self) -> HRESULT;
    fn _pad15(&self) -> HRESULT; fn _pad16(&self) -> HRESULT; fn _pad17(&self) -> HRESULT;
    fn _pad18(&self) -> HRESULT; fn _pad19(&self) -> HRESULT; fn _pad20(&self) -> HRESULT;
    fn _pad21(&self) -> HRESULT; fn _pad22(&self) -> HRESULT; fn _pad23(&self) -> HRESULT;
    fn _pad24(&self) -> HRESULT;

    // Slot 25: Redirects target PID to endpoint (pass HSTRING::new() to reset to system default)
    pub unsafe fn SetPersistedDefaultAudioEndpoint(
        &self,
        process_id: u32,
        flow: EDataFlow,
        role: ERole, // Call for eConsole, eMultimedia, and eCommunications
        device_id: *const HSTRING,
    ) -> HRESULT;

    // Slot 26: Reads current endpoint for PID
    pub unsafe fn GetPersistedDefaultAudioEndpoint(
        &self,
        process_id: u32,
        flow: EDataFlow,
        role: ERole,
        device_id: *mut HSTRING,
    ) -> HRESULT;

    // Slot 27: Clears all persisted routing rules
    pub unsafe fn ClearAllPersistedApplicationDefaultEndpoints(&self) -> HRESULT;
}
```

### 2.2 Per-Process WASAPI Loopback (`AUDIOCLIENT_PROCESS_LOOPBACK_PARAMS`)
Captures audio solely from a specific `ProcessId` tree via the Windows Audio Engine without opening a handle to the target process.

* **Device String Constant:** `w!("VAD\\Process_Loopback")`
* **API Call:** `ActivateAudioInterfaceAsync(w!("VAD\\Process_Loopback"), &IAudioClient::IID, Some(&prop_variant), &completion_handler)`

```rust
#[repr(C)]
pub struct AudioClientProcessLoopbackParams {
    pub target_process_id: u32,
    pub process_loopback_mode: u32, // 0 = INCLUDE_TARGET_PROCESS_TREE, 1 = EXCLUDE
}

#[repr(C)]
pub struct AudioClientActivationParams {
    pub activation_type: u32, // 1 = AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK
    pub process_loopback_params: AudioClientProcessLoopbackParams,
}
// Wrap AudioClientActivationParams in PROPVARIANT with vt = VT_BLOB (65),
// blob.cbSize = size_of::<AudioClientActivationParams>() as u32,
// blob.pBlobData = &mut params as *mut _ as *mut u8.
```
* Initialize the activated `IAudioClient` with `AUDCLNT_SHAREMODE_SHARED`, `AUDCLNT_STREAMFLAGS_LOOPBACK | AUDCLNT_STREAMFLAGS_EVENTCALLBACK | AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM`, and a $48000\text{ Hz}$ Float32 Stereo `WAVEFORMATEX`.

## 3. REAL-TIME DSP & MIXING ENGINE

### 3.1 Master Clock & Drift Compensation
* **Master Clock:** The physical headphone output `IAudioClient` render event (`AUDCLNT_STREAMFLAGS_EVENTCALLBACK`) drives the entire mixing graph every $10\text{ ms}$.
* **SPSC Ring Buffer:** Each captured PID stream pushes interleaved stereo `f32` frames into an `rtrb::RingBuffer<f32>` (capacity: $16384\text{ samples} = 8192\text{ stereo frames}$).
* **PI Adaptive Resampler:** Virtual loopback clocks and hardware DAC crystals drift slightly. Track target ring buffer fill level ($L_{\text{target}} = 960\text{ frames} = 20\text{ ms}$).
  * Compute error: $e = L_{\text{current}} - L_{\text{target}}$.
  * Compute resampling ratio: $r = 1.0 + \text{clamp}(K_p \cdot e + K_i \cdot \sum e, -0.002, +0.002)$ where $K_p = 10^{-6}, K_i = 10^{-9}$.
  * Read from `rtrb::Consumer<f32>` using 4-point Hermite interpolation at fractional step size $r$.

### 3.2 10-Band Cascaded Parametric EQ (Per Bus)
Each of the 5 buses (`Game`, `Chat`, `Media`, `Aux`, `Mic`) maintains 10 cascaded 2nd-order Biquad filters.

#### State & Parameters per Band $k \in [0, 9]$:
* `enabled`: `bool`, `filter_type`: `Peaking | LowShelf | HighShelf | LowPass | HighPass`
* $f_0 \in [20.0, 20000.0]\text{ Hz}$, $G_{\text{dB}} \in [-12.0, +12.0]\text{ dB}$, $Q \in [0.1, 10.0]$
* Normalized `f32` coefficients: $b_0, b_1, b_2, a_1, a_2$
* Delay state per channel ($c \in \{0, 1\}$): $z_{1,c}, z_{2,c}$

#### RBJ Audio EQ Cookbook Formulas ($f_s = 48000.0\text{ Hz}$):
Let $A = 10^{G_{\text{dB}} / 40}$, $\omega_0 = 2\pi \frac{f_0}{f_s}$, and $\alpha = \frac{\sin(\omega_0)}{2Q}$.

1. **Peaking EQ:**
   $$B_0 = 1 + \alpha A, \quad B_1 = -2\cos(\omega_0), \quad B_2 = 1 - \alpha A$$
   $$A_0 = 1 + \frac{\alpha}{A}, \quad A_1 = -2\cos(\omega_0), \quad A_2 = 1 - \frac{\alpha}{A}$$

2. **Low Shelf:**
   $$B_0 = A\left((A+1) - (A-1)\cos(\omega_0) + 2\sqrt{A}\alpha\right)$$
   $$B_1 = 2A\left((A-1) - (A+1)\cos(\omega_0)\right)$$
   $$B_2 = A\left((A+1) - (A-1)\cos(\omega_0) - 2\sqrt{A}\alpha\right)$$
   $$A_0 = (A+1) + (A-1)\cos(\omega_0) + 2\sqrt{A}\alpha$$
   $$A_1 = -2\left((A-1) + (A+1)\cos(\omega_0)\right)$$
   $$A_2 = (A+1) + (A-1)\cos(\omega_0) - 2\sqrt{A}\alpha$$

3. **High Shelf:**
   $$B_0 = A\left((A+1) + (A-1)\cos(\omega_0) + 2\sqrt{A}\alpha\right)$$
   $$B_1 = -2A\left((A-1) + (A+1)\cos(\omega_0)\right)$$
   $$B_2 = A\left((A+1) + (A-1)\cos(\omega_0) - 2\sqrt{A}\alpha\right)$$
   $$A_0 = (A+1) - (A-1)\cos(\omega_0) + 2\sqrt{A}\alpha$$
   $$A_1 = 2\left((A-1) - (A+1)\cos(\omega_0)\right)$$
   $$A_2 = (A+1) - (A-1)\cos(\omega_0) - 2\sqrt{A}\alpha$$

4. **Low Pass:**
   $$B_0 = \frac{1 - \cos(\omega_0)}{2}, \quad B_1 = 1 - \cos(\omega_0), \quad B_2 = \frac{1 - \cos(\omega_0)}{2}$$
   $$A_0 = 1 + \alpha, \quad A_1 = -2\cos(\omega_0), \quad A_2 = 1 - \alpha$$

5. **High Pass:**
   $$B_0 = \frac{1 + \cos(\omega_0)}{2}, \quad B_1 = -(1 + \cos(\omega_0)), \quad B_2 = \frac{1 + \cos(\omega_0)}{2}$$
   $$A_0 = 1 + \alpha, \quad A_1 = -2\cos(\omega_0), \quad A_2 = 1 - \alpha$$

Normalize all coefficients by dividing by $A_0$:
$$b_0 = \frac{B_0}{A_0}, \quad b_1 = \frac{B_1}{A_0}, \quad b_2 = \frac{B_2}{A_0}, \quad a_1 = \frac{A_1}{A_0}, \quad a_2 = \frac{A_2}{A_0}$$

#### Direct Form II Transposed Difference Equation (Per Sample $x$):
$$y = b_0 x + z_1$$
$$z_1 = b_1 x - a_1 y + z_2$$
$$z_2 = b_2 x - a_2 y$$

*Zipper-Noise Prevention:* When Tauri IPC updates EQ parameters, push a `CoeffsUpdate` struct into a lock-free command queue and linearly ramp $(b_0, b_1, b_2, a_1, a_2)$ over $128\text{ frames}$ on the audio thread.

#### UI Frequency Response Curve Calculation (For EQ Graph Rendering):
To draw the composite dB magnitude curve at any screen frequency $f$:
$$\omega = 2\pi \frac{f}{f_s}$$
$$H_k(\omega) = 10 \log_{10} \left( \frac{\left(b_0 + b_1\cos(\omega) + b_2\cos(2\omega)\right)^2 + \left(b_1\sin(\omega) + b_2\sin(2\omega)\right)^2}{\left(1 + a_1\cos(\omega) + a_2\cos(2\omega)\right)^2 + \left(a_1\sin(\omega) + a_2\sin(2\omega)\right)^2} \right)$$
$$\text{Total}_{\text{dB}}(f) = \sum_{k=0}^{9} H_k(\omega)$$

### 3.3 Dynamics Stage (Noise Gate & Compressor)
* **Envelope Follower:** Peak/RMS detector with attack coefficient $\alpha_A = e^{-1 / (t_{\text{attack}} \cdot f_s)}$ and release coefficient $\alpha_R = e^{-1 / (t_{\text{release}} \cdot f_s)}$.
* **Noise Gate (Chat & Mic Buses):** Attenuate signal by ratio when envelope drops below threshold $T_{\text{gate}} \in [-80\text{ dB}, 0\text{ dB}]$.
* **Smart Volume / Compressor:** Soft-knee downward compressor with threshold $T_{\text{comp}}$, ratio $R \in [1.0, 20.0]$, and makeup gain.

### 3.4 ChatMix Crossfader & Master Summing
Given ChatMix dial value $m \in [-1.0, +1.0]$ ($-1.0 = \text{Full Game}$, $0.0 = \text{Center}$, $+1.0 = \text{Full Chat}$):

$$G_{\text{Game}}(m) = \begin{cases} 1.0 & \text{if } m \le 0.0 \\ (1.0 - m)^2 & \text{if } m \text{ is greater than } 0.0 \end{cases}$$

$$G_{\text{Chat}}(m) = \begin{cases} (1.0 + m)^2 & \text{if } m \text{ is less than } 0.0 \\ 1.0 & \text{if } m \ge 0.0 \end{cases}$$

#### Final Master Bus Sample Equation:
$$S_{\text{out}}[n] = G_{\text{Master}} \cdot \left( G_{\text{Game}}(m) \cdot B_{\text{Game}}[n] + G_{\text{Chat}}(m) \cdot B_{\text{Chat}}[n] + B_{\text{Media}}[n] + B_{\text{Aux}}[n] \right)$$

Apply a fast soft-clipper ($\tanh$ or $-0.1\text{ dBFS}$ brickwall limiter) before writing $S_{\text{out}}[n]$ to the physical WASAPI render buffer.

## 4. RUST + TAURI v2 WORKSPACE STRUCTURE

```
sonar-rs/
 ├── src-tauri/
 │    ├── Cargo.toml               # windows = "0.58", rtrb = "0.3", tauri = "2.0", serde = "1.0"
 │    └── src/
 │         ├── main.rs             # Tauri builder, System Tray lifecycle (destroy WebView on close)
 │         ├── commands.rs         # #[tauri::command] handlers + Tauri Channel for 60Hz Peak Meters
 │         ├── audio/
 │         │    ├── engine.rs      # Master WASAPI render loop, MMCSS thread priority, bus summing
 │         │    ├── resampler.rs   # 4-point Hermite fractional resampler + PI drift controller
 │         │    └── ring_stream.rs # Per-PID rtrb::RingBuffer<f32> wrapper
 │         ├── interop/
 │         │    ├── policy.rs      # IAudioPolicyConfig COM vtable + HSTRING device ID formatter
 │         │    ├── loopback.rs    # ActivateAudioInterfaceAsync + IActivateAudioInterfaceCompletionHandler
 │         │    └── sessions.rs    # IAudioSessionManager2 scanner (uses PROCESS_QUERY_LIMITED_INFORMATION)
 │         ├── dsp/
 │         │    ├── biquad.rs      # DF-II Transposed filter + RBJ coefficient calculator
 │         │    ├── eq_bank.rs     # 10-band cascade + 128-sample lock-free parameter ramp
 │         │    ├── dynamics.rs    # Envelope follower, Noise Gate, Compressor
 │         │    └── meter.rs       # Lock-free AtomicU32 (f32::to_bits) RMS/Peak tracker
 │         └── state/
 │              └── config.rs      # JSON persistence for bus presets, EQ curves, PID routing rules
 └── src/                          # Lightweight Frontend (Svelte 5 / Vue 3 / SolidJS + Canvas EQ)
      ├── components/
      │    ├── MixerView.svelte    # 5 Bus strips, ChatMix dial, drag-and-drop PID routing cards
      │    └── EqCanvas.svelte     # 60fps interactive 10-node Biquad spline visualizer
      └── main.ts
```

### Configuration Schema (`config.json`):
```json
{
  "masterOutputDeviceId": "{0.0.0.00000000}.{guid}",
  "dummySinkDeviceId": "{0.0.0.00000000}.{guid}",
  "physicalMicDeviceId": "{0.0.1.00000000}.{guid}",
  "virtualMicOutputDeviceId": "{0.0.0.00000000}.{guid}",
  "chatMixBalance": 0.0,
  "processRoutingRules": {
    "cs2.exe": "Game",
    "discord.exe": "Chat",
    "spotify.exe": "Media"
  },
  "buses": {
    "Game": {
      "volume": 1.0,
      "muted": false,
      "compressor": { "enabled": false, "thresholdDb": -18.0, "ratio": 3.0 },
      "eqBands": [
        { "enabled": true, "type": "LowShelf", "freq": 60.0, "gainDb": 2.5, "q": 0.707 }
      ]
    }
  }
}
```

## 5. PHASED EXECUTION PLAN FOR AGENT

### Phase 1: Rust COM Interop & Process Capture Foundation
1. Configure `Cargo.toml` with `windows` features (`Win32_Media_Audio`, `Win32_System_Com`, `Win32_System_WinRT`, `Win32_System_Threading`) and `rtrb`.
2. Implement `IMMDeviceEnumerator` wrapper to list active Render/Capture endpoints and locate the silent dummy sink (`VB-Cable` / `Steam Streaming Speakers`).
3. Implement `interop::policy` (`IAudioPolicyConfig`) and verify redirecting a target PID's audio to the dummy sink.
4. Implement `interop::loopback` (`ActivateAudioInterfaceAsync` with custom `#[implement(IActivateAudioInterfaceCompletionHandler)]`) and verify capturing raw `f32` PCM from a target PID into an `rtrb::RingBuffer<f32>`.

### Phase 2: Master Clock & Multi-Bus Mixing Engine
1. Implement `audio::resampler` (4-point Hermite + PI drift controller).
2. Build `audio::engine` driven by a $10\text{ ms}$ event-driven WASAPI Shared render client on the physical headphones with MMCSS `"Pro Audio"` thread priority.
3. Sum captured PID streams into `Game`, `Chat`, `Media`, and `Aux` bus buffers, apply the `ChatMix` crossfader math, and verify zero-underrun playback.

### Phase 3: Real-Time DSP Pipeline (EQ & Dynamics)
1. Implement `dsp::biquad` and `dsp::eq_bank` (10 bands per bus) with 128-sample coefficient smoothing.
2. Implement `dsp::dynamics` (Noise Gate + Compressor) and lock-free `AtomicU32` `dsp::meter` readouts.
3. Wire up the `Mic` bus (`Physical Mic Capture -> Gate -> 10-Band PEQ -> Compressor -> Virtual Mic Render`).

### Phase 4: Tauri v2 UI, System Tray Optimization & Auto-Router
1. Build the Mixer UI and interactive 10-band Parametric EQ spline canvas using $H_k(\omega)$ magnitude evaluation.
2. Stream real-time VU meter peaks over a Tauri v2 `tauri::ipc::Channel`.
3. Configure Tauri System Tray behavior to destroy the WebView2 window on close while keeping the Rust audio engine running in the background at less than $15\text{ MB}$ RAM.