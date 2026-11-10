use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

pub const DEFAULT_EQ_FREQS: [f32; 10] = [
    31.25, 62.5, 125.0, 250.0, 500.0, 1000.0, 2000.0, 4000.0, 8000.0, 16000.0,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum BusId {
    Game,
    Chat,
    Media,
    Aux,
    Mic,
}

impl BusId {
    pub const ALL: [BusId; 5] = [
        BusId::Game,
        BusId::Chat,
        BusId::Media,
        BusId::Aux,
        BusId::Mic,
    ];

    pub const PLAYBACK_BUSES: [BusId; 4] = [
        BusId::Game,
        BusId::Chat,
        BusId::Media,
        BusId::Aux,
    ];

    #[inline]
    pub const fn index(self) -> usize {
        match self {
            BusId::Game => 0,
            BusId::Chat => 1,
            BusId::Media => 2,
            BusId::Aux => 3,
            BusId::Mic => 4,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            BusId::Game => "Game",
            BusId::Chat => "Chat",
            BusId::Media => "Media",
            BusId::Aux => "Aux",
            BusId::Mic => "Mic",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FilterType {
    Peaking,
    LowShelf,
    HighShelf,
    LowPass,
    HighPass,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct EqBandConfig {
    pub enabled: bool,
    #[serde(rename = "type")]
    pub filter_type: FilterType,
    pub freq: f32,
    #[serde(rename = "gainDb")]
    pub gain_db: f32,
    pub q: f32,
}

impl EqBandConfig {
    pub fn default_for_band(band_index: usize) -> Self {
        let idx = band_index.min(9);
        let filter_type = match idx {
            0 => FilterType::LowShelf,
            9 => FilterType::HighShelf,
            _ => FilterType::Peaking,
        };
        let q = match idx {
            0 | 9 => 0.707,
            _ => 1.414,
        };
        Self {
            enabled: true,
            filter_type,
            freq: DEFAULT_EQ_FREQS[idx],
            gain_db: 0.0,
            q,
        }
    }

    pub fn clamped(self) -> Self {
        Self {
            enabled: self.enabled,
            filter_type: self.filter_type,
            freq: self.freq.clamp(20.0, 20000.0),
            gain_db: self.gain_db.clamp(-12.0, 12.0),
            q: self.q.clamp(0.1, 10.0),
        }
    }
}

fn default_attack_ms() -> f32 {
    10.0
}
fn default_release_ms() -> f32 {
    120.0
}
fn default_knee_db() -> f32 {
    6.0
}
fn default_gate_attack_ms() -> f32 {
    2.0
}
fn default_gate_release_ms() -> f32 {
    80.0
}
fn default_gate_ratio() -> f32 {
    4.0
}
fn default_one() -> f32 {
    1.0
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CompressorConfig {
    pub enabled: bool,
    #[serde(rename = "thresholdDb")]
    pub threshold_db: f32,
    pub ratio: f32,
    #[serde(rename = "attackMs", default = "default_attack_ms")]
    pub attack_ms: f32,
    #[serde(rename = "releaseMs", default = "default_release_ms")]
    pub release_ms: f32,
    #[serde(rename = "makeupGainDb", default)]
    pub makeup_gain_db: f32,
    #[serde(rename = "kneeDb", default = "default_knee_db")]
    pub knee_db: f32,
}

impl Default for CompressorConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            threshold_db: -18.0,
            ratio: 3.0,
            attack_ms: 10.0,
            release_ms: 120.0,
            makeup_gain_db: 0.0,
            knee_db: 6.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct NoiseGateConfig {
    pub enabled: bool,
    #[serde(rename = "thresholdDb")]
    pub threshold_db: f32,
    #[serde(default = "default_gate_ratio")]
    pub ratio: f32,
    #[serde(rename = "attackMs", default = "default_gate_attack_ms")]
    pub attack_ms: f32,
    #[serde(rename = "releaseMs", default = "default_gate_release_ms")]
    pub release_ms: f32,
}

impl Default for NoiseGateConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            threshold_db: -48.0,
            ratio: 4.0,
            attack_ms: 2.0,
            release_ms: 80.0,
        }
    }
}

/// Peak limiter with instant attack and smoothed release; guarantees the bus never exceeds
/// `ceiling_db` (dBFS).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct LimiterConfig {
    pub enabled: bool,
    pub ceiling_db: f32,
    pub release_ms: f32,
}

impl Default for LimiterConfig {
    fn default() -> Self {
        Self { enabled: false, ceiling_db: -1.0, release_ms: 80.0 }
    }
}

impl LimiterConfig {
    pub fn normalized(mut self) -> Self {
        self.ceiling_db = if self.ceiling_db.is_finite() { self.ceiling_db.clamp(-12.0, 0.0) } else { -1.0 };
        self.release_ms = if self.release_ms.is_finite() { self.release_ms.clamp(10.0, 1000.0) } else { 80.0 };
        self
    }
}

/// Split-band de-esser: attenuates only the sibilance band above `frequency` when it exceeds
/// `threshold_db`, by at most `max_reduction_db`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DeEsserConfig {
    pub enabled: bool,
    pub frequency: f32,
    pub threshold_db: f32,
    pub max_reduction_db: f32,
}

impl Default for DeEsserConfig {
    fn default() -> Self {
        Self { enabled: false, frequency: 6000.0, threshold_db: -30.0, max_reduction_db: 6.0 }
    }
}

impl DeEsserConfig {
    pub fn normalized(mut self) -> Self {
        self.frequency = if self.frequency.is_finite() { self.frequency.clamp(3000.0, 10000.0) } else { 6000.0 };
        self.threshold_db = if self.threshold_db.is_finite() { self.threshold_db.clamp(-60.0, 0.0) } else { -30.0 };
        self.max_reduction_db = if self.max_reduction_db.is_finite() { self.max_reduction_db.clamp(0.0, 18.0) } else { 6.0 };
        self
    }
}

/// Which neural network cleans the voice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NoiseEngine {
    /// RNNoise: tiny, 10 ms delay, light on the CPU.
    #[default]
    Fast,
    /// DeepFilterNet3: cleaner result, about 45 ms delay and more CPU.
    High,
}

