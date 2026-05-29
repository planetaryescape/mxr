use anyhow::{Context, Result};
use mxr_config::ChimeSound;
use rodio::source::{SineWave, Source};
use std::time::Duration;

#[derive(Debug, Clone, Copy)]
struct Tone {
    hz: f32,
    ms: u64,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("mxr-chime-player: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let (sound, volume) = parse_args(std::env::args().skip(1))?;
    if sound == ChimeSound::None {
        return Ok(());
    }

    play_sound(sound, volume)
}

fn parse_args(args: impl IntoIterator<Item = String>) -> Result<(ChimeSound, f32)> {
    let mut args = args.into_iter();
    let mut sound = None;
    let mut volume = 0.35_f32;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--sound" => {
                let value = args.next().context("missing value for --sound")?;
                sound = Some(parse_sound(&value)?);
            }
            "--volume" => {
                let value = args.next().context("missing value for --volume")?;
                volume = value.parse().context("invalid --volume value")?;
            }
            _ => anyhow::bail!("unknown argument {arg}"),
        }
    }

    Ok((sound.context("missing --sound")?, volume))
}

fn parse_sound(value: &str) -> Result<ChimeSound> {
    match value {
        "none" => Ok(ChimeSound::None),
        "bell" => Ok(ChimeSound::Bell),
        "glass" => Ok(ChimeSound::Glass),
        "pop" => Ok(ChimeSound::Pop),
        "sent" => Ok(ChimeSound::Sent),
        "archive" => Ok(ChimeSound::Archive),
        "thud" => Ok(ChimeSound::Thud),
        "alert" => Ok(ChimeSound::Alert),
        _ => anyhow::bail!("unknown sound {value}"),
    }
}

fn play_sound(sound: ChimeSound, volume: f32) -> Result<()> {
    let mut sink = rodio::DeviceSinkBuilder::open_default_sink()?;
    sink.log_on_drop(false);
    let player = rodio::Player::connect_new(sink.mixer());
    let volume = volume.clamp(0.0, 1.0);

    for tone in tones(sound) {
        player.append(
            SineWave::new(tone.hz)
                .take_duration(Duration::from_millis(tone.ms))
                .amplify(volume),
        );
    }

    player.sleep_until_end();
    Ok(())
}

fn tones(sound: ChimeSound) -> &'static [Tone] {
    match sound {
        ChimeSound::None => &[],
        ChimeSound::Bell => &[
            Tone { hz: 880.0, ms: 70 },
            Tone {
                hz: 1320.0,
                ms: 110,
            },
        ],
        ChimeSound::Glass => &[
            Tone {
                hz: 1174.66,
                ms: 80,
            },
            Tone {
                hz: 1567.98,
                ms: 140,
            },
        ],
        ChimeSound::Pop => &[Tone { hz: 660.0, ms: 90 }],
        ChimeSound::Sent => &[
            Tone { hz: 523.25, ms: 55 },
            Tone { hz: 659.25, ms: 65 },
            Tone { hz: 783.99, ms: 95 },
        ],
        ChimeSound::Archive => &[
            Tone { hz: 783.99, ms: 55 },
            Tone { hz: 659.25, ms: 65 },
            Tone { hz: 523.25, ms: 90 },
        ],
        ChimeSound::Thud => &[Tone { hz: 196.0, ms: 120 }],
        ChimeSound::Alert => &[Tone { hz: 440.0, ms: 70 }, Tone { hz: 440.0, ms: 70 }],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_args_requires_sound_and_accepts_volume() {
        let (sound, volume) =
            parse_args(["--sound", "archive", "--volume", "0.2"].map(String::from))
                .expect("parse args");

        assert_eq!(sound, ChimeSound::Archive);
        assert_eq!(volume, 0.2);
    }
}
