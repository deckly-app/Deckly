//! Optional Timer chime. Only the audio worker opens the system's default
//! output; an unavailable or slow device must not delay keys or the editor.

use super::Msg;
use rodio::{DeviceSinkBuilder, Player, buffer::SamplesBuffer};
use std::num::{NonZeroU16, NonZeroU32};
use std::sync::mpsc::{Sender, SyncSender, sync_channel};
use std::time::Duration;

pub fn start(results: Sender<Msg>) -> SyncSender<()> {
    // Simultaneous expirations share a chime rather than queueing a long
    // series of alarms. Nothing blocks the caller.
    let (tx, rx) = sync_channel(1);
    let worker = std::thread::Builder::new()
        .name("deckly-timer-audio".into())
        .spawn(move || {
            while rx.recv().is_ok() {
                let result = play();
                while rx.try_recv().is_ok() {}
                if let Err(e) = result {
                    let _ = results.send(Msg::TimerAudioFailed(e));
                }
            }
        });
    if let Err(e) = worker {
        eprintln!("deckly: cannot start timer audio thread: {e}");
    }
    tx
}

fn play() -> Result<(), String> {
    let mut output = DeviceSinkBuilder::from_default_device()
        .and_then(|builder| builder.open_stream())
        .map_err(|e| e.to_string())?;
    output.log_on_drop(false);
    let player = Player::connect_new(output.mixer());
    player.append(SamplesBuffer::new(NonZeroU16::MIN, SAMPLE_RATE, chime()));
    // Enough time to play and drain the chime, but never wait indefinitely
    // for a device that was unplugged mid-note.
    std::thread::sleep(Duration::from_millis(800));
    if player.empty() {
        Ok(())
    } else {
        Err("the default audio output stopped responding".into())
    }
}

const SAMPLE_RATE: NonZeroU32 = match NonZeroU32::new(48_000) {
    Some(rate) => rate,
    None => NonZeroU32::MIN,
};
const NOTE_SAMPLES: u32 = SAMPLE_RATE.get() / 5;

fn chime() -> Vec<f32> {
    [660.0, 880.0]
        .into_iter()
        .flat_map(|hz| {
            (0..NOTE_SAMPLES).map(move |i| {
                let phase = std::f32::consts::TAU * hz * i as f32 / SAMPLE_RATE.get() as f32;
                // Fade at both ends so the chime doesn't click.
                let envelope = (i.min(NOTE_SAMPLES - 1 - i) as f32 / 960.0).min(1.0);
                phase.sin() * envelope * 0.25
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "plays a short chime on the system's default audio output"]
    fn plays_on_the_default_audio_output() {
        play().unwrap();
    }

    #[test]
    fn the_generated_chime_is_short_bounded_and_fades_at_each_end() {
        let samples = chime();
        assert_eq!(samples.len(), (NOTE_SAMPLES * 2) as usize);
        assert_eq!(samples.first(), Some(&0.0));
        assert_eq!(samples.last(), Some(&0.0));
        assert!(samples.iter().all(|s| s.abs() <= 0.25));
        assert!(samples.iter().any(|s| s.abs() > 0.2));
    }
}