/// Voice noise suppression. `strength` blends the cleaned signal over the dry one.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct NoiseSuppressionConfig {
    pub enabled: bool,
    pub strength: f32,
    pub engine: NoiseEngine,
}

impl Default for NoiseSuppressionConfig {
    fn default() -> Self {
        Self { enabled: false, strength: 1.0, engine: NoiseEngine::Fast }
    }
}

impl NoiseSuppressionConfig {
    pub fn normalized(mut self) -> Self {
        self.strength = if self.strength.is_finite() { self.strength.clamp(0.0, 1.0) } else { 1.0 };
        self
    }
}

/// Automatically lowers `targets` while any of the `triggers` is louder than `threshold_db`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DuckingConfig {
    pub enabled: bool,
    pub triggers: Vec<BusId>,
    pub targets: Vec<BusId>,
    pub amount_db: f32,
    pub threshold_db: f32,
    pub attack_ms: f32,
    pub release_ms: f32,
}

impl Default for DuckingConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            triggers: vec![BusId::Chat],
            targets: vec![BusId::Game, BusId::Media],
            amount_db: -12.0,
            threshold_db: -42.0,
            attack_ms: 30.0,
            release_ms: 600.0,
        }
    }
}

impl DuckingConfig {
    pub fn normalized(mut self) -> Self {
        let finite = |v: f32, lo: f32, hi: f32, fallback: f32| if v.is_finite() { v.clamp(lo, hi) } else { fallback };
        self.amount_db = finite(self.amount_db, -30.0, 0.0, -12.0);
        self.threshold_db = finite(self.threshold_db, -70.0, -10.0, -42.0);
        self.attack_ms = finite(self.attack_ms, 5.0, 500.0, 30.0);
        self.release_ms = finite(self.release_ms, 50.0, 3000.0, 600.0);
        self.triggers.sort();
        self.triggers.dedup();
        self.targets.retain(|b| *b != BusId::Mic);
        self.targets.sort();
        self.targets.dedup();
        let triggers = self.triggers.clone();
        self.targets.retain(|b| !triggers.contains(b));
        self
    }

