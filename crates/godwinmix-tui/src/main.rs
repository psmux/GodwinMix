//! `gmx-tui`: the terminal in front of a mixer that may be a continent away.
//!
//! The loop is the one 05 section 2 asks for. State arrives as a snapshot and
//! then deltas; the screen is painted at `event/flush` and at nothing else the
//! mixer sends; keys turn into the same JSON-RPC calls the web UI makes.

use std::io::Write;
use std::time::Duration;

use crossterm::event::{Event, EventStream};
use futures_util::StreamExt;
use godwinmix_tui::app::{App, Mode};
use godwinmix_tui::args::{self, Parsed};
use godwinmix_tui::client::{self, Command, Incoming};
use godwinmix_tui::keys;
use godwinmix_tui::picture::{self, Picture};
use godwinmix_tui::term;
use godwinmix_tui::ui;
use ratatui::layout::Rect;
use tokio::sync::mpsc;

#[tokio::main(flavor = "current_thread")]
async fn main() -> std::io::Result<()> {
    let args = match args::parse(std::env::args().skip(1)) {
        Parsed::Run(args) => *args,
        Parsed::Print(text) => {
            print!("{text}");
            return Ok(());
        }
        Parsed::Bad(message) => {
            eprintln!("gmx-tui: {message}");
            std::process::exit(2);
        }
    };
    // rustls takes its crypto provider from the process, and tokio-tungstenite
    // builds its client config without naming one. Installing it here is what
    // makes a wss:// mixer reachable; ws:// never touches it.
    let _ = rustls::crypto::ring::default_provider().install_default();

    let wants_picture = args.config.multiview.is_some();
    let (command_tx, command_rx) = mpsc::channel::<Command>(32);
    let (incoming_tx, mut incoming_rx) = mpsc::channel::<Incoming>(256);
    let link = tokio::spawn(client::run(args.config.clone(), command_rx, incoming_tx));

    let mut terminal = ratatui::init();
    // Asked once, with the terminal already in raw mode, because the answer
    // arrives on stdin as bytes rather than as a key press.
    let kind = if wants_picture { term::detect(args.picture) } else { Picture::None };
    let outcome = run(&mut terminal, App::new(&args.config), command_tx, &mut incoming_rx, kind).await;
    ratatui::restore();
    if kind == Picture::Kitty {
        // Leave no images behind on the terminal the operator goes back to.
        let _ = std::io::stdout().write_all(b"\x1b_Ga=d,d=A\x1b\\");
    }
    link.abort();
    outcome
}

async fn run(
    terminal: &mut ratatui::DefaultTerminal,
    mut app: App,
    commands: mpsc::Sender<Command>,
    incoming: &mut mpsc::Receiver<Incoming>,
    kind: Picture,
) -> std::io::Result<()> {
    let mut keyboard = EventStream::new();
    let mut tick = tokio::time::interval(Duration::from_millis(500));
    let mut painted_frame = u64::MAX;
    loop {
        if app.dirty {
            app.dirty = false;
            let mut image_area: Option<Rect> = None;
            terminal.draw(|frame| image_area = ui::draw(frame, &app, kind))?;
            // The escape written picture goes on after the widgets, because
            // ratatui knows nothing about the cells a graphics protocol owns.
            if let (Some(area), true) = (image_area, kind.is_escape()) {
                if painted_frame != app.frames_seen {
                    painted_frame = app.frames_seen;
                    paint(area, &app, kind)?;
                }
            }
        }
        tokio::select! {
            event = keyboard.next() => match event {
                None => break,
                Some(Err(e)) => return Err(e),
                Some(Ok(Event::Key(key))) => {
                    let action = match app.mode {
                        Mode::Filter | Mode::AdUri => keys::typing(key),
                        _ => keys::normal(key),
                    };
                    if let Some(command) = app.act(action) {
                        if commands.send(command).await.is_err() {
                            break;
                        }
                    }
                    app.dirty = true;
                    if app.quit {
                        break;
                    }
                }
                Some(Ok(Event::Resize(_, _))) => {
                    app.dirty = true;
                    painted_frame = u64::MAX;
                }
                Some(Ok(_)) => {}
            },
            message = incoming.recv() => match message {
                None => break,
                Some(message) => {
                    if let Some(command) = app.on_incoming(message) {
                        if commands.send(command).await.is_err() {
                            break;
                        }
                    }
                }
            },
            _ = tick.tick() => app.dirty = true,
        }
    }
    Ok(())
}

/// One mosaic frame, straight to the terminal.
fn paint(area: Rect, app: &App, kind: Picture) -> std::io::Result<()> {
    let Some(escapes) = picture::escapes(app, area, kind) else { return Ok(()) };
    let mut out = std::io::stdout();
    crossterm::queue!(out, crossterm::cursor::SavePosition, crossterm::cursor::MoveTo(area.x, area.y))?;
    out.write_all(escapes.as_bytes())?;
    crossterm::queue!(out, crossterm::cursor::RestorePosition)?;
    out.flush()
}
