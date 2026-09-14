//! The command line, parsed by hand.
//!
//! Six flags do not need an argument parser, and a terminal UI that runs on a
//! Raspberry Pi over SSH is the wrong place to spend a dependency. The names
//! and the environment variables are the ones `gmx ctl` uses, so an operator
//! who has already exported `GODWINMIX_URL` and `GODWINMIX_TOKEN` runs this
//! with no arguments at all.

use crate::client::{Config, MultiviewWant};
use crate::picture::Picture;

pub const USAGE: &str = "\
gmx-tui, the GodwinMix terminal UI

USAGE:
    gmx-tui [options]

OPTIONS:
    --url <url>        The mixer's control address [env: GODWINMIX_URL]
                       [default: http://127.0.0.1:8080]
    --token <token>    Bearer token, when the mixer has one [env: GODWINMIX_TOKEN]
    --multiview        Ask for the mosaic and draw it. Off by default: without
                       this flag the core is never asked for a picture and
                       builds no mosaic pipeline for this client.
    --fps <n>          Mosaic frames per second, 1 to 30 [default: 4]
    --width <px>       Mosaic width in pixels, 320 to 1920 [default: 320]
    --picture <how>    kitty, sixel or blocks. Detected when not given.
    --version          Print the version and exit
    -h, --help         Print this and exit

The keys are in docs/reference/keyboard.md, and ? shows them while it runs.
";

pub struct Args {
    pub config: Config,
    pub picture: Option<Picture>,
}

/// What to do instead of running, when the arguments say so.
pub enum Parsed {
    Run(Box<Args>),
    Print(String),
    Bad(String),
}

pub fn parse<I: IntoIterator<Item = String>>(argv: I) -> Parsed {
    let mut url = std::env::var("GODWINMIX_URL").ok();
    let mut token = std::env::var("GODWINMIX_TOKEN").ok().filter(|t| !t.is_empty());
    let mut multiview = false;
    let mut want = MultiviewWant::default();
    let mut picture = None;
    let mut args = argv.into_iter();
    while let Some(arg) = args.next() {
        let mut value = || args.next().ok_or_else(|| format!("{arg} needs a value after it"));
        let outcome: Result<(), String> = match arg.as_str() {
            "-h" | "--help" => return Parsed::Print(USAGE.to_string()),
            "--version" => {
                return Parsed::Print(format!("gmx-tui {}\n", env!("CARGO_PKG_VERSION")))
            }
            "--multiview" => {
                multiview = true;
                Ok(())
            }
            "--url" => value().map(|v| url = Some(v)),
            "--token" => value().map(|v| token = Some(v)),
            "--fps" => value().and_then(|v| number(&v, 1, 30)).map(|n| want.fps = n),
            "--width" => value().and_then(|v| number(&v, 320, 1920)).map(|n| want.width = n),
            "--picture" => value().and_then(|v| {
                picture = Picture::parse(&v);
                if picture.is_some() {
                    Ok(())
                } else {
                    Err(format!("--picture takes kitty, sixel, blocks or none, not '{v}'"))
                }
            }),
            other => Err(format!("no such option '{other}'. Run gmx-tui --help.")),
        };
        if let Err(message) = outcome {
            return Parsed::Bad(message);
        }
    }
    let url = crate::client::normalise_url(url.as_deref().unwrap_or("http://127.0.0.1:8080"));
    Parsed::Run(Box::new(Args {
        config: Config { url, token, multiview: multiview.then_some(want) },
        picture,
    }))
}

fn number(text: &str, low: u32, high: u32) -> Result<u32, String> {
    let n: u32 = text
        .parse()
        .map_err(|_| format!("'{text}' is not a whole number. It has to be {low} to {high}."))?;
    if n < low || n > high {
        return Err(format!("{n} is outside {low} to {high}."));
    }
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(args: &[&str]) -> Args {
        match parse(args.iter().map(|s| s.to_string())) {
            Parsed::Run(args) => *args,
            Parsed::Print(text) => panic!("printed instead: {text}"),
            Parsed::Bad(message) => panic!("refused: {message}"),
        }
    }

    #[test]
    fn the_default_is_a_local_mixer_with_no_picture() {
        let args = run(&[]);
        assert!(args.config.multiview.is_none());
        assert_eq!(args.picture, None);
        // GODWINMIX_URL may be set in the environment running the tests, so
        // the only safe assertion is that the socket ends up at /rpc.
        assert!(args.config.url.ends_with("/rpc"), "{}", args.config.url);
    }

    #[test]
    fn multiview_takes_its_numbers() {
        let args = run(&["--multiview", "--fps", "8", "--width", "640"]);
        let want = args.config.multiview.unwrap();
        assert_eq!((want.fps, want.width), (8, 640));
    }

    #[test]
    fn a_number_outside_the_range_is_refused_by_name() {
        match parse(["--fps".to_string(), "90".to_string()]) {
            Parsed::Bad(message) => assert!(message.contains("1 to 30"), "{message}"),
            _ => panic!("90 fps should have been refused"),
        }
    }

    #[test]
    fn the_url_is_normalised() {
        let args = run(&["--url", "box:8080"]);
        assert_eq!(args.config.url, "ws://box:8080/rpc");
    }
}
