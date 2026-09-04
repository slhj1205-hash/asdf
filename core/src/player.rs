use std::{path::Path, time::Duration};

use crate::song::Song;

pub type BackendError = Box<dyn std::error::Error + Send + Sync + 'static>;

pub trait AudioBackend {
    fn describe(&self) -> &'static str;

    fn play_uri(&mut self, uri: &str) -> Result<(), BackendError>;
    fn pause(&mut self) -> Result<(), BackendError>;
    fn resume(&mut self) -> Result<(), BackendError>;
    fn stop(&mut self) -> Result<(), BackendError>;

    fn set_volume(&mut self, volume: f64);
    fn volume(&self) -> f64;

    fn position(&self) -> Option<Duration>;
    fn duration(&self) -> Option<Duration>;
    fn seek(&mut self, position: Duration) -> Result<(), BackendError>;

    fn poll_events(&mut self) -> Vec<BackendEvent>;
}

#[derive(Debug)]
pub enum BackendEvent {
    EndOfStream,
    Error(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaybackState {
    Idle,
    Playing,
    Paused,
}

#[derive(Debug)]
pub enum PlayerEvent {
    SongEnded,
    Error(String),
    PositionTick(Duration),
    StateChanged(PlaybackState),
}

pub struct Player<B: AudioBackend> {
    backend: B,
    state: PlaybackState,
    pending: Vec<PlayerEvent>,
    last_position_poll: Option<std::time::Instant>,
}

const POSITION_POLL_INTERVAL: Duration = Duration::from_millis(100);

impl<B: AudioBackend> Player<B> {
    pub fn new(backend: B) -> Player<B> {
        Player {
            backend,
            state: PlaybackState::Idle,
            pending: Vec::new(),
            last_position_poll: None,
        }
    }

    pub fn play(&mut self, song: &Song) -> Result<(), Error> {
        let uri = path_to_uri(song.path())?;

        if let Err(e) = self.backend.play_uri(&uri) {
            self.set_state(PlaybackState::Idle);
            return Err(Error::Backend(e));
        }

        self.set_state(PlaybackState::Playing);
        Ok(())
    }

    pub fn toggle(&mut self) -> Result<(), Error> {
        match self.state {
            PlaybackState::Idle => Ok(()),
            PlaybackState::Playing => {
                self.backend.pause().map_err(Error::Backend)?;
                self.set_state(PlaybackState::Paused);
                Ok(())
            }
            PlaybackState::Paused => {
                self.backend.resume().map_err(Error::Backend)?;
                self.set_state(PlaybackState::Playing);
                Ok(())
            }
        }
    }

    pub fn stop(&mut self) -> Result<(), Error> {
        if self.state == PlaybackState::Idle {
            return Ok(());
        }
        let result = self.backend.stop().map_err(Error::Backend);
        self.set_state(PlaybackState::Idle);
        result
    }

    pub fn poll_events(&mut self) -> Vec<PlayerEvent> {
        let mut out = std::mem::take(&mut self.pending);

        for event in self.backend.poll_events() {
            match event {
                BackendEvent::EndOfStream => {
                    self.set_state(PlaybackState::Idle);
                    out.push(PlayerEvent::SongEnded);
                }
                BackendEvent::Error(text) => {
                    self.set_state(PlaybackState::Idle);
                    out.push(PlayerEvent::Error(text));
                }
            }
        }

        if self.state != PlaybackState::Idle
            && self.position_poll_due()
            && let Some(pos) = self.backend.position()
        {
            out.push(PlayerEvent::PositionTick(pos));
        }

        out
    }

    fn position_poll_due(&mut self) -> bool {
        let now = std::time::Instant::now();
        match self.last_position_poll {
            Some(last) if now.duration_since(last) < POSITION_POLL_INTERVAL => false,
            _ => {
                self.last_position_poll = Some(now);
                true
            }
        }
    }

    fn set_state(&mut self, new_state: PlaybackState) {
        if self.state != new_state {
            self.state = new_state;
            self.pending.push(PlayerEvent::StateChanged(new_state));
        }
    }

    pub fn state(&self) -> PlaybackState {
        self.state
    }
    pub fn describe_backend(&self) -> &'static str {
        self.backend.describe()
    }
    pub fn set_volume(&mut self, volume: f64) {
        let volume = volume.clamp(0.0, 1.0);
        self.backend.set_volume(volume);
    }
    pub fn adjust_volume(&mut self, delta: f64) {
        let new_volume = (self.volume() + delta).clamp(0.0, 1.0);
        self.set_volume(new_volume);
    }
    pub fn volume(&self) -> f64 {
        self.backend.volume()
    }

    pub fn position(&self) -> Option<Duration> {
        self.backend.position()
    }
    pub fn duration(&self) -> Option<Duration> {
        self.backend.duration()
    }
    pub fn seek(&mut self, position: Duration) -> Result<(), Error> {
        self.backend.seek(position).map_err(Error::Backend)
    }
}

fn path_bytes(path: &Path) -> Vec<u8> {
    use std::os::unix::ffi::OsStrExt;
    path.as_os_str().as_bytes().to_vec()
}

fn path_to_uri(path: &Path) -> Result<String, Error> {
    let absolute = std::fs::canonicalize(path).map_err(|source| Error::Path {
        path: path.to_path_buf(),
        source,
    })?;

    let bytes = path_bytes(&absolute);

    let mut uri = String::with_capacity(bytes.len() + 8);
    uri.push_str("file://");
    for &byte in bytes.iter() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' => {
                uri.push(byte as char);
            }
            _ => {
                uri.push('%');
                uri.push(char::from_digit((byte >> 4) as u32, 16).unwrap_or('0'));
                uri.push(char::from_digit((byte & 0x0f) as u32, 16).unwrap_or('0'));
            }
        }
    }
    Ok(uri)
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("playback backend error: {0}")]
    Backend(#[source] BackendError),

    #[error("failed to resolve path {}: {source}", path.display())]
    Path {
        path: std::path::PathBuf,
        #[source]
        source: std::io::Error,
    },
}

