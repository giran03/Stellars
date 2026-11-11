use crate::interop::default_device::{restore_if_taken_over, DefaultDevices};
use crate::interop::sessions::{enumerate_audio_endpoints, AudioEndpointInfo};
use crate::interop::silent_sink::{spare_outputs, SpareOutput};
use crate::state::config::RouterConfig;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fs;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::Duration;
use windows::core::{GUID, PROPVARIANT, PWSTR};
use windows::Win32::Media::Audio::{
    eCapture, eRender, IMMDevice, IMMDeviceEnumerator, MMDeviceEnumerator, DEVICE_STATE_ACTIVE,
};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoTaskMemFree, CLSCTX_ALL, COINIT_APARTMENTTHREADED,
    STGM_READ, STGM_READWRITE,
};
use windows::Win32::UI::Shell::PropertiesSystem::{IPropertyStore, PROPERTYKEY};

/// Driver packages the cable setup downloads, as `(https url, pinned SHA-256)`. The elevated setup
/// refuses any file whose hash differs, so a tampered or swapped download is never unpacked. When
/// VB-Audio publishes a new package, update the URL and hash together.
pub const VBCABLE_DOWNLOADS: [(&str, &str); 2] = [
    (
        "https://download.vb-audio.com/Download_CABLE/VBCABLE_Driver_Pack46.zip",
        "BC44E8974191A5A2427D6C979E2DAB08DBB09925F3F435DFA2F0BA3E45B6EB90",
    ),
    (
        "https://download.vb-audio.com/Download_CABLE/VBCABLE_Driver_Pack45.zip",
        "B950E39F01AF1D04EA623C8F6D8EB9B6EA5C477C637295FABF20631C85116BFB",
    ),
];

pub const HIFICABLE_DOWNLOADS: [(&str, &str); 1] = [(
    "https://download.vb-audio.com/Download_CABLE/HiFiCableAsioBridgeSetup_v1007.zip",
    "3ECF204BFD8579D36BB918F9856EB1EADDD49C75146F5E8A8F59DCB8375AE89A",
)];

const VT_LPWSTR_VAL: u16 = 31;

/// Keeps the driver-setup PowerShell from flashing a console window.
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// `PKEY_Device_DeviceDesc` (`{a45c254e-df1c-4efd-8020-67d146a850e0}, 2`) — Endpoint pin name (e.g. `Stellars - Microphone`).
const PKEY_DEVICE_DESC: PROPERTYKEY = PROPERTYKEY {
    fmtid: GUID::from_u128(0xa45c254e_df1c_4efd_8020_67d146a850e0),
    pid: 2,
};

/// `PKEY_DeviceInterface_FriendlyName` (`{b3f8fa53-0004-438e-9003-51a46e139bfc}, 6`) — Adapter name (e.g. `Stellars Virtual Audio Cable`).
const PKEY_INTERFACE_FRIENDLY_NAME: PROPERTYKEY = PROPERTYKEY {
    fmtid: GUID::from_u128(0xb3f8fa53_0004_438e_9003_51a46e139bfc),
    pid: 6,
};

/// `DEVPKEY_Device_MatchingDeviceId` (`{a8b865dd-2e3d-4094-ad97-e593a70c75d6}, 8`) — Immutable PnP hardware match ID (`VBAudioVACWDM`, `VBAudioHFVAIO`).
const DEVPKEY_MATCHING_DEVICE_ID: PROPERTYKEY = PROPERTYKEY {
    fmtid: GUID::from_u128(0xa8b865dd_2e3d_4094_ad97_e593a70c75d6),
    pid: 8,
};

/// `PKEY_AudioEndpoint_FormFactor` / `KSNODETYPE` (`{1da5d803-d492-4edd-8c23-e0c0ffee7f0e}, 8`).
const PKEY_ENDPOINT_KSNODETYPE: PROPERTYKEY = PROPERTYKEY {
    fmtid: GUID::from_u128(0x1da5d803_d492_4edd_8c23_e0c0ffee7f0e),
    pid: 8,
};

