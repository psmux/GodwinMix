//! Build the keyed picture using the current frame as the edge guide.
use super::*;

impl State {
    pub(super) fn video_info(&mut self, pad: &gst::Pad) -> Option<gst_video::VideoInfo> {
        let caps = pad.current_caps()?;
        if let Some((have, info)) = &self.info {
            if have == &caps {
                return Some(info.clone());
            }
        }
        let info = gst_video::VideoInfo::from_caps(&caps).ok()?;
        if info.format() != gst_video::VideoFormat::I420 {
            return None;
        }
        self.info = Some((caps, info.clone()));
        Some(info)
    }

    fn alpha_into(&self, luma: &[u8], stride: usize, size: (usize, usize), r: Region, mask: Option<&super::super::worker::Mask>, out: &mut Vec<u8>) {
        match mask {
            Some(m) => super::super::guided::alpha(&m.data, (m.width, m.height), (luma, stride), size, r, &self.curve, out),
            None => blocks::alpha(&[], (0, 0), size, r, &self.curve, out),
        }
        if self.settings.feather > 0 {
            crate::plugin::filters::chroma::feather::soften(out, r.w / 2, r.h / 2, self.settings.feather.div_ceil(2) as usize);
        }
    }

    pub(super) fn picture(&mut self, frame: &gst::Buffer, info: &gst_video::VideoInfo, mask: Option<&super::super::worker::Mask>) -> Option<Picture> {
        let f = gst_video::VideoFrameRef::from_buffer_ref_readable(frame.as_ref(), info).ok()?;
        let s = info.stride();
        let strides = [s[0] as usize, s[1] as usize, s[2] as usize];
        let (w, h) = (info.width() as usize, info.height() as usize);
        let r = Region::of(w, h, &self.settings.matte);
        let (mut alpha, mut chroma) = self.reuse();
        self.alpha_into(f.plane_data(0).ok()?, strides[0], (w, h), r, mask, &mut alpha);
        blocks::chroma(f.plane_data(1).ok()?, f.plane_data(2).ok()?, strides, r, &mut chroma);
        let keyed = Arc::new(Keyed { frame: frame.clone(), info: info.clone(), region: (r.x, r.y, r.w, r.h), alpha, chroma });
        self.handed.push(keyed.clone());
        let whole = r == Region { x: 0, y: 0, w, h };
        let within = (!whole).then_some(Area { x: r.x as u32, y: r.y as u32, w: r.w as u32, h: r.h as u32 });
        Some(Picture {
            buffer: gst::Buffer::new(),
            width: r.w as u32,
            height: r.h as u32,
            stride: 0,
            natural: (w as u32, h as u32),
            content: None,
            within,
            keyed: Some(keyed),
        })
    }

    /// The block arrays of a picture the board has finished with.
    fn reuse(&mut self) -> (Vec<u8>, Vec<[u8; 2]>) {
        self.handed.truncate_front(3);
        let free = self.handed.iter().position(|k| Arc::strong_count(k) == 1);
        match free.map(|i| self.handed.remove(i)).and_then(|k| Arc::try_unwrap(k).ok()) {
            Some(k) => (k.alpha, k.chroma),
            None => (Vec::new(), Vec::new()),
        }
    }

    pub(super) fn flatten(&mut self, buffer: &mut gst::BufferRef, info: &gst_video::VideoInfo, mask: Option<&super::super::worker::Mask>) {
        let (w, h) = (info.width() as usize, info.height() as usize);
        let r = Region::of(w, h, &self.settings.matte);
        let mut alpha = std::mem::take(&mut self.flat);
        if let Ok(mut f) = gst_video::VideoFrameRef::from_buffer_ref_writable(buffer, info) {
            let s = info.stride();
            let strides = [s[0] as usize, s[1] as usize, s[2] as usize];
            let [y, u, v, _] = f.planes_data_mut();
            self.alpha_into(y, strides[0], (w, h), r, mask, &mut alpha);
            blocks::flatten(y, u, v, strides, (w, h), &alpha, r);
        }
        self.flat = alpha;
    }
}