#[derive(Debug, Default)]
pub struct NullBackend {
    volume: f64,
    playing_since: Option<std::time::Instant>,
    offset: Duration,
    loaded: bool,
}

impl NullBackend {
    pub fn new() -> NullBackend {
        NullBackend {
            volume: 1.0,
            ..Default::default()
        }
    }

    fn freeze(&mut self) {
        if let Some(since) = self.playing_since.take() {
            self.offset += since.elapsed();
        }
    }
}

impl AudioBackend for NullBackend {
    fn describe(&self) -> &'static str {
        "none (silent mode)"
    }

    fn play_uri(&mut self, _uri: &str) -> Result<(), BackendError> {
        self.offset = Duration::ZERO;
        self.playing_since = Some(std::time::Instant::now());
        self.loaded = true;
        Ok(())
    }
    fn pause(&mut self) -> Result<(), BackendError> {
        self.freeze();
        Ok(())
    }
    fn resume(&mut self) -> Result<(), BackendError> {
        if self.loaded && self.playing_since.is_none() {
            self.playing_since = Some(std::time::Instant::now());
        }
        Ok(())
    }
    fn stop(&mut self) -> Result<(), BackendError> {
        self.playing_since = None;
        self.offset = Duration::ZERO;
        self.loaded = false;
        Ok(())
    }
    fn set_volume(&mut self, volume: f64) {
        self.volume = volume;
    }
    fn volume(&self) -> f64 {
        self.volume
    }
    fn position(&self) -> Option<Duration> {
        if !self.loaded {
            return None;
        }
        Some(self.offset + self.playing_since.map(|s| s.elapsed()).unwrap_or_default())
    }
    fn duration(&self) -> Option<Duration> {
        None
    }
    fn seek(&mut self, position: Duration) -> Result<(), BackendError> {
        self.offset = position;
        if self.playing_since.is_some() {
            self.playing_since = Some(std::time::Instant::now());
        }
        Ok(())
    }
    fn poll_events(&mut self) -> Vec<BackendEvent> {
        Vec::new()
    }
}