#[repr(C)]
#[derive(Clone, Copy)]
struct RawPropVariantLpwstr {
    vt: u16,
    w_reserved1: u16,
    w_reserved2: u16,
    w_reserved3: u16,
    pwsz_val: *const u16,
    _pad: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VirtualCableEndpointPair {
    pub render_id: String,
    pub render_name: String,
    pub paired_capture_id: Option<String>,
    pub paired_capture_name: Option<String>,
    pub driver_kind: String,
    pub group_key: String,
    pub is_watermark_free: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RouteSlotStatus {
    pub slot: String,
    pub device_id: Option<String>,
    pub device_name: Option<String>,
    pub paired_capture_name: Option<String>,
    pub status: String, // "ready" | "unassigned" | "missing" | "conflict"
    pub hint: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VirtualCableStatusPayload {
    pub vb_cable_installed: bool,
    pub hifi_cable_installed: bool,
    pub steam_audio_available: bool,
    pub detected_cables: Vec<VirtualCableEndpointPair>,
    pub silent_sink: RouteSlotStatus,
    pub virtual_mic: RouteSlotStatus,
    pub stream_mix: RouteSlotStatus,
    pub all_routes_healthy: bool,
    /// Physical outputs nobody needs to hear that could be the silent sink instead of a cable.
    pub spare_outputs: Vec<SpareOutput>,
    /// True when the Stream Mix has no cable but one would be freed by moving the silent sink
    /// from its cable to one of `spare_outputs`.
    pub can_free_cable_for_stream: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VirtualCableSetupResult {
    pub config: RouterConfig,
    pub endpoints: Vec<AudioEndpointInfo>,
    pub cable_status: VirtualCableStatusPayload,
    pub actions_taken: Vec<String>,
    pub reboot_recommended: bool,
}

/// Classifies a virtual audio device by its friendly name and returns `(driver_kind, group_key, watermark_free)`.
/// `group_key` identifies endpoints that share the same underlying kernel loopback pipe (e.g., `CABLE Input` and
/// `CABLE In 16 Ch` both feed `CABLE Output`), preventing two Stellars slots from colliding on the same pipe.
pub fn classify_virtual_endpoint(name: &str) -> Option<(&'static str, String, bool)> {
    let lower = name.to_ascii_lowercase();

    if lower.contains("hi-fi cable")
        || lower.contains("stellars hi-fi")
        || lower.contains("hi-fi virtual cable")
        || lower.contains("stellars - stream mix")
    {
        return Some(("hifi-cable", "vbcable-hifi".to_string(), true));
    }
    if lower.contains("cable-a") || lower.contains("stellars cable-a") {
        return Some(("vb-cable-abcd", "vbcable-a".to_string(), true));
    }
    if lower.contains("cable-b") || lower.contains("stellars cable-b") {
        return Some(("vb-cable-abcd", "vbcable-b".to_string(), true));
    }
    if lower.contains("cable-c") || lower.contains("stellars cable-c") {
        return Some(("vb-cable-abcd", "vbcable-c".to_string(), true));
    }
    if lower.contains("cable-d") || lower.contains("stellars cable-d") {
        return Some(("vb-cable-abcd", "vbcable-d".to_string(), true));
    }
    if lower.contains("vb-audio virtual cable")
        || lower.contains("stellars virtual audio cable")
        || lower.contains("stellars - aux in")
        || lower.contains("stellars - virtual mic in")
        || lower.contains("stellars - microphone")
        || lower.starts_with("cable input")
        || lower.starts_with("cable output")
        || lower.starts_with("cable in 16")
    {
        return Some(("vb-cable", "vbcable-main".to_string(), true));
    }
    if lower.contains("steam streaming microphone") {
        return Some(("steam", "steam-mic".to_string(), true));
    }
    if lower.contains("steam streaming speakers") {
        return Some(("steam", "steam-speakers".to_string(), true));
    }
    if lower.contains("voicemeeter") || lower.contains("vaio") {
        let sub = if lower.contains("aux") {
            "voicemeeter-aux"
        } else {
            "voicemeeter-main"
        };
        return Some(("voicemeeter", sub.to_string(), true));
    }
    if lower.contains("oculus virtual audio device") {
        return Some(("oculus", "oculus-vad".to_string(), true));
    }
    if lower.contains("sonar") && lower.contains("virtual") {
        return Some(("sonar", format!("sonar:{lower}"), true));
    }
    if lower.contains("virtual audio cable") || lower.contains("virtual cable") {
        // Eugene Muzychenko's VAC ("Line 1 (Virtual Audio Cable)") may have a trial voice watermark
        // if unlicensed; safe for silent dummy sink, deprioritized for Mic/Stream.
        let is_eugene_vac = lower.contains("(virtual audio cable)");
        return Some((
            "other-virtual",
            format!("other:{lower}"),
            !is_eugene_vac,
        ));
    }

    None
}

pub fn is_virtual_audio_endpoint_name(name: &str) -> bool {
    classify_virtual_endpoint(name).is_some()
}

/// Finds the paired Capture (`Recording`) endpoint corresponding to a virtual Render (`Playback`) endpoint.
pub fn find_paired_capture_endpoint<'a>(
    render_ep: &AudioEndpointInfo,
    endpoints: &'a [AudioEndpointInfo],
) -> Option<&'a AudioEndpointInfo> {
    let (_, group_key, _) = classify_virtual_endpoint(&render_ep.name)?;
    let captures: Vec<&AudioEndpointInfo> = endpoints
        .iter()
        .filter(|e| e.flow == "Capture")
        .collect();

    captures.into_iter().find(|cap| {
        if let Some((_, cap_group, _)) = classify_virtual_endpoint(&cap.name) {
            if cap_group == group_key {
                return true;
            }
            // Steam Streaming Speakers can pair with Steam Streaming Microphone capture if needed
            if group_key == "steam-speakers" && cap_group == "steam-mic" {
                return true;
            }
        }
        false
    })
}

/// Lists all detected virtual Render endpoints and their paired Capture endpoints.
pub fn detect_virtual_cable_pairs(endpoints: &[AudioEndpointInfo]) -> Vec<VirtualCableEndpointPair> {
    let mut pairs = Vec::new();
    for ep in endpoints.iter().filter(|e| e.flow == "Render") {
        if let Some((driver_kind, group_key, is_watermark_free)) =
            classify_virtual_endpoint(&ep.name)
        {
            let paired = find_paired_capture_endpoint(ep, endpoints);
            pairs.push(VirtualCableEndpointPair {
                render_id: ep.id.clone(),
                render_name: ep.name.clone(),
                paired_capture_id: paired.map(|p| p.id.clone()),
                paired_capture_name: paired.map(|p| p.name.clone()),
                driver_kind: driver_kind.to_string(),
                group_key,
                is_watermark_free,
            });
        }
    }
    pairs
}

/// Evaluates the health and pairing details of `Silent Routing Sink`, `Virtual Mic`, and `Stream Mix`.
pub fn inspect_virtual_cables(
    cfg: &RouterConfig,
    endpoints: &[AudioEndpointInfo],
) -> VirtualCableStatusPayload {
    let detected_cables = detect_virtual_cable_pairs(endpoints);
    let vb_cable_installed = detected_cables
        .iter()
        .any(|c| c.driver_kind == "vb-cable")
        || endpoints.iter().any(|e| {
            classify_virtual_endpoint(&e.name).map(|(k, _, _)| k == "vb-cable").unwrap_or(false)
        });
    let hifi_cable_installed = detected_cables
        .iter()
        .any(|c| c.driver_kind == "hifi-cable")
        || endpoints.iter().any(|e| {
            classify_virtual_endpoint(&e.name).map(|(k, _, _)| k == "hifi-cable").unwrap_or(false)
        });
    let steam_audio_available = detected_cables
        .iter()
        .any(|c| c.driver_kind == "steam");

    let master_id = cfg
        .master_output_device_id
        .clone()
        .or_else(|| {
            endpoints
                .iter()
                .find(|e| e.flow == "Render" && e.is_default)
                .map(|e| e.id.clone())
        });

    let group_for_id = |id: &str| -> String {
        endpoints
            .iter()
            .find(|e| e.id == id)
            .and_then(|e| classify_virtual_endpoint(&e.name).map(|(_, g, _)| g))
            .unwrap_or_else(|| format!("id:{id}"))
    };

    let evaluate_slot = |slot: &str,
                         device_id: Option<&String>,
                         other_ids: &[Option<&String>],
                         empty_hint: &str,
                         ready_prefix: &str|
     -> RouteSlotStatus {
        let Some(id) = device_id.filter(|s| !s.trim().is_empty()) else {
            return RouteSlotStatus {
                slot: slot.to_string(),
                device_id: None,
                device_name: None,
                paired_capture_name: None,
                status: "unassigned".to_string(),
                hint: empty_hint.to_string(),
            };
        };

        let Some(ep) = endpoints.iter().find(|e| e.flow == "Render" && e.id == *id) else {
            return RouteSlotStatus {
                slot: slot.to_string(),
                device_id: Some(id.clone()),
                device_name: None,
                paired_capture_name: None,
                status: "missing".to_string(),
                hint: "Selected endpoint is disconnected or disabled. Click Repair or choose another cable.".to_string(),
            };
        };

        let paired_capture_name = find_paired_capture_endpoint(ep, endpoints).map(|c| c.name.clone());
        let my_group = group_for_id(id);

        if master_id.as_deref() == Some(id.as_str()) {
            return RouteSlotStatus {
                slot: slot.to_string(),
                device_id: Some(id.clone()),
                device_name: Some(ep.name.clone()),
                paired_capture_name,
                status: "conflict".to_string(),
                hint: "Conflicts with Monitor Output (Headphones). Choose a separate virtual cable.".to_string(),
            };
        }

        for other in other_ids.iter().flatten() {
            if *other == id || group_for_id(other) == my_group {
                return RouteSlotStatus {
                    slot: slot.to_string(),
                    device_id: Some(id.clone()),
                    device_name: Some(ep.name.clone()),
                    paired_capture_name,
                    status: "conflict".to_string(),
                    hint: "Shares the same virtual cable pipe with another Stellars route. Assign a separate cable.".to_string(),
                };
            }
        }

        let hint = match (&paired_capture_name, slot) {
            (_, "silentSink") if !is_virtual_audio_endpoint_name(&ep.name) => {
                "Muted by Stellars · Routed apps' original sound goes here, so you never hear it twice.".to_string()
            }
            (_, "silentSink") => {
                "Silences raw dry app audio for Game, Chat, Media, and Aux.".to_string()
            }
            (Some(cap), _) => format!("{ready_prefix}: {cap}"),
            (None, _) => format!("Sending audio to {}", ep.name),
        };

        RouteSlotStatus {
            slot: slot.to_string(),
            device_id: Some(id.clone()),
            device_name: Some(ep.name.clone()),
            paired_capture_name,
            status: "ready".to_string(),
            hint,
        }
    };

    let sink_id = cfg.dummy_sink_device_id.as_ref();
    let vmic_id = cfg.virtual_mic_output_device_id.as_ref();
    let stream_id = cfg.stream.output_device_id.as_ref();

    let silent_sink = evaluate_slot(
        "silentSink",
        sink_id,
        &[vmic_id, stream_id],
        "Unassigned · Routed apps will still play dry audio on default speakers until assigned.",
        "Silent sink active",
    );
    let virtual_mic = evaluate_slot(
        "virtualMic",
        vmic_id,
        &[sink_id, stream_id],
        "Unassigned · Assign a virtual cable to feed your EQ/gated Mic into Discord or games.",
        "In Discord / Game Voice, select Mic",
    );
    // With only VB-CABLE and the Hi-Fi Cable, both already carry the silent sink and the virtual
    // mic: say so instead of suggesting a cable that would only cause a conflict.
    let used_groups: BTreeSet<String> = [master_id.as_ref(), sink_id, vmic_id]
        .into_iter()
        .flatten()
        .map(|id| group_for_id(id))
        .collect();
    let has_spare_cable = detected_cables
        .iter()
        .any(|c| !used_groups.contains(&c.group_key));
    let spare_outputs = spare_outputs(cfg, endpoints);
    let sink_is_cable = sink_id.is_some_and(|id| detected_cables.iter().any(|c| &c.render_id == id));
    let can_free_cable_for_stream =
        stream_id.is_none() && !has_spare_cable && sink_is_cable && !spare_outputs.is_empty();
    let stream_empty_hint = if has_spare_cable {
        "Unassigned · Assign a dedicated virtual cable to capture your Stream Mix in OBS."
    } else if can_free_cable_for_stream {
        "No spare cable · Your cables carry the silent sink and the mic. Use an output you don't listen to as the silent sink to free one."
    } else {
        "No spare cable · Your virtual cables already carry the silent sink and the mic. Add another virtual cable to send a Stream Mix to OBS."
    };
    let stream_mix = evaluate_slot(
        "streamMix",
        stream_id,
        &[sink_id, vmic_id],
        stream_empty_hint,
        "In OBS Audio Input Capture, select",
    );

    let all_routes_healthy = silent_sink.status == "ready"
        && virtual_mic.status == "ready"
        && stream_mix.status == "ready";

    VirtualCableStatusPayload {
        vb_cable_installed,
        hifi_cable_installed,
        steam_audio_available,
        detected_cables,
        silent_sink,
        virtual_mic,
        stream_mix,
        all_routes_healthy,
        spare_outputs,
        can_free_cable_for_stream,
    }
}

fn is_16ch_auxiliary_vb_endpoint(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.contains("16 ch") || lower.contains("16ch")
}

unsafe fn get_mmdevice_id(device: &IMMDevice) -> Option<String> {
    let raw_id: PWSTR = device.GetId().ok()?;
    let id_str = raw_id.to_string().ok();
    CoTaskMemFree(Some(raw_id.0 as *const _));
    id_str
}

unsafe fn read_prop_string(store: &IPropertyStore, key: &PROPERTYKEY) -> String {
    store
        .GetValue(key)
        .ok()
        .map(|pv| pv.to_string())
        .unwrap_or_default()
}

unsafe fn write_prop_string(
    store: &IPropertyStore,
    key: &PROPERTYKEY,
    value: &str,
) -> Result<(), String> {
    let wide: Vec<u16> = value.encode_utf16().chain(std::iter::once(0)).collect();
    let raw_pv = RawPropVariantLpwstr {
        vt: VT_LPWSTR_VAL,
        w_reserved1: 0,
        w_reserved2: 0,
        w_reserved3: 0,
        pwsz_val: wide.as_ptr(),
        _pad: 0,
    };
    let pv_ref: &PROPVARIANT = &*(&raw_pv as *const RawPropVariantLpwstr as *const PROPVARIANT);
    store
        .SetValue(key, pv_ref)
        .map_err(|e| format!("IPropertyStore::SetValue({value}) failed: {e}"))
}

/// Applies system-wide Windows endpoint friendly names (`PKEY_Device_DeviceDesc` and
/// `PKEY_DeviceInterface_FriendlyName`) to the included WHQL virtual cables (`VBAudioVACWDM`
/// and `VBAudioHFVAIO`) via `IMMDevice::OpenPropertyStore(STGM_READWRITE)` so they appear as
/// `Stellars - Microphone (Stellars Virtual Audio Cable)` and `Stellars - Stream Mix (Stellars Hi-Fi Virtual Cable)`
/// across Discord, OBS, games, and Windows Sound Settings without altering driver binaries or WHQL signatures.
pub fn apply_system_virtual_cable_names(cfg: &RouterConfig) -> Vec<String> {
    let mut actions = Vec::new();
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let Ok(enumerator) = CoCreateInstance::<_, IMMDeviceEnumerator>(&MMDeviceEnumerator, None, CLSCTX_ALL) else {
            return actions;
        };

        struct DiscoveredCableEndpoint {
            device: IMMDevice,
            id: String,
            flow: &'static str,
            matching_hw_id: String,
            ks_nodetype: String,
            cur_desc: String,
            cur_iface: String,
        }

        let mut discovered: Vec<DiscoveredCableEndpoint> = Vec::new();

        for (flow, flow_label) in [(eRender, "Render"), (eCapture, "Capture")] {
            let Ok(collection) = enumerator.EnumAudioEndpoints(flow, DEVICE_STATE_ACTIVE) else {
                continue;
            };
            let count = collection.GetCount().unwrap_or(0);
            for idx in 0..count {
                let Ok(device) = collection.Item(idx) else {
                    continue;
                };
                let Some(id) = get_mmdevice_id(&device) else {
                    continue;
                };
                let Ok(store) = device.OpenPropertyStore(STGM_READ) else {
                    continue;
                };
                let matching_hw_id = read_prop_string(&store, &DEVPKEY_MATCHING_DEVICE_ID);
                let ks_nodetype = read_prop_string(&store, &PKEY_ENDPOINT_KSNODETYPE);
                let cur_desc = read_prop_string(&store, &PKEY_DEVICE_DESC);
                let cur_iface = read_prop_string(&store, &PKEY_INTERFACE_FRIENDLY_NAME);

                let hw_upper = matching_hw_id.to_ascii_uppercase();
                if hw_upper.contains("VBAUDIOVACWDM") || hw_upper.contains("VBAUDIOHFVAIO") {
                    discovered.push(DiscoveredCableEndpoint {
                        device,
                        id,
                        flow: flow_label,
                        matching_hw_id: hw_upper,
                        ks_nodetype: ks_nodetype.to_ascii_uppercase(),
                        cur_desc,
                        cur_iface,
                    });
                }
            }
        }

        // Determine which Stellars slot each cable family (`VBAUDIOVACWDM`, `VBAUDIOHFVAIO`) is currently serving
        let role_for_driver = |hw_tag: &str, default_role: &str| -> &'static str {
            for ep in discovered.iter().filter(|e| e.flow == "Render" && e.matching_hw_id.contains(hw_tag)) {
                if cfg.virtual_mic_output_device_id.as_deref() == Some(ep.id.as_str()) {
                    return "mic";
                }
                if cfg.stream.output_device_id.as_deref() == Some(ep.id.as_str()) {
                    return "stream";
                }
                if cfg.dummy_sink_device_id.as_deref() == Some(ep.id.as_str()) {
                    return "sink";
                }
            }
            match default_role {
                "stream" => "stream",
                "sink" => "sink",
                _ => "mic",
            }
        };

        let vb_role = role_for_driver("VBAUDIOVACWDM", "mic");
        let hifi_role = role_for_driver("VBAUDIOHFVAIO", "stream");

        for ep in &discovered {
            let is_vb_main = ep.matching_hw_id.contains("VBAUDIOVACWDM");
            let role = if is_vb_main { vb_role } else { hifi_role };
            let target_iface = if is_vb_main {
                "Stellars Virtual Audio Cable"
            } else {
                "Stellars Hi-Fi Virtual Cable"
            };

            let is_16ch = is_vb_main
                && ep.flow == "Render"
                && (ep.ks_nodetype.contains("DFF21FE3")
                    || ep.cur_desc.to_ascii_lowercase().contains("16"));

            let target_desc = if is_16ch {
                "Stellars - Aux In 16ch"
            } else if ep.flow == "Render" {
                match role {
                    "stream" => "Stellars - Stream Mix In",
                    "sink" => "Stellars - Silent Sink",
                    _ => "Stellars - Virtual Mic In",
                }
            } else {
                // Capture (Recording) endpoint — what the user selects in Discord / OBS
                match role {
                    "stream" => "Stellars - Stream Mix",
                    "sink" => "Stellars - Silent Sink Out",
                    _ => "Stellars - Microphone",
                }
            };

            if ep.cur_desc != target_desc || ep.cur_iface != target_iface {
                if let Ok(rw_store) = ep.device.OpenPropertyStore(STGM_READWRITE) {
                    let ok_desc = write_prop_string(&rw_store, &PKEY_DEVICE_DESC, target_desc).is_ok();
                    let ok_iface =
                        write_prop_string(&rw_store, &PKEY_INTERFACE_FRIENDLY_NAME, target_iface)
                            .is_ok();
                    let _ = rw_store.Commit();
                    if ok_desc || ok_iface {
                        actions.push(format!(
                            "Renamed {} endpoint → {} ({})",
                            ep.flow, target_desc, target_iface
                        ));
                    }
                }
            }
        }
    }
    actions
}

