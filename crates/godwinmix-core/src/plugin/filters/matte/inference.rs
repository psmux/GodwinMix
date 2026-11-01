//! Model preparation and inference on the cutout worker.
use super::*;

/// The session in use and what it was opened for.
pub(super) struct Open {
    pub(super) session: ort::session::Session,
    pub(super) spec: Spec,
    pub(super) for_settings: (super::super::params::Quality, super::super::params::Device),
}


use crate::plugin::filters::matte::sample::Planes;

pub(super) fn prepare(settings: &Settings) -> anyhow::Result<(ort::session::Session, Spec, Ran)> {
    let gpu = model::wants_gpu(settings.device) && runtime::has_gpu(settings.device);
    let spec = model::choose(&settings.quality, gpu)?;
    let (session, ran) = runtime::session(&spec, settings.device)?;
    Ok((session, spec, ran))
}

/// One frame through the model. `None` when the frame cannot be read.
pub(super) fn infer(o: &mut Open, frame: &gst::Buffer, info: &gst_video::VideoInfo, input: &mut Vec<f32>) -> Option<Vec<f32>> {
    let f = gst_video::VideoFrameRef::from_buffer_ref_readable(frame.as_ref(), info).ok()?;
    let s = info.stride();
    let planes = Planes {
        y: f.plane_data(0).ok()?,
        u: f.plane_data(1).ok()?,
        v: f.plane_data(2).ok()?,
        strides: [s[0] as usize, s[1] as usize, s[2] as usize],
        width: info.width() as usize,
        height: info.height() as usize,
    };
    sample::input(&planes, &o.spec, input);
    let shape = [1usize, 3, o.spec.height, o.spec.width];
    let tensor = ort::value::TensorRef::from_array_view((shape, input.as_slice())).ok()?;
    let outputs = match o.session.run(ort::inputs![tensor]) {
        Ok(outputs) => outputs,
        Err(e) => {
            tracing::warn!(error = %e, "the cutout model failed on a frame");
            return None;
        }
    };
    let (_, data) = outputs[0].try_extract_tensor::<f32>().ok()?;
    // A model may answer at another size; only one the size of its input is
    // used, which every model this reads does.
    (data.len() == o.spec.width * o.spec.height).then(|| data.to_vec())
}


pub(super) fn wait_for_change(shared: &Shared, from: &(super::super::params::Quality, super::super::params::Device)) {
    while !shared.stop.load(Ordering::SeqCst) {
        let now = shared.settings.lock().clone();
        if (now.quality.clone(), now.device) != *from {
            return;
        }
        let mut job = shared.job.lock();
        job.take();
        shared.wake.wait_for(&mut job, std::time::Duration::from_secs(1));
    }
}
