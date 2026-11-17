//! Now Playing, from Windows' media session manager. It reports whatever is
//! playing in any app that publishes media controls (the Spotify desktop app,
//! browsers, most players) with no API keys.
//!
//! Entirely event-driven: one thread owns the session objects and sleeps on a
//! channel until Windows raises a change event or the UI sends a command.

use std::sync::OnceLock;
use std::sync::mpsc::{Receiver, Sender, channel};

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};
use windows::Foundation::TypedEventHandler;
use windows::Media::Control::{
    GlobalSystemMediaTransportControlsSession as Session,
    GlobalSystemMediaTransportControlsSessionManager as Manager,
    GlobalSystemMediaTransportControlsSessionMediaProperties as MediaProperties,
    GlobalSystemMediaTransportControlsSessionPlaybackStatus as PlaybackStatus,
};
use windows::Media::MediaPlaybackAutoRepeatMode;
use windows::Storage::Streams::DataReader;
use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx};
use windows::core::Interface;
use windows_future::IAsyncOperation;

/// 100-nanosecond ticks per millisecond, the unit of WinRT time values.
const TICKS_PER_MS: f64 = 10_000.0;
/// Ticks between 1601-01-01 (WinRT's epoch) and 1970-01-01 (Unix).
const UNIX_EPOCH_TICKS: i64 = 116_444_736_000_000_000;
/// Album art larger than this is ignored rather than loaded into memory.
const MAX_ART_BYTES: u64 = 8 * 1024 * 1024;

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaState {
    /// False when nothing is publishing media controls.
    pub available: bool,
    /// The app the session belongs to.
    pub app: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub playing: bool,
    /// Position when it was last reported, and when that was (Unix ms). The
    /// UI advances it locally while playing, so nothing polls.
    pub position_ms: f64,
    pub duration_ms: f64,
    pub updated_at_ms: f64,
    pub can_toggle: bool,
    pub can_next: bool,
    pub can_previous: bool,
    pub can_seek: bool,
    /// `None` when the session doesn't support the control.
    pub shuffle: Option<bool>,
    /// "none", "track" or "list".
    pub repeat: Option<String>,
    /// Changes whenever the album art does.
    pub art_rev: u32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum MediaCommand {
    Toggle,
    Next,
    Previous,
    #[serde(rename_all = "camelCase")]
    Seek {
        position_ms: f64,
    },
    Shuffle {
        on: bool,
    },
    Repeat {
        mode: String,
    },
}

enum Message {
    /// The set of sessions, or which one is current, changed.
    Sessions,
    /// Title, artist or artwork changed.
    Properties,
    /// Play state, controls or position changed.
    Playback,
    Command(MediaCommand),
}

#[derive(Default)]
struct Shared {
    state: MediaState,
    art: Vec<u8>,
}

static SHARED: Mutex<Option<Shared>> = Mutex::new(None);
static SENDER: OnceLock<Mutex<Sender<Message>>> = OnceLock::new();

pub fn state() -> MediaState {
    SHARED
        .lock()
        .as_ref()
        .map(|s| s.state.clone())
        .unwrap_or_default()
}

/// The current album art (JPEG or PNG bytes as the app supplied them).
pub fn art() -> Vec<u8> {
    SHARED
        .lock()
        .as_ref()
        .map(|s| s.art.clone())
        .unwrap_or_default()
}

pub fn command(command: MediaCommand) {
    if let Some(sender) = SENDER.get() {
        let _ = sender.lock().send(Message::Command(command));
    }
}

pub fn start(app: &AppHandle) {
    let (tx, rx) = channel();
    if SENDER.set(Mutex::new(tx.clone())).is_err() {
        return;
    }
    *SHARED.lock() = Some(Shared::default());
    let app = app.clone();
    let spawned = std::thread::Builder::new()
        .name("media".into())
        .spawn(move || {
            unsafe {
                let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            }
            if let Err(e) = run(&app, tx, rx) {
                log::warn!("media: session tracking stopped: {e}");
            }
        });
    if let Err(e) = spawned {
        log::warn!("media: could not start: {e}");
    }
}

/// Pick the session to show: Spotify if it has one, otherwise the session
/// Windows considers current.
fn pick_session(manager: &Manager) -> Option<Session> {
    let sessions = manager.GetSessions().ok()?;
    let spotify = sessions.into_iter().find(|s| {
        s.SourceAppUserModelId()
            .is_ok_and(|id| id.to_string().to_ascii_lowercase().contains("spotify"))
    });
    spotify.or_else(|| manager.GetCurrentSession().ok())
}

struct Tracked {
    session: Session,
    tokens: [i64; 3],
}

impl Tracked {
    fn new(session: Session, tx: &Sender<Message>) -> Self {
        let notify = |tx: &Sender<Message>, make: fn() -> Message| {
            let tx = tx.clone();
            move || {
                let _ = tx.send(make());
            }
        };
        let properties = notify(tx, || Message::Properties);
        let playback = notify(tx, || Message::Playback);
        let timeline = notify(tx, || Message::Playback);
        let tokens = [
            session
                .MediaPropertiesChanged(&TypedEventHandler::new(move |_, _| {
                    properties();
                    Ok(())
                }))
                .unwrap_or(0),
            session
                .PlaybackInfoChanged(&TypedEventHandler::new(move |_, _| {
                    playback();
                    Ok(())
                }))
                .unwrap_or(0),
            session
                .TimelinePropertiesChanged(&TypedEventHandler::new(move |_, _| {
                    timeline();
                    Ok(())
                }))
                .unwrap_or(0),
        ];
        Self { session, tokens }
    }
}

impl Drop for Tracked {
    fn drop(&mut self) {
        let _ = self.session.RemoveMediaPropertiesChanged(self.tokens[0]);
        let _ = self.session.RemovePlaybackInfoChanged(self.tokens[1]);
        let _ = self.session.RemoveTimelinePropertiesChanged(self.tokens[2]);
    }
}

fn read_art(properties: &MediaProperties) -> Option<Vec<u8>> {
    let stream = properties
        .Thumbnail()
        .ok()?
        .OpenReadAsync()
        .ok()?
        .join()
        .ok()?;
    let size = stream.Size().ok()?;
    if size == 0 || size > MAX_ART_BYTES {
        return None;
    }
    let reader = DataReader::CreateDataReader(&stream).ok()?;
    reader
        .LoadAsync(size as u32)
        .ok()?
        .cast::<IAsyncOperation<u32>>()
        .ok()?
        .join()
        .ok()?;
    let mut bytes = vec![0u8; size as usize];
    reader.ReadBytes(&mut bytes).ok()?;
    Some(bytes)
}

fn ticks_to_ms(ticks: i64) -> f64 {
    ticks as f64 / TICKS_PER_MS
}

/// Read everything except the artwork from a session into `state`.
fn read_session(session: &Session, state: &mut MediaState) -> Option<MediaProperties> {
    state.available = true;
    state.app = session
        .SourceAppUserModelId()
        .map(|s| s.to_string())
        .unwrap_or_default();

    if let Ok(info) = session.GetPlaybackInfo() {
        state.playing = info
            .PlaybackStatus()
            .is_ok_and(|s| s == PlaybackStatus::Playing);
        if let Ok(controls) = info.Controls() {
            let on = |value: windows::core::Result<bool>| value.unwrap_or(false);
            state.can_toggle = on(controls.IsPlayPauseToggleEnabled())
                || on(controls.IsPlayEnabled())
                || on(controls.IsPauseEnabled());
            state.can_next = on(controls.IsNextEnabled());
            state.can_previous = on(controls.IsPreviousEnabled());
            state.can_seek = on(controls.IsPlaybackPositionEnabled());
            state.shuffle = on(controls.IsShuffleEnabled()).then(|| {
                info.IsShuffleActive()
                    .and_then(|v| v.Value())
                    .unwrap_or(false)
            });
            state.repeat = on(controls.IsRepeatEnabled()).then(|| {
                match info.AutoRepeatMode().and_then(|v| v.Value()) {
                    Ok(MediaPlaybackAutoRepeatMode::Track) => "track",
                    Ok(MediaPlaybackAutoRepeatMode::List) => "list",
                    _ => "none",
                }
                .to_string()
            });
        }
    }

    if let Ok(timeline) = session.GetTimelineProperties() {
        let start = timeline.StartTime().map(|t| t.Duration).unwrap_or(0);
        let end = timeline.EndTime().map(|t| t.Duration).unwrap_or(0);
        let position = timeline.Position().map(|t| t.Duration).unwrap_or(0);
        state.duration_ms = ticks_to_ms((end - start).max(0));
        state.position_ms = ticks_to_ms((position - start).max(0));
        state.updated_at_ms = timeline
            .LastUpdatedTime()
            .map(|t| ticks_to_ms(t.UniversalTime - UNIX_EPOCH_TICKS))
            .unwrap_or(0.0);
    }

    let properties = session.TryGetMediaPropertiesAsync().ok()?.join().ok()?;
    state.title = properties
        .Title()
        .map(|s| s.to_string())
        .unwrap_or_default();
    state.artist = properties
        .Artist()
        .map(|s| s.to_string())
        .unwrap_or_default();
    state.album = properties
        .AlbumTitle()
        .map(|s| s.to_string())
        .unwrap_or_default();
    Some(properties)
}

fn execute(session: &Session, command: MediaCommand) {
    let result = match command {
        MediaCommand::Toggle => session.TryTogglePlayPauseAsync(),
        MediaCommand::Next => session.TrySkipNextAsync(),
        MediaCommand::Previous => session.TrySkipPreviousAsync(),
        MediaCommand::Seek { position_ms } => {
            session.TryChangePlaybackPositionAsync((position_ms.max(0.0) * TICKS_PER_MS) as i64)
        }
        MediaCommand::Shuffle { on } => session.TryChangeShuffleActiveAsync(on),
        MediaCommand::Repeat { mode } => {
            session.TryChangeAutoRepeatModeAsync(match mode.as_str() {
                "track" => MediaPlaybackAutoRepeatMode::Track,
                "list" => MediaPlaybackAutoRepeatMode::List,
                _ => MediaPlaybackAutoRepeatMode::None,
            })
        }
    };
    // The outcome arrives as a change event; a refusal just changes nothing.
    if let Ok(operation) = result {
        let _ = operation.join();
    }
}

fn run(app: &AppHandle, tx: Sender<Message>, rx: Receiver<Message>) -> windows::core::Result<()> {
    let manager = Manager::RequestAsync()?.join()?;
    let sessions_tx = tx.clone();
    manager.SessionsChanged(&TypedEventHandler::new(move |_, _| {
        let _ = sessions_tx.send(Message::Sessions);
        Ok(())
    }))?;
    let current_tx = tx.clone();
    manager.CurrentSessionChanged(&TypedEventHandler::new(move |_, _| {
        let _ = current_tx.send(Message::Sessions);
        Ok(())
    }))?;

    let mut tracked: Option<Tracked> = None;
    let mut reselect = true;
    let mut reload_art = true;
    loop {
        if reselect {
            let picked = pick_session(&manager);
            let same = match (&tracked, &picked) {
                (Some(t), Some(p)) => t.session == *p,
                (None, None) => true,
                _ => false,
            };
            if !same {
                tracked = picked.map(|session| Tracked::new(session, &tx));
                reload_art = true;
            }
            reselect = false;
        }

        let previous = state();
        let mut next = MediaState {
            art_rev: previous.art_rev,
            ..Default::default()
        };
        let mut new_art: Option<Vec<u8>> = None;
        if let Some(tracked) = &tracked {
            let properties = read_session(&tracked.session, &mut next);
            let track_changed = (
                next.title.as_str(),
                next.artist.as_str(),
                next.album.as_str(),
            ) != (
                previous.title.as_str(),
                previous.artist.as_str(),
                previous.album.as_str(),
            );
            if reload_art || track_changed {
                new_art = Some(properties.as_ref().and_then(read_art).unwrap_or_default());
            }
        } else {
            new_art = Some(Vec::new());
        }
        reload_art = false;

        {
            let mut shared = SHARED.lock();
            let shared = shared.get_or_insert_with(Shared::default);
            if let Some(art) = new_art
                && art != shared.art
            {
                shared.art = art;
                next.art_rev = previous.art_rev.wrapping_add(1);
            }
            shared.state = next.clone();
        }
        if next != previous {
            let _ = app.emit("media://changed", next);
        }

        // Sleep until something happens, then fold a burst into one pass.
        let Ok(first) = rx.recv() else { return Ok(()) };
        for message in std::iter::once(first).chain(rx.try_iter()) {
            match message {
                Message::Sessions => reselect = true,
                Message::Properties => reload_art = true,
                Message::Playback => {}
                Message::Command(command) => {
                    if let Some(tracked) = &tracked {
                        execute(&tracked.session, command);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_winrt_ticks_to_milliseconds() {
        assert_eq!(ticks_to_ms(10_000), 1.0);
        assert_eq!(ticks_to_ms(215 * 10_000_000), 215_000.0);
        // 1970-01-01 in WinRT ticks is Unix time zero.
        assert_eq!(ticks_to_ms(UNIX_EPOCH_TICKS - UNIX_EPOCH_TICKS), 0.0);
    }

    #[test]
    fn commands_deserialize_from_the_ui_shape() {
        let seek: MediaCommand =
            serde_json::from_str(r#"{"type":"seek","positionMs":42000}"#).unwrap();
        assert!(matches!(seek, MediaCommand::Seek { position_ms } if position_ms == 42000.0));
        let toggle: MediaCommand = serde_json::from_str(r#"{"type":"toggle"}"#).unwrap();
        assert!(matches!(toggle, MediaCommand::Toggle));
        let repeat: MediaCommand =
            serde_json::from_str(r#"{"type":"repeat","mode":"track"}"#).unwrap();
        assert!(matches!(repeat, MediaCommand::Repeat { mode } if mode == "track"));
    }

    #[test]
    fn default_state_means_nothing_is_playing() {
        let state = MediaState::default();
        assert!(!state.available && !state.playing);
        assert_eq!(state.shuffle, None);
    }
}
