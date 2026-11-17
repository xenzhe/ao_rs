//! Sound effects and music (Recursos/wav, SoundsOgg, OGG). OWNER: Opus D
//! (docs/P3_CONTRATO.md).
//!
//! - Effects: `wav/<id>.wav` (PCM), falling back to `SoundsOgg/<id>.ogg`, loaded the
//!   first time they play and cached (`clsAudioEngine.PlayWav` resolves the same ids).
//! - Music: `OGG/ost_<n>.ogg` of the map (`PlayMapMusic`), read on a background thread
//!   so entering a map never waits on the disk; only one track is kept in memory.
//! - Ambient loop: rain outside/inside a roof (`SND_RAIN_OUT_LOOP` / `IN_LOOP`).
//! - Volumes from the `[audio]` section of `config/client.toml` (0..1):
//!   `master`, `music`, `effects`, `steps`, `ambient`, `enabled`. Muted when
//!   `AO_SHOT` (automated screenshots) or `AO_MUTE` is set.
use macroquad::audio::{load_sound_from_bytes, play_sound, set_sound_volume, stop_sound, PlaySoundParams, Sound};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver};

#[derive(Deserialize, Clone, Debug)]
#[serde(default)]
pub struct AudioConfig {
    pub enabled: bool,
    pub master: f32,
    pub music: f32,
    pub effects: f32,
    pub steps: f32,
    pub ambient: f32,
}

impl Default for AudioConfig {
    fn default() -> Self {
        AudioConfig { enabled: true, master: 0.8, music: 0.45, effects: 0.9, steps: 0.5, ambient: 0.5 }
    }
}

#[derive(Deserialize, Default)]
struct FileConfig {
    #[serde(default)]
    audio: AudioConfig,
}

impl AudioConfig {
    pub fn load() -> AudioConfig {
        let path = std::env::var("AO_CLIENT_CONFIG").unwrap_or_else(|_| "config/client.toml".into());
        let mut c = std::fs::read_to_string(path).ok().and_then(|t| toml::from_str::<FileConfig>(&t).ok()).unwrap_or_default().audio;
        if std::env::var("AO_SHOT").is_ok() || std::env::var("AO_MUTE").is_ok() {
            c.enabled = false;
        }
        c
    }
}

/// Which volume slider a sound follows.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Channel {
    Effects,
    Steps,
}

/// Validates a sound file before handing it to the decoder (which panics on anything
/// it cannot read): PCM WAV or an Ogg stream.
pub fn playable(bytes: &[u8]) -> bool {
    if bytes.len() > 44 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WAVE" {
        // First chunk is "fmt " with audio format 1 (PCM) or 3 (float) in every AO file.
        return &bytes[12..16] == b"fmt " && matches!(u16::from_le_bytes([bytes[20], bytes[21]]), 1 | 3);
    }
    bytes.len() > 64 && &bytes[0..4] == b"OggS"
}

/// Candidate files for an effect id, in the order the official engine tries them.
pub fn effect_paths(root: &Path, id: u16) -> [PathBuf; 2] {
    [root.join("wav").join(format!("{id}.wav")), root.join("SoundsOgg").join(format!("{id}.ogg"))]
}

pub fn music_path(root: &Path, n: i32) -> PathBuf {
    root.join("OGG").join(format!("ost_{n}.ogg"))
}

/// Decodes an Ogg Vorbis file into an in-memory 16-bit PCM WAV, on a worker thread:
/// the mixer's own decoder runs on the main thread and takes seconds for a track in a
/// debug build. Mono becomes stereo and the rate is kept (the mixer resamples).
pub fn ogg_to_wav(bytes: Vec<u8>) -> Option<Vec<u8>> {
    let mut r = lewton::inside_ogg::OggStreamReader::new(std::io::Cursor::new(bytes)).ok()?;
    let ch = r.ident_hdr.audio_channels.clamp(1, 2) as usize;
    let rate = r.ident_hdr.audio_sample_rate;
    let mut pcm: Vec<i16> = Vec::new();
    while let Ok(Some(p)) = r.read_dec_packet_itl() {
        if ch == 1 {
            pcm.extend(p.iter().flat_map(|s| [*s, *s]));
        } else {
            pcm.extend(p);
        }
    }
    if pcm.is_empty() {
        return None;
    }
    Some(wav_bytes(&pcm, rate))
}