/// Protects the user's physical headphones and microphone from being hijacked if Windows
/// sets a newly installed virtual cable as the system default device, then assigns
/// non-conflicting virtual cables to `Silent Routing Sink`, `Virtual Mic`, and `Stream Mix`.
pub fn auto_assign_virtual_routes(
    cfg: &mut RouterConfig,
    endpoints: &[AudioEndpointInfo],
    overwrite_existing: bool,
) -> Vec<String> {
    let mut actions = Vec::new();

    // 1. Protect Monitor Output (Headphones) if unset or pointing to a virtual cable when a physical device exists
    let render_eps: Vec<&AudioEndpointInfo> = endpoints
        .iter()
        .filter(|e| e.flow == "Render")
        .collect();
    let capture_eps: Vec<&AudioEndpointInfo> = endpoints
        .iter()
        .filter(|e| e.flow == "Capture")
        .collect();

    if cfg.master_output_device_id.is_none() {
        if let Some(default_render) = render_eps.iter().find(|e| e.is_default) {
            if is_virtual_audio_endpoint_name(&default_render.name) {
                if let Some(phys) = render_eps
                    .iter()
                    .find(|e| !is_virtual_audio_endpoint_name(&e.name))
                {
                    cfg.master_output_device_id = Some(phys.id.clone());
                    actions.push(format!(
                        "Pinned Monitor Output to physical device '{}'",
                        phys.name
                    ));
                }
            }
        }
    }

    if cfg.physical_mic_device_id.is_none() {
        if let Some(default_cap) = capture_eps.iter().find(|e| e.is_default) {
            if is_virtual_audio_endpoint_name(&default_cap.name) {
                if let Some(phys_mic) = capture_eps
                    .iter()
                    .find(|e| !is_virtual_audio_endpoint_name(&e.name))
                {
                    cfg.physical_mic_device_id = Some(phys_mic.id.clone());
                    actions.push(format!(
                        "Pinned Microphone Input to physical device '{}'",
                        phys_mic.name
                    ));
                }
            }
        }
    }

    let master_id = cfg.master_output_device_id.clone().or_else(|| {
        render_eps
            .iter()
            .find(|e| e.is_default)
            .map(|e| e.id.clone())
    });

    let mut used_ids: BTreeSet<String> = BTreeSet::new();
    let mut used_groups: BTreeSet<String> = BTreeSet::new();

    let group_for = |id: &str| -> String {
        render_eps
            .iter()
            .find(|e| e.id == id)
            .and_then(|e| classify_virtual_endpoint(&e.name).map(|(_, g, _)| g))
            .unwrap_or_else(|| format!("id:{id}"))
    };

    if let Some(ref m_id) = master_id {
        used_ids.insert(m_id.clone());
        used_groups.insert(group_for(m_id));
    }

    let is_valid_and_free =
        |id_opt: &Option<String>, used_ids: &BTreeSet<String>, used_groups: &BTreeSet<String>| -> bool {
            let Some(id) = id_opt.as_ref().filter(|s| !s.trim().is_empty()) else {
                return false;
            };
            if !render_eps.iter().any(|e| e.id == *id) {
                return false;
            }
            let grp = group_for(id);
            !used_ids.contains(id) && !used_groups.contains(&grp)
        };

    if overwrite_existing {
        cfg.dummy_sink_device_id = None;
        cfg.virtual_mic_output_device_id = None;
        cfg.stream.output_device_id = None;
    } else {
        // Validate existing slots in order: Silent Sink, Virtual Mic, Stream Mix
        if is_valid_and_free(&cfg.dummy_sink_device_id, &used_ids, &used_groups) {
            let id = cfg.dummy_sink_device_id.clone().unwrap();
            used_groups.insert(group_for(&id));
            used_ids.insert(id);
        } else {
            cfg.dummy_sink_device_id = None;
        }

        if is_valid_and_free(&cfg.virtual_mic_output_device_id, &used_ids, &used_groups) {
            let id = cfg.virtual_mic_output_device_id.clone().unwrap();
            used_groups.insert(group_for(&id));
            used_ids.insert(id);
        } else {
            cfg.virtual_mic_output_device_id = None;
        }

        if is_valid_and_free(&cfg.stream.output_device_id, &used_ids, &used_groups) {
            let id = cfg.stream.output_device_id.clone().unwrap();
            used_groups.insert(group_for(&id));
            used_ids.insert(id);
        } else {
            cfg.stream.output_device_id = None;
        }
    }

    let pairs = detect_virtual_cable_pairs(endpoints);

    // Helper to pick best candidate according to a scoring closure
    let pick_candidate = |used_ids: &mut BTreeSet<String>,
                          used_groups: &mut BTreeSet<String>,
                          score_fn: &dyn Fn(&VirtualCableEndpointPair) -> i32|
     -> Option<VirtualCableEndpointPair> {
        let mut best: Option<(i32, &VirtualCableEndpointPair)> = None;
        for p in &pairs {
            if used_ids.contains(&p.render_id) || used_groups.contains(&p.group_key) {
                continue;
            }
            let score = score_fn(p);
            if score < 0 {
                continue;
            }
            if best.is_none_or(|(b_score, _)| score > b_score) {
                best = Some((score, p));
            }
        }
        if let Some((_, chosen)) = best {
            used_ids.insert(chosen.render_id.clone());
            used_groups.insert(chosen.group_key.clone());
            Some(chosen.clone())
        } else {
            None
        }
    };

    // 2. Assign Virtual Mic Output first if a paired-capture watermark-free cable is available
    // (or if multiple virtual endpoints exist, reserve the primary VB-CABLE for Virtual Mic)
    let distinct_groups_available = pairs
        .iter()
        .filter(|p| !used_groups.contains(&p.group_key))
        .map(|p| p.group_key.as_str())
        .collect::<BTreeSet<_>>()
        .len();

    if cfg.virtual_mic_output_device_id.is_none() && distinct_groups_available >= 2 {
        if let Some(chosen) = pick_candidate(&mut used_ids, &mut used_groups, &|p| {
            let mut s = if p.is_watermark_free { 100 } else { 10 };
            if p.paired_capture_id.is_some() {
                s += 100;
            }
            if is_16ch_auxiliary_vb_endpoint(&p.render_name) {
                s -= 40;
            }
            s += match p.driver_kind.as_str() {
                "vb-cable" => 50,
                "steam" if p.group_key == "steam-mic" => 45,
                "vb-cable-abcd" => 40,
                "hifi-cable" => 35,
                "voicemeeter" => 25,
                _ => 0,
            };
            s
        }) {
            cfg.virtual_mic_output_device_id = Some(chosen.render_id.clone());
            actions.push(format!("Assigned Virtual Mic Output → {}", chosen.render_name));
        }
    }

    // 3. Assign Silent Routing Sink (prefers non-capture sinks like Steam Speakers / Oculus first so paired cables remain free for Stream/Mic)
    if cfg.dummy_sink_device_id.is_none() {
        if let Some(chosen) = pick_candidate(&mut used_ids, &mut used_groups, &|p| {
            let mut s = 100;
            if is_16ch_auxiliary_vb_endpoint(&p.render_name) {
                s -= 30;
            }
            s += match p.driver_kind.as_str() {
                "steam" if p.group_key == "steam-speakers" => 60,
                "oculus" => 55,
                "other-virtual" => 45, // Safe for silent sink because nothing records from the silent sink
                "hifi-cable" => 40,
                "vb-cable-abcd" => 35,
                "vb-cable" => 30,
                "voicemeeter" => 25,
                _ => 10,
            };
            s
        }) {
            cfg.dummy_sink_device_id = Some(chosen.render_id.clone());
            actions.push(format!(
                "Assigned Silent Routing Sink → {}",
                chosen.render_name
            ));
        }
    }

    // 4. If Virtual Mic wasn't assigned earlier (e.g. only 1 cable was left after Silent Sink), try assigning now
    if cfg.virtual_mic_output_device_id.is_none() {
        if let Some(chosen) = pick_candidate(&mut used_ids, &mut used_groups, &|p| {
            let mut s = if p.is_watermark_free { 100 } else { 10 };
            if p.paired_capture_id.is_some() {
                s += 80;
            }
            if is_16ch_auxiliary_vb_endpoint(&p.render_name) {
                s -= 40;
            }
            s += match p.driver_kind.as_str() {
                "vb-cable" => 50,
                "hifi-cable" => 45,
                "vb-cable-abcd" => 40,
                "steam" => 35,
                _ => 0,
            };
            s
        }) {
            cfg.virtual_mic_output_device_id = Some(chosen.render_id.clone());
            actions.push(format!("Assigned Virtual Mic Output → {}", chosen.render_name));
        }
    }

    // 5. Assign Stream Mix Output
    if cfg.stream.output_device_id.is_none() {
        if let Some(chosen) = pick_candidate(&mut used_ids, &mut used_groups, &|p| {
            let mut s = if p.is_watermark_free { 100 } else { 5 };
            if p.paired_capture_id.is_some() {
                s += 80;
            }
            if is_16ch_auxiliary_vb_endpoint(&p.render_name) {
                s -= 40;
            }
            s += match p.driver_kind.as_str() {
                "hifi-cable" => 55,
                "vb-cable-abcd" => 50,
                "vb-cable" => 45,
                "steam" => 35,
                "voicemeeter" => 25,
                _ => 0,
            };
            s
        }) {
            cfg.stream.output_device_id = Some(chosen.render_id.clone());
            actions.push(format!("Assigned Stream Mix Output → {}", chosen.render_name));
        }
    }

    actions
}