    pub fn trigger_mask(&self) -> u32 {
        self.triggers.iter().fold(0, |m, b| m | (1 << b.index()))
    }

    pub fn target_mask(&self) -> u32 {
        self.targets.iter().fold(0, |m, b| m | (1 << b.index()))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BusConfig {
    #[serde(default)]
    pub limiter: LimiterConfig,
    #[serde(rename = "deEsser", default)]
    pub de_esser: DeEsserConfig,
    #[serde(rename = "noiseSuppression", default)]
    pub noise_suppression: NoiseSuppressionConfig,
    #[serde(rename = "preGainDb", default)]
    pub pre_gain_db: f32,
    #[serde(default = "default_one")]
    pub volume: f32,
    #[serde(default)]
    pub muted: bool,
    #[serde(rename = "noiseGate", default)]
    pub noise_gate: NoiseGateConfig,
    #[serde(default)]
    pub compressor: CompressorConfig,
    #[serde(rename = "eqBands")]
    pub eq_bands: Vec<EqBandConfig>,
}

impl BusConfig {
    pub fn default_for_bus(bus: BusId) -> Self {
        let mut eq_bands: Vec<EqBandConfig> = (0..10).map(EqBandConfig::default_for_band).collect();
        let mut noise_gate = NoiseGateConfig::default();
        let mut compressor = CompressorConfig::default();

        match bus {
            BusId::Game => {
                // Subtle low-end punch & footsteps clarity preset
                eq_bands[1].gain_db = 2.0;
                eq_bands[6].gain_db = 1.5;
            }
            BusId::Chat => {
                noise_gate.enabled = true;
                noise_gate.threshold_db = -50.0;
                compressor.enabled = true;
                compressor.threshold_db = -20.0;
                compressor.ratio = 3.0;
                eq_bands[0].filter_type = FilterType::HighPass;
                eq_bands[0].freq = 85.0;
            }
            BusId::Mic => {
                noise_gate.enabled = true;
                noise_gate.threshold_db = -46.0;
                compressor.enabled = true;
                compressor.threshold_db = -18.0;
                compressor.ratio = 3.5;
                eq_bands[0].filter_type = FilterType::HighPass;
                eq_bands[0].freq = 80.0;
            }
            BusId::Media | BusId::Aux => {}
        }

        Self {
            limiter: LimiterConfig::default(),
            de_esser: DeEsserConfig::default(),
            noise_suppression: NoiseSuppressionConfig::default(),
            pre_gain_db: 0.0,
            volume: 1.0,
            muted: false,
            noise_gate,
            compressor,
            eq_bands,
        }
    }

    pub fn normalized(mut self) -> Self {
        self.limiter = self.limiter.normalized();
        self.de_esser = self.de_esser.normalized();
        self.noise_suppression = self.noise_suppression.normalized();
        while self.eq_bands.len() < 10 {
            let idx = self.eq_bands.len();
            self.eq_bands.push(EqBandConfig::default_for_band(idx));
        }
        self.eq_bands.truncate(10);
        for band in &mut self.eq_bands {
            *band = band.clamped();
        }
        self.volume = self.volume.clamp(0.0, 2.0);
        self.pre_gain_db = self.pre_gain_db.clamp(-24.0, 24.0);
        self.compressor.ratio = self.compressor.ratio.clamp(1.0, 20.0);
        self.compressor.threshold_db = self.compressor.threshold_db.clamp(-60.0, 0.0);
        self.noise_gate.threshold_db = self.noise_gate.threshold_db.clamp(-80.0, 0.0);
        self.noise_gate.ratio = self.noise_gate.ratio.clamp(1.0, 20.0);
        self
    }
}

/// Per-channel send into the recording mix, independent of playback controls.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct StreamChannelConfig {
    pub volume: f32,
    pub muted: bool,
}

impl Default for StreamChannelConfig {
    fn default() -> Self { Self { volume: 1.0, muted: false } }
}

impl StreamChannelConfig {
    pub fn normalized(self) -> Self {
        Self { volume: if self.volume.is_finite() { self.volume.clamp(0.0, 1.5) } else { 1.0 }, ..self }
    }
    pub fn gain(self) -> f32 { if self.muted { 0.0 } else { self.volume } }
}

/// Which point of the mic chain feeds the stream mix.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StreamMicSource {
    /// Exactly what the virtual mic carries: after the fader, limiter and L/R levels.
    #[default]
    Virtual,
    /// Processed mic tapped before the fader (ignores the virtual-mic level controls).
    Raw,
}