/// Stereo 16-bit PCM WAV container.
pub fn wav_bytes(pcm: &[i16], rate: u32) -> Vec<u8> {
    let data = (pcm.len() * 2) as u32;
    let mut w = Vec::with_capacity(44 + data as usize);
    w.extend_from_slice(b"RIFF");
    w.extend_from_slice(&(36 + data).to_le_bytes());
    w.extend_from_slice(b"WAVEfmt ");
    w.extend_from_slice(&16u32.to_le_bytes());
    w.extend_from_slice(&1u16.to_le_bytes());
    w.extend_from_slice(&2u16.to_le_bytes());
    w.extend_from_slice(&rate.to_le_bytes());
    w.extend_from_slice(&(rate * 4).to_le_bytes());
    w.extend_from_slice(&4u16.to_le_bytes());
    w.extend_from_slice(&16u16.to_le_bytes());
    w.extend_from_slice(b"data");
    w.extend_from_slice(&data.to_le_bytes());
    for s in pcm {
        w.extend_from_slice(&s.to_le_bytes());
    }
    w
}

/// `load_sound_from_bytes` is async only for wasm; on desktop it completes on the first
/// poll, so it is driven here without an executor.
fn load_now(bytes: &[u8]) -> Option<Sound> {
    use std::future::Future;
    use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};
    fn noop(_: *const ()) {}
    fn clone(p: *const ()) -> RawWaker {
        RawWaker::new(p, &VTABLE)
    }
    static VTABLE: RawWakerVTable = RawWakerVTable::new(clone, noop, noop, noop);
    if !playable(bytes) {
        return None;
    }
    // SAFETY: the vtable functions ignore the data pointer.
    let waker = unsafe { Waker::from_raw(RawWaker::new(std::ptr::null(), &VTABLE)) };
    let mut cx = Context::from_waker(&waker);
    let mut fut = std::pin::pin!(load_sound_from_bytes(bytes));
    // The decoder unwraps internally: a corrupt file must not take the game down.
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| fut.as_mut().poll(&mut cx)));
    match r {
        Ok(Poll::Ready(Ok(s))) => Some(s),
        _ => None,
    }
}

pub struct Audio {
    root: PathBuf,
    pub cfg: AudioConfig,
    effects: HashMap<u16, Option<Sound>>,
    music: Option<(i32, Sound)>,
    wanted_music: i32,
    loading: Option<(i32, Receiver<Option<Vec<u8>>>)>,
    ambient: Option<(u16, Sound)>,
}

impl Audio {
    pub fn new(root: PathBuf) -> Audio {
        Audio { root, cfg: AudioConfig::load(), effects: HashMap::new(), music: None, wanted_music: 0, loading: None, ambient: None }
    }

    fn effect(&mut self, id: u16) -> Option<Sound> {
        if let Some(s) = self.effects.get(&id) {
            return s.clone();
        }
        // Ogg goes through lewton first (errors, never panics): release builds use
        // `panic = "abort"`, so the mixer's unwrapping decoder only ever sees clean WAV.
        let s = effect_paths(&self.root, id).iter().find_map(|p| {
            let b = std::fs::read(p).ok()?;
            let b = if b.starts_with(b"OggS") { ogg_to_wav(b)? } else { b };
            load_now(&b)
        });
        self.effects.insert(id, s.clone());
        s
    }

