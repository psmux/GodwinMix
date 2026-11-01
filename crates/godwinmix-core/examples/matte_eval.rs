//! CPU evaluation using the production sampler, model, smoothing and block alpha.
//! Feed packed I420 frames, write packed I420 composites and a timing summary.
#[cfg(feature = "matte")]
#[path = "../src/plugin/filters/matte/sample.rs"]
mod sample;
#[cfg(feature = "matte")]
pub use godwinmix_core::plugin;
#[cfg(feature = "matte")]
#[allow(dead_code)]
#[path = "../src/plugin/filters/matte/blocks.rs"]
mod blocks;
#[cfg(feature = "matte")]
#[path = "../src/plugin/filters/matte/guided.rs"]
mod guided;
#[cfg(feature = "matte")]
#[path = "../src/plugin/filters/matte/components.rs"]
mod components;
#[cfg(feature = "matte")]
mod model {
    pub use godwinmix_core::plugin::filters::matte::model::Spec;
}

#[cfg(feature = "matte")]
fn main() -> anyhow::Result<()> {
    use godwinmix_core::plugin::filters::{
        chroma::frame::Region,
        matte::{
            model,
            params::{Device, Quality},
            runtime, Settings,
        },
    };
    use std::{
        io::{Read, Write},
        time::Instant,
    };
    let args: Vec<String> = std::env::args().collect();
    anyhow::ensure!(
        (7..=8).contains(&args.len()),
        "usage: matte_eval INPUT.i420 OUTPUT.i420 WIDTH HEIGHT fast|fine BACKGROUND.i420 [baseline]"
    );
    let (w, h): (usize, usize) = (args[3].parse()?, args[4].parse()?);
    let quality = if args[5] == "fine" {
        Quality::Fine
    } else {
        Quality::Fast
    };
    let spec = model::choose(&quality, false)?;
    let (mut session, ran) = runtime::session(&spec, Device::Cpu)?;
    eprintln!("{} on {}: {:?}", spec.name, ran.label(), session.inputs());
    let mut reader = std::io::BufReader::new(std::fs::File::open(&args[1])?);
    let mut writer = std::io::BufWriter::new(std::fs::File::create(&args[2])?);
    let (mut frame, mut input, mut mask, mut alpha) =
        (vec![0u8; w * h * 3 / 2], Vec::new(), Vec::new(), Vec::new());
    let baseline = args.get(7).is_some_and(|arg| arg == "baseline");
    let settings = if baseline { Settings { cutoff: 0.5, softness: 0.3, ..Settings::default() } }
        else { Settings::default() };
    let curve = settings.curve();
    let background = std::fs::read(&args[6])?;
    anyhow::ensure!(background.len() == frame.len(), "background must contain one matching I420 frame");
    let mut times = Vec::new();
    let mut refinement = Vec::new();
    let mut cleanup = Vec::new();
    while reader.read_exact(&mut frame).is_ok() {
        let started = Instant::now();
        let planes = sample::Planes {
            y: &frame[..w * h],
            u: &frame[w * h..w * h * 5 / 4],
            v: &frame[w * h * 5 / 4..],
            strides: [w, w / 2, w / 2],
            width: w,
            height: h,
        };
        sample::input(&planes, &spec, &mut input);
        let tensor = ort::value::TensorRef::from_array_view((
            [1usize, 3, spec.height, spec.width],
            input.as_slice(),
        ))?;
        let outputs = session.run(ort::inputs![tensor])?;
        let (_, answer) = outputs[0].try_extract_tensor::<f32>()?;
        anyhow::ensure!(
            answer.len() == spec.width * spec.height,
            "unexpected output size {}",
            answer.len()
        );
        let last = std::mem::take(&mut mask);
        sample::mask(answer, Some(&last), settings.steady, &mut mask);
        let refining = Instant::now();
        if !baseline { components::clean(&mut mask, spec.width, spec.height); }
        cleanup.push(refining.elapsed().as_secs_f64() * 1000.0);
        let guiding = Instant::now();
        if baseline {
            blocks::alpha(&mask, (spec.width, spec.height), (w, h),
                Region { x: 0, y: 0, w, h }, &curve, &mut alpha);
        } else { guided::alpha(
            &mask,
            (spec.width, spec.height),
            planes.y, w,
            (w, h),
            Region { x: 0, y: 0, w, h },
            &curve,
            &mut alpha,
        );
        }
        refinement.push(guiding.elapsed().as_secs_f64() * 1000.0);
        times.push(started.elapsed().as_secs_f64() * 1000.0);
        // Composite against the shipped studio picture at matching canvas size.
        for by in 0..h / 2 {
            for bx in 0..w / 2 {
                let a = alpha[by * w / 2 + bx] as u32;
                for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                    let i = (2 * by + dy) * w + 2 * bx + dx;
                    frame[i] = ((frame[i] as u32 * a + background[i] as u32 * (255 - a) + 127) / 255) as u8;
                }
                for offset in [w * h, w * h * 5 / 4] {
                    let i = offset + by * w / 2 + bx;
                    frame[i] = ((frame[i] as u32 * a + background[i] as u32 * (255 - a) + 127) / 255) as u8;
                }
            }
        }
        writer.write_all(&frame)?;
    }
    eprintln!("component cleanup mean {:.3} ms", cleanup.iter().sum::<f64>() / cleanup.len() as f64);
    let refine_mean = refinement.iter().sum::<f64>() / refinement.len() as f64;
    refinement.sort_by(f64::total_cmp);
    eprintln!("guided alpha: mean {refine_mean:.3} ms, median {:.3} ms, p95 {:.3} ms", refinement[refinement.len()/2], refinement[refinement.len()*95/100]);
    let mean = times.iter().sum::<f64>() / times.len() as f64;
    times.sort_by(f64::total_cmp);
    eprintln!("{} frames: mean {mean:.2} ms, median {:.2} ms, p95 {:.2} ms (sample, inference, smoothing, alpha)",times.len(),times[times.len()/2],times[times.len()*95/100]);
    Ok(())
}

#[cfg(not(feature = "matte"))]
fn main() {
    eprintln!("Enable the matte feature to evaluate a cutout");
}
