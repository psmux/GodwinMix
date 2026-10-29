//! Load, play, pause, speed, seek and the look: the moves every screen follows.

use super::{check, Inner, Prompter};
use godwinmix_protocol::error::RpcError;
use godwinmix_protocol::prompter::{LoadRequest, LookRequest, PrompterState, SeekRequest, SpeedRequest};

impl Prompter {
    pub fn load(&self, req: LoadRequest) -> Result<PrompterState, RpcError> {
        self.script(&req.id)?;
        let (_, state) = self.change(true, |inner| {
            inner.on = Some(req.id.clone());
            inner.position = 0.0;
            inner.playing = req.play;
            inner.rebuild();
        });
        Ok(state)
    }

    pub fn play(&self, speed: Option<f64>) -> Result<PrompterState, RpcError> {
        self.loaded("prompter.play")?;
        let speed = speed.map(check::speed).transpose()?;
        let (_, state) = self.change(false, |inner| {
            inner.speed = speed.unwrap_or(inner.speed);
            // Play from the end goes back to the top, as a tape would.
            if inner.speed > 0.0 && inner.position >= inner.rows.len() as f64 {
                inner.position = 0.0;
            }
            inner.playing = true;
        });
        Ok(state)
    }

    pub fn pause(&self) -> PrompterState {
        self.change(true, |inner| inner.playing = false).1
    }

    pub fn speed(&self, req: SpeedRequest) -> Result<PrompterState, RpcError> {
        let now = self.inner.lock().speed;
        let speed = match (req.speed, req.by) {
            (Some(s), None) => s,
            (None, Some(by)) => ((now + by) * 1000.0).round() / 1000.0,
            _ => return Err(RpcError::invalid_params("prompter.speed takes one of speed (rows a second) or by (added to the speed it has), not both and not neither.".to_string()).with("speed", now)),
        };
        let speed = check::speed(speed)?;
        Ok(self.change(false, |inner| inner.speed = speed).1)
    }

    pub fn seek(&self, req: SeekRequest) -> Result<PrompterState, RpcError> {
        self.loaded("prompter.seek")?;
        let named = [req.row.is_some(), req.fraction.is_some(), req.marker.is_some(), req.step.is_some(), req.by.is_some()];
        if named.iter().filter(|n| **n).count() != 1 {
            return Err(RpcError::invalid_params(
                "prompter.seek takes exactly one of row, fraction, marker, step or by. To go to the top send {row: 0}.".to_string(),
            ));
        }
        let to = {
            let mut inner = self.inner.lock();
            inner.settle();
            target(&inner, &req)?
        };
        Ok(self.change(true, |inner| inner.position = to.clamp(0.0, inner.rows.len() as f64)).1)
    }

    pub fn set_look(&self, req: LookRequest) -> Result<PrompterState, RpcError> {
        let look = check::look(self.inner.lock().look, &req)?;
        let (_, state) = self.change(true, |inner| {
            let place = inner.place();
            let wider = look.line_chars != inner.look.line_chars;
            inner.look = look;
            if wider {
                inner.rebuild();
                inner.position = inner.row_of(place);
            }
        });
        Ok(state)
    }

    fn loaded(&self, method: &str) -> Result<(), RpcError> {
        match self.inner.lock().on {
            Some(_) => Ok(()),
            None => Err(check::nothing_loaded(method, self.ids())),
        }
    }
}

/// Where a seek lands, in rows.
fn target(inner: &Inner, req: &SeekRequest) -> Result<f64, RpcError> {
    let rows = inner.rows.len() as f64;
    if let Some(row) = req.row {
        return finite("row", row);
    }
    if let Some(f) = req.fraction {
        return Ok(finite("fraction", f)?.clamp(0.0, 1.0) * rows);
    }
    if let Some(by) = req.by {
        return Ok(inner.position + finite("by", by)?);
    }
    if let Some(step) = req.step {
        return Ok(stepped(inner, step));
    }
    let name = req.marker.as_deref().unwrap_or("").trim();
    let by_number = name.parse::<usize>().ok().and_then(|n| n.checked_sub(1)).and_then(|n| inner.markers.get(n));
    let found = by_number.or_else(|| inner.markers.iter().find(|m| m.name.eq_ignore_ascii_case(name)));
    match found {
        Some(m) => Ok(m.row as f64),
        None => {
            let names: Vec<String> = inner.markers.iter().map(|m| m.name.clone()).collect();
            let listed = if names.is_empty() { "This script has no markers: a line that starts with # is one.".to_string() } else { format!("Its markers are: {}.", names.join(", ")) };
            Err(RpcError::new(godwinmix_protocol::error::ErrorCode::NotFound, format!("there is no marker '{name}' in this script. {listed}"))
                .with("field", "marker")
                .with("markers", names))
        }
    }
}

fn finite(name: &str, value: f64) -> Result<f64, RpcError> {
    if value.is_finite() {
        return Ok(value);
    }
    Err(RpcError::invalid_params(format!("{name} has to be a number.")).with("field", name.to_string()))
}

/// `step` markers on from where the prompter is. Back from a little way into
/// a section is the start of that section, which is what a pedal tap means.
fn stepped(inner: &Inner, step: i32) -> f64 {
    let here = inner.position;
    let starts: Vec<f64> = inner.markers.iter().map(|m| m.row as f64).collect();
    if step > 0 {
        let ahead: Vec<f64> = starts.iter().copied().filter(|r| *r > here + 0.01).collect();
        return ahead.get(step as usize - 1).copied().unwrap_or(inner.rows.len() as f64);
    }
    let behind: Vec<f64> = starts.iter().copied().filter(|r| *r < here - 0.5).collect();
    let back = (-step) as usize;
    if back > behind.len() { 0.0 } else { behind[behind.len() - back] }
}

impl Inner {
    /// The line as written that the position is in, and how far through it.
    fn place(&self) -> (u32, f64) {
        let at = (self.position.floor() as usize).min(self.rows.len().saturating_sub(1));
        let Some(row) = self.rows.get(at) else { return (0, 0.0) };
        let first = self.rows.iter().position(|r| r.line == row.line).unwrap_or(at);
        let count = self.rows.iter().filter(|r| r.line == row.line).count().max(1);
        (row.line, ((self.position - first as f64) / count as f64).clamp(0.0, 1.0))
    }

    /// The same place in rows made at another width.
    fn row_of(&self, (line, through): (u32, f64)) -> f64 {
        let Some(first) = self.rows.iter().position(|r| r.line == line) else { return 0.0 };
        let count = self.rows.iter().filter(|r| r.line == line).count();
        first as f64 + through * count as f64
    }
}