    /// Plays an effect at relative volume `vol` (distance attenuation), lazily loaded.
    pub fn play(&mut self, id: u16, vol: f32, ch: Channel) {
        if !self.cfg.enabled || id == 0 || vol <= 0.01 {
            return;
        }
        let v = vol * self.cfg.master * if ch == Channel::Steps { self.cfg.steps } else { self.cfg.effects };
        if let Some(s) = self.effect(id) {
            play_sound(&s, PlaySoundParams { looped: false, volume: v.clamp(0.0, 1.0) });
        }
    }

    /// Music of the map (`n <= 0` = silence). Restarting the same track is ignored.
    pub fn music(&mut self, n: i32) {
        if !self.cfg.enabled || n == self.wanted_music {
            return;
        }
        self.wanted_music = n;
        if let Some((_, s)) = self.music.take() {
            stop_sound(&s);
        }
        if n <= 0 {
            return;
        }
        let path = music_path(&self.root, n);
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let _ = tx.send(std::fs::read(path).ok().and_then(ogg_to_wav));
        });
        self.loading = Some((n, rx));
    }

    /// Ambient loop (`0` = none), e.g. rain in or out of a roof.
    pub fn ambient(&mut self, id: u16) {
        if !self.cfg.enabled || self.ambient.as_ref().map(|a| a.0) == Some(id) || (id == 0 && self.ambient.is_none()) {
            return;
        }
        if let Some((_, s)) = self.ambient.take() {
            stop_sound(&s);
        }
        if id > 0 {
            if let Some(s) = self.effect(id) {
                play_sound(&s, PlaySoundParams { looped: true, volume: self.cfg.master * self.cfg.ambient });
                self.ambient = Some((id, s));
            }
        }
    }

    /// Picks up a finished background read and starts the track.
    pub fn update(&mut self) {
        let Some((n, rx)) = &self.loading else { return };
        let Ok(bytes) = rx.try_recv() else { return };
        let n = *n;
        self.loading = None;
        if n != self.wanted_music {
            return;
        }
        let t0 = std::time::Instant::now();
        let sound = bytes.and_then(|b| load_now(&b));
        if std::env::var("AO_DEBUG").is_ok() {
            eprintln!("music ost_{n}: decoded={} in {:?}", sound.is_some(), t0.elapsed());
        }
        if let Some(s) = sound {
            let v = self.cfg.master * self.cfg.music;
            play_sound(&s, PlaySoundParams { looped: true, volume: v });
            set_sound_volume(&s, v);
            self.music = Some((n, s));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_before_decoding() {
        assert!(!playable(b""));
        assert!(!playable(&[0u8; 100]));
        let mut wav = b"RIFF\0\0\0\0WAVEfmt \x10\0\0\0\x01\0\x01\0".to_vec();
        wav.resize(64, 0);
        assert!(playable(&wav));
        wav[20] = 2; // ADPCM: not supported by the decoder
        assert!(!playable(&wav));
    }

    #[test]
    fn official_files_resolve() {
        let root = ao_formats::recursos_dir();
        if !root.join("wav").is_dir() {
            return;
        }
        // Steps (23/24), hit (10), swing (2) and the meditation (158) exist and are PCM.
        for id in [23u16, 24, 10, 2, 158, 194] {
            let ok = effect_paths(&root, id).iter().any(|p| std::fs::read(p).is_ok_and(|b| playable(&b)));
            assert!(ok, "sound {id}");
        }
        assert!(std::fs::read(music_path(&root, 1)).is_ok_and(|b| playable(&b)));
        // Off-thread Ogg decoding gives a valid stereo PCM WAV.
        let wav = ogg_to_wav(std::fs::read(root.join("SoundsOgg").join("1.ogg")).unwrap()).expect("decode");
        assert!(playable(&wav) && wav.len() > 1000);
        assert_eq!(u16::from_le_bytes([wav[22], wav[23]]), 2);
        let c: FileConfig = toml::from_str("[audio]\nmusic = 0.2\n").unwrap();
        assert_eq!(c.audio.music, 0.2);
        assert!(c.audio.enabled);
    }
}
