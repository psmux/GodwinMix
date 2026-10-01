//! Where this user's GodwinMix files live, worked out in one place.
//!
//! Everything per user hangs off one folder: `GODWINMIX_HOME` when it is set,
//! `~/.godwinmix` otherwise. The plugins, the secret store, the frame bus
//! registry, saved presets, the marketplace list and the installed codec
//! catalogue are all under it. A path that worked this out for itself from
//! `HOME` is how a scratch `GODWINMIX_HOME` once installed a plugin into the
//! operator's real one, so nothing else should read `HOME` for these.
//!
//! The config's `[control] plugins_dir` and `GODWINMIX_PLUGINS_DIR` still win
//! for the plugins folder, in that order; they name a folder on purpose.

use std::ffi::OsString;
use std::path::PathBuf;

/// The variable that moves every per user path at once.
pub const HOME_ENV: &str = "GODWINMIX_HOME";
/// The variable that moves only the plugins folder.
pub const PLUGINS_ENV: &str = "GODWINMIX_PLUGINS_DIR";

/// `GODWINMIX_HOME`, else `~/.godwinmix`.
pub fn dir() -> PathBuf {
    dir_from(&env)
}

/// Where plugins are installed and read: `GODWINMIX_PLUGINS_DIR`, else
/// `plugins` under [`dir`]. A config's `plugins_dir` overrides both; the
/// loader applies it at startup.
pub fn plugins_dir() -> PathBuf {
    plugins_from(&env)
}

/// The secret store: `secrets` under [`dir`].
pub fn secrets_dir() -> PathBuf {
    dir().join("secrets")
}

/// Presets an operator saved: `presets` under [`dir`].
pub fn presets_dir() -> PathBuf {
    dir().join("presets")
}

/// A folder a config names, with a leading `~` read as the user's home, the
/// way a shell would have. Anything else is taken as written.
pub fn expand(path: &str) -> PathBuf {
    expand_from(path, &env)
}

fn env(key: &str) -> Option<OsString> {
    std::env::var_os(key).filter(|v| !v.is_empty())
}

fn user_home(get: &dyn Fn(&str) -> Option<OsString>) -> PathBuf {
    get("HOME").or_else(|| get("USERPROFILE")).map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."))
}

fn dir_from(get: &dyn Fn(&str) -> Option<OsString>) -> PathBuf {
    match get(HOME_ENV) {
        Some(explicit) => PathBuf::from(explicit),
        None => user_home(get).join(".godwinmix"),
    }
}

fn plugins_from(get: &dyn Fn(&str) -> Option<OsString>) -> PathBuf {
    match get(PLUGINS_ENV) {
        Some(explicit) => PathBuf::from(explicit),
        None => dir_from(get).join("plugins"),
    }
}

fn expand_from(path: &str, get: &dyn Fn(&str) -> Option<OsString>) -> PathBuf {
    match path.strip_prefix("~/").or_else(|| path.strip_prefix("~\\")) {
        Some(rest) => user_home(get).join(rest),
        None if path == "~" => user_home(get),
        None => PathBuf::from(path),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with(pairs: &'static [(&'static str, &'static str)]) -> impl Fn(&str) -> Option<OsString> {
        move |k| pairs.iter().find(|(key, _)| *key == k).map(|(_, v)| OsString::from(v))
    }

    #[test]
    fn godwinmix_home_moves_every_per_user_path() {
        let get = with(&[("HOME", "/home/real"), (HOME_ENV, "/scratch/gm")]);
        assert_eq!(dir_from(&get), PathBuf::from("/scratch/gm"));
        assert_eq!(plugins_from(&get), PathBuf::from("/scratch/gm/plugins"));
    }

    #[test]
    fn without_it_the_home_folder_is_used_and_the_plugins_variable_still_wins() {
        let get = with(&[("HOME", "/home/real")]);
        assert_eq!(dir_from(&get), PathBuf::from("/home/real/.godwinmix"));
        assert_eq!(plugins_from(&get), PathBuf::from("/home/real/.godwinmix/plugins"));
        let get = with(&[("HOME", "/home/real"), (HOME_ENV, "/s"), (PLUGINS_ENV, "/p")]);
        assert_eq!(plugins_from(&get), PathBuf::from("/p"));
    }

    #[test]
    fn a_tilde_in_a_config_path_is_the_users_home() {
        let get = with(&[("HOME", "/home/real")]);
        assert_eq!(expand_from("~/x/plugins", &get), PathBuf::from("/home/real/x/plugins"));
        assert_eq!(expand_from("/abs", &get), PathBuf::from("/abs"));
        assert_eq!(expand_from("rel/~", &get), PathBuf::from("rel/~"));
    }
}