/// Command-line flag the installer uses to set up the virtual cables without opening the app.
pub const SETUP_CABLES_FLAG: &str = "--setup-cables";

/// True when the process was started by the installer to set up the virtual cables.
pub fn is_setup_invocation<I: IntoIterator<Item = String>>(args: I) -> bool {
    args.into_iter().skip(1).any(|a| a == SETUP_CABLES_FLAG)
}

/// Exit code `--setup-cables` returns when Windows must restart before the cables appear
/// (`ERROR_SUCCESS_REBOOT_REQUIRED`); the installer then offers a restart on its last page.
pub const SETUP_REBOOT_EXIT_CODE: i32 = 3010;

/// Headless setup run by the installer: installs VB-CABLE and the Hi-Fi Cable when missing and
/// applies the Stellars endpoint names. The installer is already elevated, so no extra prompt is
/// needed. Returns a log of what happened and whether a restart is needed; it never panics so the
/// installation can always finish.
pub fn run_installer_setup() -> (Vec<String>, bool) {
    let (mut actions, reboot_recommended) = match run_elevated_vbcable_installer(true, false) {
        Ok(result) => result,
        Err(err) => (vec![format!("Virtual cable setup failed: {err}")], false),
    };
    // Name each cable after the slot the app will give it on first launch, so the names seen in
    // Discord and OBS are right from the start and do not change when Stellars opens.
    let mut first_run_cfg = RouterConfig::default();
    if let Ok(endpoints) = enumerate_audio_endpoints() {
        let _ = auto_assign_virtual_routes(&mut first_run_cfg, &endpoints, false);
    }
    actions.extend(apply_system_virtual_cable_names(&first_run_cfg));
    (actions, reboot_recommended)
}

