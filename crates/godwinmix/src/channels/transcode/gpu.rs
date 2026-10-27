//! Pictures kept in the GPU's memory from decode to encode.
//!
//! A hardware decoder hands out pictures in its own memory, and the
//! catalogue's `download` brings them back to system memory for a scaler and
//! an encoder on the CPU. When the decoder is NVIDIA's, every scale reading it
//! can run as `cudaconvertscale`, and every encoder reading those is NVENC,
//! none of that is needed: the decode, the scale and the pad, and the encode
//! all work on the same CUDA memory, and a 1080i picture never crosses the
//! bus. Each node so placed says `"memory": "cuda"`, and the listener builds
//! it that way.

use serde_json::{json, Value};

use super::machine::Machine;

/// Mark every decode whose readers can all stay on the GPU, and its readers.
pub fn keep_on_gpu(nodes: &mut [Value], machine: &Machine) {
    if !machine.cuda_scale {
        return;
    }
    let decodes: Vec<String> = nodes
        .iter()
        .filter(|n| n["kind"] == "decode" && n["track"] == "video" && n["accel"] == "nvidia")
        .filter_map(|n| n["id"].as_str().map(str::to_string))
        .collect();
    for decode in decodes {
        let scales: Vec<String> = ids(nodes, |n| n["kind"] == "scale" && n["input"] == decode.as_str());
        let reads = |n: &Value| n["input"] == decode.as_str() || scales.iter().any(|s| n["input"] == s.as_str());
        let encodes: Vec<String> = ids(nodes, |n| n["kind"] == "encode" && reads(n));
        let on_gpu = |id: &String| nodes.iter().find(|n| n["id"] == id.as_str()).is_some_and(|n| match n["kind"].as_str() {
            Some("scale") => n["deinterlace"] == "off" && n["fit"] != "crop",
            Some("encode") => n["accel"] == "nvidia",
            _ => false,
        });
        if encodes.is_empty() || !scales.iter().chain(&encodes).all(on_gpu) {
            continue;
        }
        for n in nodes.iter_mut() {
            let id = n["id"].as_str().unwrap_or_default().to_string();
            if id == decode {
                if let Some(o) = n.as_object_mut() {
                    o.remove("download");
                }
            }
            if id == decode || scales.contains(&id) || encodes.contains(&id) {
                n["memory"] = json!("cuda");
            }
            if scales.contains(&id) {
                n["element"] = json!("cudaconvertscale");
            }
        }
    }
}

/// Put every NVIDIA node on card `gpu`: that card's decoder and encoder by
/// name, and, where the machine has several cards, `"gpu"` on each node so
/// the listener sets the converters' `cuda-device-id` and hands every
/// element that card's CUDA context. With one card nothing changes.
pub fn on_card(nodes: &mut [Value], machine: &Machine, gpu: Option<u32>) {
    let cards = &machine.cards;
    for n in nodes.iter_mut().filter(|n| n["accel"] == "nvidia" || n["memory"] == "cuda") {
        if n["accel"] == "nvidia" {
            if let Some(e) = n["element"].as_str().map(|e| cards.element(e, gpu)) {
                n["element"] = json!(e);
            }
        }
        if cards.several() {
            n["gpu"] = json!(gpu.unwrap_or(0));
        }
    }
}

fn ids(nodes: &[Value], pick: impl Fn(&Value) -> bool) -> Vec<String> {
    nodes.iter().filter(|n| pick(n)).filter_map(|n| n["id"].as_str().map(str::to_string)).collect()
}
