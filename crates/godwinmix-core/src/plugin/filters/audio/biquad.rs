//! One second order filter section, from Robert Bristow-Johnson's cookbook,
//! run in transposed direct form II with a state per channel.

use std::f64::consts::PI;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shape {
    HighPass,
    LowPass,
    Peak,
    LowShelf,
    HighShelf,
}

impl Shape {
    pub fn parse(name: &str) -> Option<Shape> {
        Some(match name {
            "highpass" | "high_pass" | "low_cut" => Shape::HighPass,
            "lowpass" | "low_pass" | "high_cut" => Shape::LowPass,
            "peak" | "bell" => Shape::Peak,
            "lowshelf" | "low_shelf" => Shape::LowShelf,
            "highshelf" | "high_shelf" => Shape::HighShelf,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Band {
    pub shape: Shape,
    pub hz: f64,
    pub gain_db: f64,
    pub q: f64,
}

#[derive(Debug, Clone, Copy, Default)]
struct Coeffs {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
}

pub struct Biquad {
    c: Coeffs,
    /// Two state values per channel.
    z: Vec<[f32; 2]>,
}

impl Biquad {
    pub fn new(band: &Band, rate: u32, channels: usize) -> Biquad {
        Biquad { c: coeffs(band, rate), z: vec![[0.0; 2]; channels] }
    }

    pub fn process(&mut self, samples: &mut [f32], channels: usize) {
        let c = self.c;
        for frame in samples.chunks_exact_mut(channels) {
            for (x, z) in frame.iter_mut().zip(self.z.iter_mut()) {
                let y = c.b0 * *x + z[0];
                z[0] = c.b1 * *x - c.a1 * y + z[1];
                z[1] = c.b2 * *x - c.a2 * y;
                *x = y;
            }
        }
        // A denormal tail on silence costs a hundred times a normal sample
        // on some CPUs; flush it.
        for z in &mut self.z {
            for v in z.iter_mut() {
                if v.abs() < 1e-20 {
                    *v = 0.0;
                }
            }
        }
    }
}

fn coeffs(band: &Band, rate: u32) -> Coeffs {
    let fs = rate.max(1) as f64;
    let hz = band.hz.clamp(10.0, fs * 0.49);
    let w = 2.0 * PI * hz / fs;
    let (sin, cos) = w.sin_cos();
    let q = band.q.max(0.05);
    let alpha = sin / (2.0 * q);
    let a = 10f64.powf(band.gain_db / 40.0);
    let (b0, b1, b2, a0, a1, a2) = match band.shape {
        Shape::HighPass => ((1.0 + cos) / 2.0, -(1.0 + cos), (1.0 + cos) / 2.0, 1.0 + alpha, -2.0 * cos, 1.0 - alpha),
        Shape::LowPass => ((1.0 - cos) / 2.0, 1.0 - cos, (1.0 - cos) / 2.0, 1.0 + alpha, -2.0 * cos, 1.0 - alpha),
        Shape::Peak => (1.0 + alpha * a, -2.0 * cos, 1.0 - alpha * a, 1.0 + alpha / a, -2.0 * cos, 1.0 - alpha / a),
        Shape::LowShelf => {
            let k = 2.0 * a.sqrt() * alpha;
            (
                a * ((a + 1.0) - (a - 1.0) * cos + k),
                2.0 * a * ((a - 1.0) - (a + 1.0) * cos),
                a * ((a + 1.0) - (a - 1.0) * cos - k),
                (a + 1.0) + (a - 1.0) * cos + k,
                -2.0 * ((a - 1.0) + (a + 1.0) * cos),
                (a + 1.0) + (a - 1.0) * cos - k,
            )
        }
        Shape::HighShelf => {
            let k = 2.0 * a.sqrt() * alpha;
            (
                a * ((a + 1.0) + (a - 1.0) * cos + k),
                -2.0 * a * ((a - 1.0) + (a + 1.0) * cos),
                a * ((a + 1.0) + (a - 1.0) * cos - k),
                (a + 1.0) - (a - 1.0) * cos + k,
                2.0 * ((a - 1.0) - (a + 1.0) * cos),
                (a + 1.0) - (a - 1.0) * cos - k,
            )
        }
    };
    Coeffs {
        b0: (b0 / a0) as f32,
        b1: (b1 / a0) as f32,
        b2: (b2 / a0) as f32,
        a1: (a1 / a0) as f32,
        a2: (a2 / a0) as f32,
    }
}
