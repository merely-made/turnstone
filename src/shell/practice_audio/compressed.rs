// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Decode once so the preview clock and Woodshed's cut refer to the same PCM frames.
use std::{
    fs::File,
    io::ErrorKind,
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};

use symphonia::core::{
    codecs::audio::AudioDecoderOptions,
    errors::Error as DecodeError,
    formats::{FormatOptions, probe::Hint},
    io::MediaSourceStream,
    meta::MetadataOptions,
};
use tempfile::NamedTempFile;

pub(super) fn is_compressed(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("mp3") || ext.eq_ignore_ascii_case("mp4"))
}

pub(super) fn convert(source: &Path, cancel: &AtomicBool) -> Result<NamedTempFile, String> {
    decode(source, cancel).map_err(|error| format!("Could not prepare audio: {error}"))
}

fn decode(source: &Path, cancel: &AtomicBool) -> Result<NamedTempFile, Box<dyn std::error::Error>> {
    let extension = source
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("");
    let stream = MediaSourceStream::new(Box::new(File::open(source)?), Default::default());
    let mut hint = Hint::new();
    hint.with_extension(&extension.to_ascii_lowercase());
    let mut format = symphonia::default::get_probe().probe(
        &hint,
        stream,
        FormatOptions::default(),
        MetadataOptions::default(),
    )?;
    let (track, mut decoder) = format
        .tracks()
        .iter()
        .find_map(|track| {
            let params = track.codec_params.as_ref()?.audio()?;
            symphonia::default::get_codecs()
                .make_audio_decoder(params, &AudioDecoderOptions::default())
                .ok()
                .map(|decoder| (track, decoder))
        })
        .ok_or("This file has no supported audio track (AAC or MP3).")?;
    let track_id = track.id;
    let track_length = track
        .duration
        .map(|duration| duration.get())
        .zip(track.time_base)
        .filter(|(ticks, _)| extension.eq_ignore_ascii_case("mp4") && *ticks > 0);
    let target = tempfile::Builder::new().suffix(".wav").tempfile()?;
    let mut output = None;
    let mut samples: Vec<f32> = Vec::new();
    let mut frames = 0u64;
    let mut frame_limit = u64::MAX;
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err("Preview closed.".into());
        }
        let packet = match format.next_packet() {
            Ok(Some(packet)) if packet.track_id == track_id => packet,
            Ok(Some(_)) => continue,
            Ok(None) => break,
            Err(DecodeError::IoError(error)) if error.kind() == ErrorKind::UnexpectedEof => break,
            Err(error) => return Err(error.into()),
        };
        let audio = decoder.decode(&packet)?;
        if audio.frames() == 0 {
            continue;
        }
        let spec = audio.spec().clone();
        let rate = spec.rate();
        let channels = spec.channels().count();
        if output.is_none() {
            if !(1_000..=384_000).contains(&rate) || channels == 0 {
                return Err("Unsupported sample rate or channel layout.".into());
            }
            if let Some((ticks, base)) = track_length {
                frame_limit = u64::try_from(
                    u128::from(ticks) * u128::from(base.numer.get()) * u128::from(rate)
                        / u128::from(base.denom.get()),
                )?;
            }
            let writer = hound::WavWriter::create(
                target.path(),
                hound::WavSpec {
                    channels: channels.min(2) as u16,
                    sample_rate: rate,
                    bits_per_sample: 16,
                    sample_format: hound::SampleFormat::Int,
                },
            )?;
            output = Some((spec.clone(), writer));
        }
        let (initial_spec, writer) = output.as_mut().expect("initialized on first decoded frame");
        if *initial_spec != spec {
            return Err("The audio track changes format mid-recording.".into());
        }
        let count = (frame_limit - frames).min(audio.frames() as u64);
        if frames + count > (u64::from(u32::MAX) - 128) / (2 * channels.min(2) as u64) {
            return Err("The recording exceeds the PCM preview size limit.".into());
        }
        audio.copy_to_vec_interleaved::<f32>(&mut samples);
        for frame in samples
            .chunks_exact(channels)
            .take(count as usize)
        {
            if frame.iter().any(|sample| !sample.is_finite()) {
                return Err("Invalid sample in the audio track.".into());
            }
            let surround: f64 = frame.iter().skip(2).map(|&sample| f64::from(sample)).sum();
            for &front in frame.iter().take(2) {
                let sample = if channels > 2 {
                    (2.0 * f64::from(front) + surround) / channels as f64
                } else {
                    f64::from(front)
                };
                writer.write_sample((sample * 32768.0).round().clamp(-32768.0, 32767.0) as i16)?;
            }
        }
        frames += count;
        if frames == frame_limit {
            break;
        }
    }
    if frames == 0 {
        return Err("The audio track has no playable frames.".into());
    }
    let (_, writer) = output.ok_or("Missing audio track.")?;
    writer.finalize()?;
    Ok(target)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fixture(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("scenarios/fixtures/practice_audio")
            .join(name)
    }

    #[test]
    fn decoded_preview_uses_the_gapless_mp3_and_the_mp4_audio_track() {
        // A 0.1 s, 660 Hz mono tone at 48 kHz. The MP4 has video in track 0 and
        // 1024 AAC priming frames on its audio-track clock.
        for (name, frames) in [("tone.mp3", 4800), ("video-tone.mp4", 5824)] {
            let preview = convert(&fixture(name), &AtomicBool::new(false)).unwrap();
            let mut wav = hound::WavReader::open(preview.path()).unwrap();
            assert_eq!(
                (wav.spec().sample_rate, wav.spec().channels, wav.duration()),
                (48000, 1, frames),
                "{name}"
            );
            let samples: Vec<_> = wav
                .samples::<i16>()
                .skip(1600)
                .take(2400)
                .collect::<Result<_, _>>()
                .unwrap();
            let crossings = samples
                .windows(2)
                .filter(|pair| pair[0] < 0 && pair[1] >= 0)
                .count();
            assert!(
                (32..=34).contains(&crossings),
                "{name} should contain the 660 Hz tone, got {crossings} crossings over 50 ms"
            );
        }
    }

    #[test]
    fn silent_video_and_cancelled_preview_never_produce_practice_audio() {
        let error = convert(&fixture("video-only.mp4"), &AtomicBool::new(false)).unwrap_err();
        assert!(error.contains("no supported audio track"), "{error}");
        let error = convert(&fixture("video-tone.mp4"), &AtomicBool::new(true)).unwrap_err();
        assert!(error.contains("Preview closed"), "{error}");
    }
}