/// Independent pre-fader recording mix. An app override wins over category membership.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct StreamConfig {
    pub enabled: bool,
    pub output_device_id: Option<String>,
    pub volume: f32,
    pub muted: bool,
    pub categories: Vec<BusId>,
    pub app_overrides: BTreeMap<String, bool>,
    pub channels: BTreeMap<BusId, StreamChannelConfig>,
    pub mic_source: StreamMicSource,
}

impl Default for StreamConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            output_device_id: None,
            volume: 1.0,
            muted: false,
            categories: vec![BusId::Game, BusId::Chat, BusId::Mic],
            app_overrides: BTreeMap::new(),
            channels: BusId::ALL.into_iter().map(|bus| (bus, StreamChannelConfig::default())).collect(),
            mic_source: StreamMicSource::default(),
        }
    }
}

impl StreamConfig {
    pub fn includes_app(&self, exe: &str, bus: Option<BusId>) -> bool {
        self.app_overrides.get(&exe.to_ascii_lowercase()).copied()
            .unwrap_or_else(|| bus.is_some_and(|b| self.categories.contains(&b)))
    }

    pub fn normalized(mut self) -> Self {
        self.volume = if self.volume.is_finite() { self.volume.clamp(0.0, 1.5) } else { 1.0 };
        for bus in BusId::ALL {
            let channel = self.channels.entry(bus).or_default();
            *channel = channel.normalized();
        }
        self.output_device_id = self.output_device_id.filter(|id| !id.trim().is_empty());
        self.categories.sort();
        self.categories.dedup();
        self.app_overrides = self.app_overrides.into_iter()
            .filter(|(name, _)| !name.trim().is_empty())
            .map(|(name, included)| (name.to_ascii_lowercase(), included)).collect();
        self
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RouterConfig {
    #[serde(default)]
    pub stream: StreamConfig,
    #[serde(rename = "masterOutputDeviceId")]
    pub master_output_device_id: Option<String>,
    #[serde(rename = "dummySinkDeviceId")]
    pub dummy_sink_device_id: Option<String>,
    #[serde(rename = "physicalMicDeviceId")]
    pub physical_mic_device_id: Option<String>,
    #[serde(rename = "virtualMicOutputDeviceId")]
    pub virtual_mic_output_device_id: Option<String>,
    #[serde(rename = "chatMixBalance")]
    pub chat_mix_balance: f32,
    #[serde(rename = "masterVolume", default = "default_one")]
    pub master_volume: f32,
    #[serde(rename = "micToMonitor", default)]
    pub mic_to_monitor: bool,
    #[serde(rename = "processRoutingRules")]
    pub process_routing_rules: BTreeMap<String, BusId>,
    /// Per-app volume (0.0–2.0, lower-cased exe name). Missing entries mean unity gain.
    #[serde(rename = "processVolumes", default)]
    pub process_volumes: BTreeMap<String, f32>,
    #[serde(default)]
    pub ducking: DuckingConfig,
    pub buses: BTreeMap<BusId, BusConfig>,
}

impl Default for RouterConfig {
    fn default() -> Self {
        let mut process_routing_rules = BTreeMap::new();
        process_routing_rules.insert("cs2.exe".to_string(), BusId::Game);
        process_routing_rules.insert("valorant-win64-shipping.exe".to_string(), BusId::Game);
        process_routing_rules.insert("r5apex.exe".to_string(), BusId::Game);
        process_routing_rules.insert("discord.exe".to_string(), BusId::Chat);
        process_routing_rules.insert("teamspeak3.exe".to_string(), BusId::Chat);
        process_routing_rules.insert("spotify.exe".to_string(), BusId::Media);
        process_routing_rules.insert("chrome.exe".to_string(), BusId::Media);
        process_routing_rules.insert("firefox.exe".to_string(), BusId::Media);

        let mut buses = BTreeMap::new();
        for bus in BusId::ALL {
            buses.insert(bus, BusConfig::default_for_bus(bus));
        }

        Self {
            master_output_device_id: None,
            stream: StreamConfig::default(),
            dummy_sink_device_id: None,
            physical_mic_device_id: None,
            virtual_mic_output_device_id: None,
            chat_mix_balance: 0.0,
            master_volume: 1.0,
            mic_to_monitor: false,
            process_routing_rules,
            process_volumes: BTreeMap::new(),
            ducking: DuckingConfig::default(),
            buses,
        }
    }
}

impl RouterConfig {
    pub fn normalized(mut self) -> Self {
        self.stream = self.stream.normalized();
        self.chat_mix_balance = self.chat_mix_balance.clamp(-1.0, 1.0);
        self.master_volume = self.master_volume.clamp(0.0, 2.0);
        for bus in BusId::ALL {
            let entry = self
                .buses
                .remove(&bus)
                .unwrap_or_else(|| BusConfig::default_for_bus(bus))
                .normalized();
            self.buses.insert(bus, entry);
        }
        let normalized_rules = self
            .process_routing_rules
            .into_iter()
            .map(|(k, v)| (k.to_ascii_lowercase(), v))
            .collect();
        self.process_routing_rules = normalized_rules;
        self.ducking = self.ducking.normalized();
        self.process_volumes = self
            .process_volumes
            .into_iter()
            .filter(|(name, vol)| !name.trim().is_empty() && vol.is_finite())
            .map(|(name, vol)| (name.to_ascii_lowercase(), vol.clamp(0.0, 2.0)))
            .collect();
        self
    }

    pub fn load_from_path(path: &Path) -> Self {
        if let Ok(raw) = fs::read_to_string(path) {
            if let Ok(parsed) = serde_json::from_str::<RouterConfig>(&raw) {
                return parsed.normalized();
            }
        }
        Self::default()
    }

    pub fn save_to_path(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let serialized = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        // Write beside the target and rename, so a crash mid-write never leaves a half file.
        let tmp = path.with_extension("json.tmp");
        fs::write(&tmp, serialized).map_err(|e| e.to_string())?;
        fs::rename(&tmp, path).map_err(|e| {
            let _ = fs::remove_file(&tmp);
            e.to_string()
        })
    }

    pub fn default_config_path() -> PathBuf {
        if let Ok(appdata) = std::env::var("APPDATA") {
            PathBuf::from(appdata).join("StellarsSonar").join("config.json")
        } else {
            PathBuf::from("config.json")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_stream_settings_default_to_unity_sends_and_roundtrip() {
        let legacy: StreamConfig = serde_json::from_str(r#"{"enabled":true,"volume":0.8}"#).unwrap();
        for bus in BusId::ALL { assert_eq!(legacy.channels[&bus].gain(), 1.0); }
        let mut cfg = legacy;
        cfg.channels.insert(BusId::Game, StreamChannelConfig { volume: 0.25, muted: false });
        cfg.channels.insert(BusId::Mic, StreamChannelConfig { volume: 0.8, muted: true });
        let restored: StreamConfig = serde_json::from_str(&serde_json::to_string(&cfg).unwrap()).unwrap();
        assert_eq!(restored.channels[&BusId::Game].gain(), 0.25);
        assert_eq!(restored.channels[&BusId::Mic].gain(), 0.0);
        cfg.channels.clear();
        cfg.channels.insert(BusId::Game, StreamChannelConfig { volume: f32::NAN, muted: false });
        cfg.channels.insert(BusId::Chat, StreamChannelConfig { volume: 9.0, muted: false });
        cfg.channels.insert(BusId::Media, StreamChannelConfig { volume: -1.0, muted: false });
        let cfg = cfg.normalized();
        assert_eq!(cfg.channels.len(), 5);
        assert_eq!(cfg.channels[&BusId::Game].volume, 1.0);
        assert_eq!(cfg.channels[&BusId::Chat].volume, 1.5);
        assert_eq!(cfg.channels[&BusId::Media].volume, 0.0);
    }

    #[test]
    fn stream_app_overrides_win_over_category_membership() {
        let mut cfg = StreamConfig::default();
        assert!(cfg.includes_app("game.exe", Some(BusId::Game)));
        assert!(!cfg.includes_app("music.exe", Some(BusId::Media)));
        cfg.app_overrides.insert("game.exe".into(), false);
        cfg.app_overrides.insert("music.exe".into(), true);
        assert!(!cfg.includes_app("GAME.EXE", Some(BusId::Game)));
        assert!(cfg.includes_app("MUSIC.EXE", Some(BusId::Media)));
        assert!(cfg.includes_app("music.exe", None));
        assert!(!cfg.includes_app("unknown.exe", None));
    }

    #[test]
    fn stream_normalizes_and_roundtrips_saved_sources() {
        let cfg = StreamConfig { volume: f32::NAN, output_device_id: Some(" ".into()),
            categories: vec![BusId::Game, BusId::Game, BusId::Mic],
            app_overrides: BTreeMap::from([("DISCORD.EXE".into(), false)]),
            ..StreamConfig::default() }.normalized();
        assert_eq!(cfg.volume, 1.0);
        assert_eq!(cfg.output_device_id, None);
        assert_eq!(cfg.categories.len(), 2);
        assert_eq!(cfg.app_overrides.get("discord.exe"), Some(&false));
        let saved = serde_json::to_string(&cfg).unwrap();
        assert_eq!(serde_json::from_str::<StreamConfig>(&saved).unwrap(), cfg);
    }

    #[test]
    fn test_spec_config_schema_deserialization() {
        let raw_json = r#"{
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
        }"#;

        let parsed: RouterConfig =
            serde_json::from_str::<RouterConfig>(raw_json).expect("Spec JSON should deserialize").normalized();
        assert_eq!(parsed.buses.len(), 5);
        assert!(!parsed.stream.enabled, "Old configs must not start broadcasting");
        assert!(parsed.stream.output_device_id.is_none());
        assert_eq!(parsed.buses[&BusId::Game].eq_bands.len(), 10);
        assert_eq!(parsed.buses[&BusId::Game].eq_bands[0].freq, 60.0);
        assert_eq!(parsed.buses[&BusId::Game].eq_bands[0].gain_db, 2.5);
        assert_eq!(parsed.process_routing_rules["cs2.exe"], BusId::Game);
    }

    #[test]
    fn old_configs_with_virtual_mic_levels_still_load() {
        let mut value = serde_json::to_value(RouterConfig::default()).expect("serialize");
        value["micVirtualLeftVolume"] = serde_json::json!(0.5);
        value["micVirtualRightVolume"] = serde_json::json!(1.2);
        assert!(serde_json::from_value::<RouterConfig>(value).is_ok());
    }
}
