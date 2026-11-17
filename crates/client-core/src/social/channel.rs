//! What the client knows about channel (filled from `S2C::Channel` only). OWNER: Sonnet 2.
use ao_protocol::social::channel::ChannelKind;
use ao_protocol::ChannelS2C;
use std::collections::VecDeque;

/// Most lines kept; older ones are dropped.
pub const HISTORY_MAX: usize = 200;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChannelLine {
    pub channel: ChannelKind,
    pub author: String,
    /// Whisper receiver (empty on other channels).
    pub target: String,
    pub text: String,
}

#[derive(Debug, Default)]
pub struct ChannelState {
    /// Oldest first, bounded by `HISTORY_MAX`.
    pub history: VecDeque<ChannelLine>,
    /// Name of the last player who whispered to us (for a quick reply).
    pub last_whisper_from: Option<String>,
}

impl ChannelState {
    pub fn apply(&mut self, msg: &ChannelS2C) {
        match msg {
            ChannelS2C::Message { channel, author, target, text } => {
                if *channel == ChannelKind::Whisper && !author.is_empty() {
                    self.last_whisper_from = Some(author.clone());
                }
                self.history.push_back(ChannelLine { channel: *channel, author: author.clone(), target: target.clone(), text: text.clone() });
                while self.history.len() > HISTORY_MAX {
                    self.history.pop_front();
                }
            }
        }
    }

    /// Lines of one channel, oldest first.
    pub fn lines(&self, channel: ChannelKind) -> impl Iterator<Item = &ChannelLine> {
        self.history.iter().filter(move |l| l.channel == channel)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(channel: ChannelKind, n: usize) -> ChannelS2C {
        ChannelS2C::Message { channel, author: "Ana".into(), target: String::new(), text: format!("l{n}") }
    }

    #[test]
    fn history_is_bounded_and_filtered() {
        let mut s = ChannelState::default();
        for i in 0..HISTORY_MAX + 50 {
            s.apply(&line(if i % 2 == 0 { ChannelKind::Global } else { ChannelKind::Guild }, i));
        }
        assert_eq!(s.history.len(), HISTORY_MAX);
        assert_eq!(s.history.front().unwrap().text, "l50");
        assert!(s.lines(ChannelKind::Guild).all(|l| l.channel == ChannelKind::Guild));
        s.apply(&ChannelS2C::Message { channel: ChannelKind::Whisper, author: "Bea".into(), target: "Ana".into(), text: "hola".into() });
        assert_eq!(s.last_whisper_from.as_deref(), Some("Bea"));
    }
}