fn driver_staging_dir() -> PathBuf {
    if let Ok(local_app_data) = std::env::var("LOCALAPPDATA") {
        PathBuf::from(local_app_data)
            .join("StellarsSonar")
            .join("drivers")
    } else {
        std::env::temp_dir().join("StellarsSonar").join("drivers")
    }
}

/// Runs an elevated PowerShell driver setup & repair script that downloads (if not cached),
/// extracts, and installs/repairs WHQL-signed VB-CABLE (`VBCABLE_Setup_x64.exe`) and
/// VB-Audio Hi-Fi Cable (`HiFiCableAsioBridgeSetup.exe`), then scans PnP devices.
pub fn run_elevated_vbcable_installer(
    install_hifi_cable: bool,
    force_repair: bool,
) -> Result<(Vec<String>, bool), String> {
    let staging = driver_staging_dir();
    fs::create_dir_all(&staging).map_err(|e| format!("Failed to create driver staging directory: {e}"))?;

    let status_file = staging.join("install_result.txt");
    let _ = fs::remove_file(&status_file);

    let script_path = staging.join("stellars_install_cables.ps1");
    let inner_script = build_elevated_powershell_script(
        &staging,
        &status_file,
        install_hifi_cable,
        force_repair,
    );
    fs::write(&script_path, inner_script)
        .map_err(|e| format!("Failed to write driver setup script: {e}"))?;

    // Launch elevated PowerShell via Start-Process -Verb RunAs -Wait so user gets a single UAC prompt
    let launcher_cmd = format!(
        "$ErrorActionPreference = 'Stop'; \
         $p = Start-Process -FilePath 'powershell.exe' -ArgumentList '-NoProfile','-NonInteractive','-ExecutionPolicy','Bypass','-WindowStyle','Hidden','-File','{}' -Verb RunAs -Wait -PassThru; \
         exit $p.ExitCode",
        script_path.display().to_string().replace('\'', "''")
    );

    // Windows may make a new cable the default device; remember the user's own devices first.
    let defaults_before = DefaultDevices::capture_now();

    let output = Command::new("powershell.exe")
        .creation_flags(CREATE_NO_WINDOW)
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-WindowStyle",
            "Hidden",
            "-Command",
            &launcher_cmd,
        ])
        .output()
        .map_err(|e| format!("Failed to launch PowerShell installer: {e}"))?;

    let mut actions = Vec::new();
    let mut reboot_recommended = false;

    if let Ok(raw_log) = fs::read_to_string(&status_file) {
        for line in raw_log.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            if trimmed == "REBOOT_RECOMMENDED=1" {
                reboot_recommended = true;
            } else {
                actions.push(trimmed.to_string());
            }
        }
    }

    if !output.status.success() && actions.is_empty() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.to_ascii_lowercase().contains("canceled by the user")
            || stderr.to_ascii_lowercase().contains("cancelled")
        {
            return Err("Administrator permission (UAC) was declined. VB-CABLE installation requires Administrator rights.".to_string());
        }
        return Err(format!(
            "VB-CABLE installer exited with code {:?}: {}",
            output.status.code(),
            stderr.trim()
        ));
    }

    // Wait up to 4.5s for Windows MMDeviceEnumerator to register newly installed audio endpoints
    for _ in 0..9 {
        if let Ok(eps) = enumerate_audio_endpoints() {
            let status = inspect_virtual_cables(&RouterConfig::default(), &eps);
            if status.vb_cable_installed && (!install_hifi_cable || status.hifi_cable_installed) {
                break;
            }
        }
        thread::sleep(Duration::from_millis(500));
    }

    actions.extend(restore_if_taken_over(&defaults_before));

    Ok((actions, reboot_recommended))
}

