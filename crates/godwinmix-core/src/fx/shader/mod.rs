//! Shader transitions in the gl-transitions form.
//!
//! A shader is a GLSL function `vec4 transition(vec2 uv)` that reads the two
//! scenes through `getFromColor(uv)` and `getToColor(uv)`, with `progress`
//! from 0 to 1 and `ratio` the canvas width over its height. Its own
//! parameters are uniforms with their default in a comment,
//! `uniform float amplitude; // = 0.04`, which is how the MIT licensed
//! gl-transitions collection writes them; here each becomes a constant at
//! that value.
//!
//! On a machine with GStreamer GL the shader runs on the GPU (`gl`). Where
//! there is none, a shader that has a software version here (`cpu`) runs
//! that, and any other runs as a dissolve and `fx.list` says so.

pub mod cpu;
pub mod gl;
pub mod probe;

use super::frame::Pic;
use super::matte::dissolve;
use super::Mix;
use crate::overlay::blend::Planes;
use anyhow::{bail, Result};
use std::sync::{Arc, OnceLock};

/// The fragment shader `glshader` runs: the user's function between the
/// two lookups into a frame that holds both scenes, old above new.
pub fn fragment(source: &str) -> Result<String> {
    let body = constants(source)?;
    Ok(format!(
        "#version 100\n#ifdef GL_ES\n#ifdef GL_FRAGMENT_PRECISION_HIGH\nprecision highp float;\n#else\nprecision mediump float;\n#endif\n#endif\n\
         varying vec2 v_texcoord;\nuniform sampler2D tex;\nuniform float progress;\nuniform float ratio;\n\
         vec4 getFromColor(vec2 uv) {{ uv = clamp(uv, 0.0, 1.0); return texture2D(tex, vec2(uv.x, (1.0 - uv.y) * 0.4995)); }}\n\
         vec4 getToColor(vec2 uv) {{ uv = clamp(uv, 0.0, 1.0); return texture2D(tex, vec2(uv.x, 0.5005 + (1.0 - uv.y) * 0.4995)); }}\n\
         {body}\n\
         void main () {{\n  if (v_texcoord.y > 0.5) {{ gl_FragColor = vec4(0.0); return; }}\n\
           gl_FragColor = transition(vec2(v_texcoord.x, 1.0 - v_texcoord.y * 2.0));\n}}\n"
    ))
}

/// Every `uniform T name; // = value;` as `const T name = value;`. A
/// uniform with no default is refused by name, because there is nowhere to
/// set it from.
pub fn constants(source: &str) -> Result<String> {
    if !source.contains("transition") || !source.contains("vec4") {
        bail!("this is not a gl-transitions shader: it needs a `vec4 transition(vec2 uv)` function");
    }
    let mut out = String::with_capacity(source.len());
    for line in source.lines() {
        let trimmed = line.trim_start();
        if !trimmed.starts_with("uniform ") {
            out.push_str(line);
            out.push('\n');
            continue;
        }
        let (decl, comment) = trimmed.split_once("//").unwrap_or((trimmed, ""));
        let decl = decl.trim().trim_end_matches(';').trim();
        let parts: Vec<&str> = decl.split_whitespace().collect();
        let [_, kind, name] = parts.as_slice() else { bail!("could not read the line {trimmed:?}") };
        if matches!(*name, "progress" | "ratio") {
            continue;
        }
        let value = comment.trim().strip_prefix('=').map(|v| v.trim().trim_end_matches(';').trim());
        match value.filter(|v| !v.is_empty()) {
            Some(v) => out.push_str(&format!("const {kind} {name} = {v};\n")),
            None => bail!("the uniform {name} has no default. Write it as `uniform {kind} {name}; // = <value>` so it can run"),
        }
    }
    Ok(out)
}

/// How a shader transition runs on this machine.
pub enum Runner {
    Gpu(Box<gl::Gl>),
    Cpu(cpu::Shader),
    Dissolve,
}

impl Mix for Runner {
    fn mix(&self, old: &Pic<'_>, f: &mut Planes<'_>, t: f64) {
        match self {
            Runner::Gpu(g) => g.mix(old, f, t, None),
            Runner::Cpu(s) => s(old, f, t),
            Runner::Dissolve => dissolve(old, f, t),
        }
    }
}

impl Runner {
    /// `gpu`, `cpu` or `fade`, as `fx.list` reports it.
    pub fn word(&self) -> &'static str {
        match self {
            Runner::Gpu(_) => "gpu",
            Runner::Cpu(_) => "cpu",
            Runner::Dissolve => "fade",
        }
    }
}

/// How a shader called `name` would run here, without starting anything.
pub fn runs(name: &str) -> &'static str {
    match (probe::available(), cpu::find(name)) {
        (true, _) => "gpu",
        (false, Some(_)) => "cpu",
        (false, None) => "fade",
    }
}

/// A shader for one window. The GPU is set up on a thread of its own, and
/// until it is ready the frames are drawn the software way, or dissolved.
pub struct ShaderMix {
    ready: Arc<OnceLock<Runner>>,
    interim: Runner,
}

impl ShaderMix {
    pub fn start(name: &str, source: &str, size: (i32, i32)) -> ShaderMix {
        let interim = cpu::find(name).map(Runner::Cpu).unwrap_or(Runner::Dissolve);
        let ready: Arc<OnceLock<Runner>> = Arc::default();
        let (slot, name, source) = (ready.clone(), name.to_string(), source.to_string());
        let _ = std::thread::Builder::new().name("gmx-fx-shader".into()).spawn(move || {
            if !probe::available() {
                return;
            }
            match fragment(&source).and_then(|f| gl::Gl::start(&f, size)) {
                Ok(g) => {
                    let _ = slot.set(Runner::Gpu(Box::new(g)));
                }
                Err(e) => tracing::warn!(shader = %name, error = %format!("{e:#}"), "the shader would not run on the GPU; drawing it the software way or as a dissolve"),
            }
        });
        ShaderMix { ready, interim }
    }
}

impl Mix for ShaderMix {
    fn mix(&self, old: &Pic<'_>, f: &mut Planes<'_>, t: f64) {
        // The GPU is fed from its first frame, but until it has answered
        // the software way draws, so a slow GPU still shows a transition.
        match self.ready.get() {
            Some(Runner::Gpu(g)) => g.mix(old, f, t, Some(&self.interim)),
            Some(r) => r.mix(old, f, t),
            None => self.interim.mix(old, f, t),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_uniform_with_a_default_becomes_a_constant_and_one_without_is_refused() {
        let src = "uniform float amplitude; // = 0.04\nuniform vec2 dir; // = vec2(1.0, 0.0);\nvec4 transition(vec2 uv) { return getToColor(uv); }";
        let body = constants(src).unwrap();
        assert!(body.contains("const float amplitude = 0.04;"), "{body}");
        assert!(body.contains("const vec2 dir = vec2(1.0, 0.0);"), "{body}");
        let refused = constants("uniform float speed;\nvec4 transition(vec2 uv) { return vec4(0.0); }").unwrap_err();
        assert!(refused.to_string().contains("speed has no default"), "{refused}");
        assert!(fragment("void main() {}").is_err(), "a plain fragment shader is not a transition");
    }
}
