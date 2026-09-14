//! What the connect page may ask the shell to do. Two calls, and neither one
//! takes an address or a token from anywhere but the operator.

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use crate::core_link::{self, CoreInfo, Target};
use crate::settings::{self, Connection, Mode};
use crate::{sidecar, Shell};

/// What was remembered from last time, as the page needs it.
#[derive(Serialize)]
pub struct Remembered {
    mode: Mode,
    address: String,
    token: String,
    /// False on a first run, which is the difference between showing the
    /// dialog and going straight to the mixer.
    remembered: bool,
}

/// What the page asks for.
#[derive(Deserialize)]
pub struct Wanted {
    mode: Mode,
    #[serde(default)]
    address: String,
    #[serde(default)]
    token: String,
}

#[tauri::command]
pub fn saved_connection(app: AppHandle) -> Remembered {
    match settings::load(&app) {
        Some(c) => Remembered { mode: c.mode, address: c.address, token: c.token, remembered: true },
        None => {
            let d = Connection::default();
            Remembered { mode: d.mode, address: d.address, token: d.token, remembered: false }
        }
    }
}

/// Connect, and tell the page where to go.
///
/// Local means the mixer on this computer: started here if it is not already
/// running, on a port taken at start with a token from the application data
/// directory. Remote means one that is already running somewhere else, which
/// this app does not start and will not stop.
#[tauri::command]
pub async fn connect_core(app: AppHandle, wanted: Wanted) -> Result<CoreInfo, String> {
    let (target, label) = match wanted.mode {
        Mode::Local => (local_target(&app).await?, "this computer".to_string()),
        Mode::Remote => {
            let base = core_link::normalise(&wanted.address)?;
            let label = base.clone();
            (Target::new(base, wanted.token.trim()), label)
        }
    };

    let http = app.state::<Shell>().http.clone();
    let info = core_link::info(&http, &target, &label).await?;

    settings::save(
        &app,
        &Connection {
            mode: wanted.mode,
            address: if wanted.mode == Mode::Remote { target.base.clone() } else { String::new() },
            token: if wanted.mode == Mode::Remote { target.token.clone() } else { String::new() },
        },
    );
    *app.state::<Shell>().target.lock().unwrap() = Some(target);
    crate::ui::set_title(&app, &info);
    Ok(info)
}

/// The mixer on this computer: the one already running under this app, or a
/// new one.
async fn local_target(app: &AppHandle) -> Result<Target, String> {
    let running = {
        let held = app.state::<Shell>();
        let guard = held.local.lock().unwrap();
        guard.as_ref().filter(|l| l.is_running()).map(|l| l.target.clone())
    };
    if let Some(target) = running {
        return Ok(target);
    }
    let local = sidecar::ensure(app).await?;
    let target = local.target.clone();
    *app.state::<Shell>().local.lock().unwrap() = Some(local);
    Ok(target)
}