fn build_elevated_powershell_script(
    staging_dir: &Path,
    status_file: &Path,
    install_hifi_cable: bool,
    force_repair: bool,
) -> String {
    let staging_escaped = staging_dir.display().to_string().replace('\'', "''");
    let status_escaped = status_file.display().to_string().replace('\'', "''");
    let install_hifi_flag = if install_hifi_cable { "$true" } else { "$false" };
    let force_repair_flag = if force_repair { "$true" } else { "$false" };

    format!(
        r#"$ErrorActionPreference = 'Continue'
$staging = '{staging_escaped}'
$statusFile = '{status_escaped}'
$installHiFi = {install_hifi_flag}
$forceRepair = {force_repair_flag}
$log = New-Object System.Collections.Generic.List[string]
$rebootNeeded = $false

function Write-Step([string]$msg) {{
    $log.Add($msg)
}}

# Downloads over HTTPS and keeps the file only when its SHA-256 matches the pinned value.
function Get-VerifiedDownload([string]$url, [string]$sha256, [string]$outFile) {{
    if (-not $url.StartsWith('https://')) {{ return $false }}
    try {{
        Invoke-WebRequest -Uri $url -OutFile $outFile -UseBasicParsing -TimeoutSec 30
    }} catch {{
        Remove-Item -LiteralPath $outFile -Force -ErrorAction SilentlyContinue
        return $false
    }}
    $actual = (Get-FileHash -LiteralPath $outFile -Algorithm SHA256).Hash
    if ($actual -ne $sha256) {{
        Remove-Item -LiteralPath $outFile -Force -ErrorAction SilentlyContinue
        Write-Step "Rejected a download from $url because its checksum did not match"
        return $false
    }}
    return $true
}}

# An installer only runs when it carries a valid Authenticode signature from VB-Audio's author.
function Test-VbAudioSigned([string]$path) {{
    $sig = Get-AuthenticodeSignature -LiteralPath $path
    return ($sig.Status -eq 'Valid' -and $sig.SignerCertificate.Subject -match 'Vincent Burel|BUREL VINCENT')
}}

try {{
    [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12

    # 1. Re-enable any disabled VB-Audio PnP devices first
    $disabledDevs = Get-PnpDevice -ErrorAction SilentlyContinue | Where-Object {{
        $_.FriendlyName -match 'VB-Audio|CABLE Input|CABLE Output|Hi-Fi Cable|Stellars' -and $_.Status -ne 'OK'
    }}
    foreach ($dev in $disabledDevs) {{
        Enable-PnpDevice -InstanceId $dev.InstanceId -Confirm:$false -ErrorAction SilentlyContinue | Out-Null
        Write-Step "Re-enabled audio endpoint: $($dev.FriendlyName)"
    }}

    # 2. Check VB-CABLE presence
    $vbPresent = Get-PnpDevice -PresentOnly -ErrorAction SilentlyContinue | Where-Object {{
        $_.FriendlyName -match 'VB-Audio Virtual Cable|CABLE Input|Stellars Virtual Audio Cable' -or $_.InstanceId -match 'VBAudioVACWDM'
    }}

    if (-not $vbPresent -or $forceRepair) {{
        $vbZip = Join-Path $staging 'VBCABLE_Driver_Pack.zip'
        $vbDir = Join-Path $staging 'vbcable'
        if (-not (Test-Path (Join-Path $vbDir 'VBCABLE_Setup_x64.exe'))) {{
            New-Item -ItemType Directory -Force -Path $vbDir | Out-Null
            $downloaded = $false
            foreach ($pkg in @(@('{vb_url_0}', '{vb_sha_0}'), @('{vb_url_1}', '{vb_sha_1}'))) {{
                if (Get-VerifiedDownload $pkg[0] $pkg[1] $vbZip) {{
                    $downloaded = $true
                    break
                }}
            }}
            if ($downloaded -and (Test-Path $vbZip)) {{
                Expand-Archive -Path $vbZip -DestinationPath $vbDir -Force
                Write-Step 'Downloaded official WHQL VB-CABLE driver package'
            }}
        }}

        $infPath = Join-Path $vbDir 'vbMmeCable64_win10.inf'
        $setupExe = Join-Path $vbDir 'VBCABLE_Setup_x64.exe'
        if (Test-Path $infPath) {{
            pnputil.exe /add-driver $infPath /install | Out-Null
            Write-Step 'Staged vbMmeCable64_win10.inf in Windows Driver Store'
        }}
        if (-not $vbPresent -and (Test-Path $setupExe) -and -not (Test-VbAudioSigned $setupExe)) {{
            Write-Step 'Skipped the VB-CABLE installer because its signature could not be verified'
        }} elseif (-not $vbPresent -and (Test-Path $setupExe)) {{
            Start-Process -FilePath $setupExe -ArgumentList '-i','-h' -WorkingDirectory $vbDir -Wait -WindowStyle Hidden
            Write-Step 'Installed VB-Audio Virtual Cable (CABLE Input / CABLE Output)'
        }}
    }} else {{
        Write-Step 'Verified VB-Audio Virtual Cable is installed and active'
    }}

    # 3. Check Hi-Fi Cable presence (2nd independent WHQL virtual cable for Stream + Mic separation)
    if ($installHiFi) {{
        $hifiPresent = Get-PnpDevice -PresentOnly -ErrorAction SilentlyContinue | Where-Object {{
            $_.FriendlyName -match 'Hi-Fi Cable|Stellars Hi-Fi' -or $_.InstanceId -match 'VBAudioHFVAIO'
        }}
        if (-not $hifiPresent -or $forceRepair) {{
            $hifiZip = Join-Path $staging 'HiFiCableAsioBridgeSetup.zip'
            $hifiDir = Join-Path $staging 'hificable'
            $hifiExe = Join-Path $hifiDir 'HiFiCableAsioBridgeSetup.exe'
            if (-not (Test-Path $hifiExe)) {{
                New-Item -ItemType Directory -Force -Path $hifiDir | Out-Null
                $downloadedHifi = $false
                foreach ($pkg in @(@('{hifi_url_0}', '{hifi_sha_0}'))) {{
                    if (Get-VerifiedDownload $pkg[0] $pkg[1] $hifiZip) {{
                        $downloadedHifi = $true
                        break
                    }}
                }}
                if ($downloadedHifi -and (Test-Path $hifiZip)) {{
                    Expand-Archive -Path $hifiZip -DestinationPath $hifiDir -Force
                    Write-Step 'Downloaded official WHQL VB-Audio Hi-Fi Cable package'
                }}
            }}
            if (-not $hifiPresent -and (Test-Path $hifiExe) -and -not (Test-VbAudioSigned $hifiExe)) {{
                Write-Step 'Skipped the Hi-Fi Cable installer because its signature could not be verified'
            }} elseif (-not $hifiPresent -and (Test-Path $hifiExe)) {{
                Start-Process -FilePath $hifiExe -ArgumentList '-i','-h' -WorkingDirectory $hifiDir -Wait -WindowStyle Hidden
                Write-Step 'Installed VB-Audio Hi-Fi Cable (Hi-Fi Cable Input / Output)'
            }}
        }} else {{
            Write-Step 'Verified VB-Audio Hi-Fi Cable is installed and active'
        }}
    }}

    # 4. Trigger PnP device rescan so MMDeviceEnumerator registers endpoints immediately
    pnputil.exe /scan-devices | Out-Null

    # 5. Apply system-wide Stellars endpoint names in MMDevices\Audio Properties
    $rights = [System.Security.AccessControl.RegistryRights]::SetValue -bor [System.Security.AccessControl.RegistryRights]::QueryValues
    foreach ($flow in @('Render','Capture')) {{
        $baseKey = "SOFTWARE\Microsoft\Windows\CurrentVersion\MMDevices\Audio\$flow"
        $flowKey = [Microsoft.Win32.Registry]::LocalMachine.OpenSubKey($baseKey)
        if ($null -eq $flowKey) {{ continue }}
        foreach ($guid in $flowKey.GetSubKeyNames()) {{
            $propSub = "$baseKey\$guid\Properties"
            $pk = [Microsoft.Win32.Registry]::LocalMachine.OpenSubKey($propSub, [Microsoft.Win32.RegistryKeyPermissionCheck]::ReadWriteSubTree, $rights)
            if ($null -eq $pk) {{ continue }}
            try {{
                $hw = [string]$pk.GetValue('{{a8b865dd-2e3d-4094-ad97-e593a70c75d6}},8')
                $ks = [string]$pk.GetValue('{{1da5d803-d492-4edd-8c23-e0c0ffee7f0e}},8')
                $curDesc = [string]$pk.GetValue('{{a45c254e-df1c-4efd-8020-67d146a850e0}},2')
                if ($hw -match 'VBAudioVACWDM') {{
                    $desc = if ($flow -eq 'Capture') {{ 'Stellars - Microphone' }} elseif ($ks -match 'DFF21FE3' -or $curDesc -match '16') {{ 'Stellars - Aux In 16ch' }} else {{ 'Stellars - Virtual Mic In' }}
                    $pk.SetValue('{{a45c254e-df1c-4efd-8020-67d146a850e0}},2', $desc, [Microsoft.Win32.RegistryValueKind]::String)
                    $pk.SetValue('{{b3f8fa53-0004-438e-9003-51a46e139bfc}},6', 'Stellars Virtual Audio Cable', [Microsoft.Win32.RegistryValueKind]::String)
                }} elseif ($hw -match 'VBAudioHFVAIO') {{
                    $desc = if ($flow -eq 'Capture') {{ 'Stellars - Stream Mix' }} else {{ 'Stellars - Stream Mix In' }}
                    $pk.SetValue('{{a45c254e-df1c-4efd-8020-67d146a850e0}},2', $desc, [Microsoft.Win32.RegistryValueKind]::String)
                    $pk.SetValue('{{b3f8fa53-0004-438e-9003-51a46e139bfc}},6', 'Stellars Hi-Fi Virtual Cable', [Microsoft.Win32.RegistryValueKind]::String)
                }}
            }} finally {{
                $pk.Close()
            }}
        }}
        $flowKey.Close()
    }}

    $vbAfter = Get-PnpDevice -PresentOnly -ErrorAction SilentlyContinue | Where-Object {{
        $_.FriendlyName -match 'VB-Audio Virtual Cable|CABLE Input|Stellars' -or $_.InstanceId -match 'VBAudioVACWDM'
    }}
    if (-not $vbAfter) {{
        $rebootNeeded = $true
    }}
}} catch {{
    Write-Step "Setup warning: $($_.Exception.Message)"
}}

if ($rebootNeeded) {{
    $log.Add('REBOOT_RECOMMENDED=1')
}}
[System.IO.File]::WriteAllLines($statusFile, $log)
"#,
        vb_url_0 = VBCABLE_DOWNLOADS[0].0,
        vb_sha_0 = VBCABLE_DOWNLOADS[0].1,
        vb_url_1 = VBCABLE_DOWNLOADS[1].0,
        vb_sha_1 = VBCABLE_DOWNLOADS[1].1,
        hifi_url_0 = HIFICABLE_DOWNLOADS[0].0,
        hifi_sha_0 = HIFICABLE_DOWNLOADS[0].1,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn driver_downloads_are_https_with_pinned_sha256() {
        for (url, sha) in VBCABLE_DOWNLOADS.iter().chain(HIFICABLE_DOWNLOADS.iter()) {
            assert!(url.starts_with("https://"), "download must use https: {url}");
            assert_eq!(sha.len(), 64, "pinned hash must be SHA-256: {url}");
            assert!(sha.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_lowercase()));
        }
    }

    #[test]
    fn setup_script_verifies_downloads_and_installer_signatures() {
        let script = build_elevated_powershell_script(
            Path::new("C:\\staging"),
            Path::new("C:\\status.txt"),
            true,
            false,
        );
        assert!(!script.contains("http://"));
        assert!(script.contains("Get-VerifiedDownload"));
        assert_eq!(script.matches("Test-VbAudioSigned").count(), 3, "definition plus one check per installer");
        for (_, sha) in VBCABLE_DOWNLOADS.iter().chain(HIFICABLE_DOWNLOADS.iter()) {
            assert!(script.contains(sha));
        }
    }

    fn sample_endpoints() -> Vec<AudioEndpointInfo> {
        vec![
            AudioEndpointInfo {
                id: "ep-headphones".into(),
                name: "Headphones (Realtek USB2.0 Audio)".into(),
                flow: "Render".into(),
                is_default: true,
                is_dummy_sink_candidate: false,
                is_virtual_mic_candidate: false,
            },
            AudioEndpointInfo {
                id: "ep-phys-mic".into(),
                name: "Microphone (USB PnP Audio Device)".into(),
                flow: "Capture".into(),
                is_default: true,
                is_dummy_sink_candidate: false,
                is_virtual_mic_candidate: false,
            },
            AudioEndpointInfo {
                id: "ep-vbcable-in".into(),
                name: "CABLE Input (VB-Audio Virtual Cable)".into(),
                flow: "Render".into(),
                is_default: false,
                is_dummy_sink_candidate: true,
                is_virtual_mic_candidate: true,
            },
            AudioEndpointInfo {
                id: "ep-vbcable-16ch".into(),
                name: "CABLE In 16 Ch (VB-Audio Virtual Cable)".into(),
                flow: "Render".into(),
                is_default: false,
                is_dummy_sink_candidate: true,
                is_virtual_mic_candidate: true,
            },
            AudioEndpointInfo {
                id: "ep-vbcable-out".into(),
                name: "CABLE Output (VB-Audio Virtual Cable)".into(),
                flow: "Capture".into(),
                is_default: false,
                is_dummy_sink_candidate: false,
                is_virtual_mic_candidate: false,
            },
            AudioEndpointInfo {
                id: "ep-hifi-in".into(),
                name: "Hi-Fi Cable Input (VB-Audio Hi-Fi Cable)".into(),
                flow: "Render".into(),
                is_default: false,
                is_dummy_sink_candidate: true,
                is_virtual_mic_candidate: true,
            },
            AudioEndpointInfo {
                id: "ep-hifi-out".into(),
                name: "Hi-Fi Cable Output (VB-Audio Hi-Fi Cable)".into(),
                flow: "Capture".into(),
                is_default: false,
                is_dummy_sink_candidate: false,
                is_virtual_mic_candidate: false,
            },
            AudioEndpointInfo {
                id: "ep-oculus-render".into(),
                name: "Headphones (Oculus Virtual Audio Device)".into(),
                flow: "Render".into(),
                is_default: false,
                is_dummy_sink_candidate: true,
                is_virtual_mic_candidate: false,
            },
        ]
    }

    
    #[test]
    fn test_inspect_cables_live() {
        let endpoints = enumerate_audio_endpoints().unwrap();
        let cfg = RouterConfig::default();
        let status = inspect_virtual_cables(&cfg, &endpoints);
        println!("[TEST_CABLE_STATUS] vb_cable_installed={}", status.vb_cable_installed);
        println!("[TEST_CABLE_STATUS] hifi_cable_installed={}", status.hifi_cable_installed);
        println!("[TEST_CABLE_STATUS] detected_cables len={}", status.detected_cables.len());
        for c in &status.detected_cables {
            println!("  cable: render='{}' paired='{:?}' kind='{}' group='{}'", c.render_name, c.paired_capture_name, c.driver_kind, c.group_key);
        }
    }

    #[test]
    fn test_auto_assign_virtual_routes_assigns_three_non_conflicting_pipes() {
        let eps = sample_endpoints();
        let mut cfg = RouterConfig::default();

        let actions = auto_assign_virtual_routes(&mut cfg, &eps, true);
        assert!(!actions.is_empty());

        // Virtual Mic gets VB-CABLE Input, Stream gets Hi-Fi Cable Input, Silent Sink gets Oculus VAD
        assert_eq!(
            cfg.virtual_mic_output_device_id.as_deref(),
            Some("ep-vbcable-in")
        );
        assert_eq!(
            cfg.stream.output_device_id.as_deref(),
            Some("ep-hifi-in")
        );
        assert_eq!(
            cfg.dummy_sink_device_id.as_deref(),
            Some("ep-oculus-render")
        );

        // Verify CABLE In 16 Ch was NOT assigned to another slot because it shares vbcable-main pipe
        let status = inspect_virtual_cables(&cfg, &eps);
        assert!(status.vb_cable_installed);
        assert!(status.hifi_cable_installed);
        assert!(status.all_routes_healthy);
        assert_eq!(
            status.virtual_mic.paired_capture_name.as_deref(),
            Some("CABLE Output (VB-Audio Virtual Cable)")
        );
        assert_eq!(
            status.stream_mix.paired_capture_name.as_deref(),
            Some("Hi-Fi Cable Output (VB-Audio Hi-Fi Cable)")
        );
    }

    #[test]
    fn test_protects_physical_headphones_when_vbcable_becomes_windows_default() {
        let mut eps = sample_endpoints();
        // Simulate Windows switching system default playback to CABLE Input after driver install
        eps[0].is_default = false;
        eps[2].is_default = true;

        let mut cfg = RouterConfig::default();
        let _ = auto_assign_virtual_routes(&mut cfg, &eps, false);

        assert_eq!(
            cfg.master_output_device_id.as_deref(),
            Some("ep-headphones"),
            "Physical headphones must be pinned when Windows switches default to VB-CABLE"
        );
    }

    /// A fresh install has only VB-CABLE and the Hi-Fi Cable (no Oculus/Steam/extra cables).
    fn fresh_install_endpoints() -> Vec<AudioEndpointInfo> {
        sample_endpoints()
            .into_iter()
            .filter(|e| e.id != "ep-oculus-render")
            .collect()
    }

    #[test]
    fn fresh_install_with_two_cables_routes_sink_and_mic_and_explains_stream() {
        let eps = fresh_install_endpoints();
        let mut cfg = RouterConfig::default();
        let _ = auto_assign_virtual_routes(&mut cfg, &eps, false);
        assert_eq!(cfg.virtual_mic_output_device_id.as_deref(), Some("ep-vbcable-in"));
        assert_eq!(cfg.dummy_sink_device_id.as_deref(), Some("ep-hifi-in"));
        assert_eq!(cfg.stream.output_device_id, None);

        let status = inspect_virtual_cables(&cfg, &eps);
        assert_eq!(status.stream_mix.status, "unassigned");
        assert!(status.stream_mix.hint.starts_with("No spare cable"), "{}", status.stream_mix.hint);
        assert!(!status.can_free_cable_for_stream, "only headphones: nothing spare to offer");
    }

    #[test]
    fn a_spare_monitor_output_is_offered_to_free_the_hifi_cable_for_stream() {
        let mut eps = fresh_install_endpoints();
        eps.push(AudioEndpointInfo {
            id: "ep-monitor".into(),
            name: "MAG 245F X24 (NVIDIA High Definition Audio)".into(),
            flow: "Render".into(),
            is_default: false,
            is_dummy_sink_candidate: false,
            is_virtual_mic_candidate: false,
        });
        let mut cfg = RouterConfig::default();
        let _ = auto_assign_virtual_routes(&mut cfg, &eps, false);
        // Physical outputs are never chosen automatically.
        assert_eq!(cfg.dummy_sink_device_id.as_deref(), Some("ep-hifi-in"));
        let status = inspect_virtual_cables(&cfg, &eps);
        assert!(status.can_free_cable_for_stream);
        assert_eq!(status.spare_outputs.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(), vec!["ep-monitor"]);

        // Accepting the suggestion: monitor becomes the silent sink, the Hi-Fi Cable the Stream Mix.
        cfg.dummy_sink_device_id = Some("ep-monitor".into());
        cfg.stream.output_device_id = Some("ep-hifi-in".into());
        let status = inspect_virtual_cables(&cfg, &eps);
        assert!(status.all_routes_healthy, "{status:?}");
        assert!(status.silent_sink.hint.starts_with("Muted by Stellars"));
        assert!(!status.can_free_cable_for_stream);
    }

    #[test]
    fn stream_hint_suggests_a_cable_when_one_is_free() {
        let eps = sample_endpoints();
        let mut cfg = RouterConfig::default();
        let _ = auto_assign_virtual_routes(&mut cfg, &eps, false);
        cfg.stream.output_device_id = None;
        let status = inspect_virtual_cables(&cfg, &eps);
        assert!(status.stream_mix.hint.starts_with("Unassigned"), "{}", status.stream_mix.hint);
    }

    #[test]
    fn setup_flag_is_detected_only_after_the_program_name() {
        let args = |list: &[&str]| list.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert!(is_setup_invocation(args(&["stellars.exe", "--setup-cables"])));
        assert!(!is_setup_invocation(args(&["stellars.exe"])));
        assert!(!is_setup_invocation(args(&["--setup-cables"])));
        assert!(!is_setup_invocation(args(&["stellars.exe", "--other"])));
    }

    #[test]
    fn test_renamed_stellars_virtual_cables_classification_and_routing() {
        let eps = vec![
            AudioEndpointInfo {
                id: "ep-headphones".into(),
                name: "Headphones (Realtek USB2.0 Audio)".into(),
                flow: "Render".into(),
                is_default: true,
                is_dummy_sink_candidate: false,
                is_virtual_mic_candidate: false,
            },
            AudioEndpointInfo {
                id: "ep-vbcable-in".into(),
                name: "Stellars - Virtual Mic In (Stellars Virtual Audio Cable)".into(),
                flow: "Render".into(),
                is_default: false,
                is_dummy_sink_candidate: true,
                is_virtual_mic_candidate: true,
            },
            AudioEndpointInfo {
                id: "ep-vbcable-16ch".into(),
                name: "Stellars - Aux In 16ch (Stellars Virtual Audio Cable)".into(),
                flow: "Render".into(),
                is_default: false,
                is_dummy_sink_candidate: true,
                is_virtual_mic_candidate: true,
            },
            AudioEndpointInfo {
                id: "ep-vbcable-out".into(),
                name: "Stellars - Microphone (Stellars Virtual Audio Cable)".into(),
                flow: "Capture".into(),
                is_default: false,
                is_dummy_sink_candidate: false,
                is_virtual_mic_candidate: false,
            },
            AudioEndpointInfo {
                id: "ep-hifi-in".into(),
                name: "Stellars - Stream Mix In (Stellars Hi-Fi Virtual Cable)".into(),
                flow: "Render".into(),
                is_default: false,
                is_dummy_sink_candidate: true,
                is_virtual_mic_candidate: true,
            },
            AudioEndpointInfo {
                id: "ep-hifi-out".into(),
                name: "Stellars - Stream Mix (Stellars Hi-Fi Virtual Cable)".into(),
                flow: "Capture".into(),
                is_default: false,
                is_dummy_sink_candidate: false,
                is_virtual_mic_candidate: false,
            },
            AudioEndpointInfo {
                id: "ep-oculus-render".into(),
                name: "Headphones (Oculus Virtual Audio Device)".into(),
                flow: "Render".into(),
                is_default: false,
                is_dummy_sink_candidate: true,
                is_virtual_mic_candidate: false,
            },
        ];

        let mut cfg = RouterConfig::default();
        let _ = auto_assign_virtual_routes(&mut cfg, &eps, true);

        assert_eq!(
            cfg.virtual_mic_output_device_id.as_deref(),
            Some("ep-vbcable-in")
        );
        assert_eq!(
            cfg.stream.output_device_id.as_deref(),
            Some("ep-hifi-in")
        );
        assert_eq!(
            cfg.dummy_sink_device_id.as_deref(),
            Some("ep-oculus-render")
        );

        let status = inspect_virtual_cables(&cfg, &eps);
        assert!(status.vb_cable_installed);
        assert!(status.hifi_cable_installed);
        assert!(status.all_routes_healthy);
        assert_eq!(
            status.virtual_mic.paired_capture_name.as_deref(),
            Some("Stellars - Microphone (Stellars Virtual Audio Cable)")
        );
        assert_eq!(
            status.stream_mix.paired_capture_name.as_deref(),
            Some("Stellars - Stream Mix (Stellars Hi-Fi Virtual Cable)")
        );
    }

    #[test]
    fn test_apply_system_virtual_cable_names_live() {
        let cfg = RouterConfig::default();
        let _ = apply_system_virtual_cable_names(&cfg);
        let eps = enumerate_audio_endpoints().expect("enumerate_audio_endpoints should succeed");
        let pairs = detect_virtual_cable_pairs(&eps);
        for p in &pairs {
            if p.driver_kind == "vb-cable" {
                assert!(
                    p.render_name.contains("Stellars Virtual Audio Cable"),
                    "Expected VB-CABLE render endpoint to be renamed system-wide, got: {}",
                    p.render_name
                );
                if let Some(ref cap_name) = p.paired_capture_name {
                    assert_eq!(
                        cap_name,
                        "Stellars - Microphone (Stellars Virtual Audio Cable)"
                    );
                }
            } else if p.driver_kind == "hifi-cable" {
                assert_eq!(
                    p.render_name,
                    "Stellars - Stream Mix In (Stellars Hi-Fi Virtual Cable)"
                );
                if let Some(ref cap_name) = p.paired_capture_name {
                    assert_eq!(
                        cap_name,
                        "Stellars - Stream Mix (Stellars Hi-Fi Virtual Cable)"
                    );
                }
            }
        }
    }
}
