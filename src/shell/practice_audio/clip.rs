// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Seal a user-matched interval using the existing page-phrase PCM and clock contract.

use super::selection::MatchedSource;
use crate::woodshed_client::{ImportWav, PagePhrase};
use hound::{SampleFormat, WavReader, WavSpec, WavWriter};
use std::{io::BufWriter, path::Path, sync::Arc};

pub(super) struct RecordingImport {
    pub(super) request: ImportWav,
    pub(super) selected: Option<MatchedSource>,
    pub(super) preview: Option<Arc<tempfile::TempPath>>,
}

impl RecordingImport {
    pub(super) fn prepare(&self) -> Result<PagePhrase, String> {
        let source = self
            .selected
            .as_ref()
            .ok_or("Match a timed selection first.")?;
        let request = &self.request;
        let checked_path = request.wav.checked_path().map_err(|e| e.to_string())?;
        let recording = self.preview.as_deref().map_or(checked_path, AsRef::as_ref);
        let phrase = extract(recording, request.start_ms, request.end_ms)?;
        let note = if request.note.is_empty() {
            source.note.clone()
        } else {
            format!("{}\n{}", request.note, source.note)
        };
        Ok(PagePhrase {
            title: request.title.clone(),
            source_page_url: source.source.to_string(),
            start_ms: request.start_ms,
            end_ms: request.end_ms,
            note,
            phrase: Arc::new(phrase),
        })
    }

    pub(super) fn save(&self, destination: &Path) -> Result<(), String> {
        if destination
            .canonicalize()
            .is_ok_and(|path| path == self.request.wav.path())
        {
            return Err("Choose a new path for the clip to keep the original recording.".into());
        }
        let selected = self
            .selected
            .as_ref()
            .ok_or("Match a timed selection first.")?;
        let phrase = self.prepare()?;
        let mut target = selected.source.clone();
        target.set_fragment(Some(&format!(
            "t={},{}",
            super::format_time(phrase.start_ms),
            super::format_time(phrase.end_ms)
        )));
        let save = || -> std::io::Result<()> {
            let folder = destination.parent().unwrap_or_else(|| Path::new("."));
            let mut temporary = tempfile::NamedTempFile::new_in(folder)?;
            let mut pcm = std::fs::File::open(&*phrase.phrase)?;
            std::io::copy(&mut pcm, temporary.as_file_mut())?;
            write_info(
                temporary.as_file_mut(),
                &[
                    (b"INAM", &phrase.title),
                    (b"ISRC", target.as_str()),
                    (b"ISBJ", &selected.note),
                    (b"ICMT", &self.request.note),
                ],
            )?;
            temporary.as_file().sync_all()?;
            temporary.persist(destination)?;
            Ok(())
        };
        save().map_err(|error| format!("Could not save the matched WAV: {error}"))
    }
}

// Standard RIFF LIST/INFO text, written after the unmodified PCM data chunk.
// Each string includes a terminating NUL, then padding to the next even byte.
fn write_info(file: &mut std::fs::File, fields: &[(&[u8; 4], &str)]) -> std::io::Result<()> {
    use std::io::{Error, ErrorKind, Seek, SeekFrom, Write};
    let size = |n: usize| {
        u32::try_from(n).map_err(|_| {
            Error::new(
                ErrorKind::InvalidData,
                "WAV metadata exceeds the RIFF size limit",
            )
        })
    };
    let mut list = b"INFO".to_vec();
    for (key, value) in fields {
        if value.as_bytes().contains(&0) {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "WAV metadata cannot contain NUL",
            ));
        }
        list.extend_from_slice(*key);
        list.extend_from_slice(&size(value.len() + 1)?.to_le_bytes());
        list.extend_from_slice(value.as_bytes());
        list.push(0);
        if list.len() % 2 == 1 {
            list.push(0);
        }
    }
    file.write_all(b"LIST")?;
    file.write_all(&size(list.len())?.to_le_bytes())?;
    file.write_all(&list)?;
    let end = file.stream_position()?;
    let riff_size = u32::try_from(end - 8)
        .map_err(|_| Error::new(ErrorKind::InvalidData, "WAV exceeds RIFF size limit"))?;
    file.seek(SeekFrom::Start(4))?;
    file.write_all(&riff_size.to_le_bytes())
}

fn extract(path: &Path, start_ms: u64, end_ms: u64) -> Result<tempfile::TempPath, String> {
    let mut reader = WavReader::open(path).map_err(|e| e.to_string())?;
    let spec = reader.spec();
    if !(1000..=384_000).contains(&spec.sample_rate) || spec.channels == 0 {
        return Err("The recording has an unsupported sample rate or channel count.".into());
    }
    let frame = |ms: u64| u128::from(ms) * u128::from(spec.sample_rate) / 1000;
    let start = frame(start_ms);
    let end = frame(end_ms);
    if start >= end || end > u128::from(reader.duration()) {
        return Err("The selected frames are outside this recording.".into());
    }
    let frames = (end - start) as usize;
    let channels = spec.channels.min(2);
    if (end - start) * u128::from(channels) * 2 > u128::from(super::MAX_PHRASE_PCM_BYTES) {
        return Err("This phrase exceeds 16 MiB. Match a shorter passage.".into());
    }
    reader.seek(start as u32).map_err(|e| e.to_string())?;
    let (file, path) = tempfile::Builder::new()
        .prefix("turnstone-phrase-")
        .suffix(".wav")
        .tempfile()
        .map_err(|e| e.to_string())?
        .into_parts();
    let mut writer = WavWriter::new(
        BufWriter::new(file),
        WavSpec {
            sample_rate: spec.sample_rate,
            channels,
            bits_per_sample: 16,
            sample_format: SampleFormat::Int,
        },
    )
    .map_err(|e| e.to_string())?;
    match spec.sample_format {
        SampleFormat::Int => {
            let scale = 2_f64.powi(16 - i32::from(spec.bits_per_sample));
            let samples = reader
                .samples::<i32>()
                .map(|s| s.map(|v| f64::from(v) * scale));
            write_frames(&mut writer, samples, frames, spec.channels)?;
        },
        SampleFormat::Float => {
            let samples = reader
                .samples::<f32>()
                .map(|s| s.map(|v| f64::from(v) * 32768.0));
            write_frames(&mut writer, samples, frames, spec.channels)?;
        },
    }
    writer.finalize().map_err(|e| e.to_string())?;
    Ok(path)
}

fn write_frames<W: std::io::Write + std::io::Seek>(
    writer: &mut WavWriter<W>,
    mut samples: impl Iterator<Item = Result<f64, hound::Error>>,
    frames: usize,
    channels: u16,
) -> Result<(), String> {
    for _ in 0..frames {
        let mut mix = [0.0; 2];
        for channel in 0..usize::from(channels) {
            let sample = samples
                .next()
                .ok_or("The recording ended inside the selected interval.")?
                .map_err(|e| e.to_string())?;
            if !sample.is_finite() {
                return Err(
                    "The recording contains a non-finite sample in the selected interval.".into(),
                );
            }
            mix[channel % 2] += sample;
        }
        for (channel, sum) in mix.iter().take(usize::from(channels.min(2))).enumerate() {
            let count = (usize::from(channels) + 1 - channel) / 2;
            let sample = (sum / count as f64).round().clamp(-32768.0, 32767.0) as i16;
            writer.write_sample(sample).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}
