// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Transfer the sealed interval from the browser's isolated world into a temporary PCM WAV.
use std::{fs::File, io::BufWriter, sync::Arc};

use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde_json::Value;
use tempfile::TempPath;

pub(crate) const SCRIPT: &str = include_str!("page_capture.js");
const CHUNK: usize = 64 * 1024;
use super::MAX_PHRASE_PCM_BYTES;

pub(crate) struct PageCapture {
    writer: hound::WavWriter<BufWriter<File>>,
    path: TempPath,
    length: usize,
    written: usize,
    pub(crate) start_ms: u64,
    pub(crate) end_ms: u64,
}

impl PageCapture {
    pub(crate) fn new(result: &Value) -> Result<Self, String> {
        check_result(result)?;
        let number = |key: &str| {
            result[key]
                .as_u64()
                .ok_or_else(|| format!("Invalid {key} from the page player."))
        };
        let start_ms = number("start_ms")?;
        let end_ms = number("end_ms")?;
        let rate = number("sample_rate")?;
        let channels = number("channels")?;
        let frames = number("frames")?;
        if start_ms >= end_ms || !(1000..=384_000).contains(&rate) || !(1..=2).contains(&channels) {
            return Err("The browser returned invalid phrase marks or audio format.".into());
        }
        let time_frame = |ms: u64| u128::from(ms) * u128::from(rate) / 1000;
        if frames == 0 || time_frame(end_ms) - time_frame(start_ms) != u128::from(frames) {
            return Err("The marked interval does not match the browser audio length.".into());
        }
        let length = frames
            .checked_mul(channels * 2)
            .filter(|length| *length <= MAX_PHRASE_PCM_BYTES)
            .ok_or("This phrase exceeds 16 MiB. Mark a shorter passage.")?
            as usize;
        let (file, path) = tempfile::Builder::new()
            .prefix("turnstone-phrase-")
            .suffix(".wav")
            .tempfile()
            .map_err(|e| e.to_string())?
            .into_parts();
        let writer = hound::WavWriter::new(
            BufWriter::new(file),
            hound::WavSpec {
                channels: channels as u16,
                sample_rate: rate as u32,
                bits_per_sample: 16,
                sample_format: hound::SampleFormat::Int,
            },
        )
        .map_err(|e| e.to_string())?;
        Ok(Self {
            writer,
            path,
            length,
            written: 0,
            start_ms,
            end_ms,
        })
    }

    pub(crate) fn next_chunk(&self) -> Option<(usize, usize)> {
        (self.written < self.length)
            .then(|| (self.written, (self.length - self.written).min(CHUNK)))
    }

    pub(crate) fn append(&mut self, result: &Value) -> Result<(), String> {
        check_result(result)?;
        let (_, length) = self
            .next_chunk()
            .ok_or("The entire phrase was already received.")?;
        let data = result["data"].as_str().ok_or("Missing phrase audio.")?;
        if data.len() != length.div_ceil(3) * 4 {
            return Err("The page returned an incomplete phrase audio block.".into());
        }
        let bytes = STANDARD.decode(data).map_err(|_| "Invalid PCM encoding.")?;
        if bytes.len() != length {
            return Err("The browser returned an unexpected amount of PCM.".into());
        }
        for sample in bytes.chunks_exact(2) {
            self.writer
                .write_sample(i16::from_le_bytes([sample[0], sample[1]]))
                .map_err(|e| e.to_string())?;
        }
        self.written += length;
        Ok(())
    }

    pub(crate) fn finish(self) -> Result<Arc<TempPath>, String> {
        if self.written != self.length {
            return Err("The page closed before the phrase finished transferring.".into());
        }
        self.writer.finalize().map_err(|e| e.to_string())?;
        Ok(Arc::new(self.path))
    }
}

fn check_result(result: &Value) -> Result<(), String> {
    if result["ok"].as_bool() == Some(true) {
        Ok(())
    } else {
        Err(result["error"]
            .as_str()
            .unwrap_or("The page player stopped responding.")
            .to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn host_assembles_only_the_marked_frames_with_nonintegral_ms_boundaries() {
        let rate = 44_100;
        let frames = (2001 * rate / 1000) - (9 * rate / 1000);
        let values = json!({"ok":true, "start_ms":9, "end_ms":2001, "frames":frames,
                            "sample_rate":rate, "channels":2});
        let mut capture = PageCapture::new(&values).unwrap();
        assert_eq!((capture.start_ms, capture.end_ms), (9, 2001));
        let samples: Vec<_> = (0..frames * 2)
            .map(|i| (i as i16).wrapping_mul(3))
            .collect();
        let pcm: Vec<_> = samples.iter().flat_map(|v| v.to_le_bytes()).collect();
        while let Some((offset, length)) = capture.next_chunk() {
            assert!(length <= CHUNK);
            capture
                .append(&json!({"ok":true,"data":STANDARD.encode(&pcm[offset..offset+length])}))
                .unwrap();
        }
        let path = capture.finish().unwrap();
        let pathname = path.to_path_buf();
        let mut file = hound::WavReader::open(&pathname).unwrap();
        assert_eq!(file.spec().channels, 2);
        assert_eq!(
            file.samples::<i16>()
                .collect::<Result<Vec<_>, _>>()
                .unwrap(),
            samples
        );
        drop(file);
        drop(path);
        assert!(!pathname.exists());
    }

    #[test]
    fn incomplete_or_oversized_page_audio_is_refused() {
        let mut values = json!({"ok":true, "start_ms":200, "end_ms":2200, "frames":96_000,
                               "sample_rate":48_000, "channels":2});
        let mut transfer = PageCapture::new(&values).unwrap();
        assert!(transfer.append(&json!({"ok":true,"data":"AAAA"})).is_err());
        assert!(transfer.finish().is_err());
        values["frames"] = json!(95_999);
        assert!(PageCapture::new(&values).is_err());
        values["frames"] = json!(48_000_000);
        values["end_ms"] = json!(1_000_200);
        assert!(PageCapture::new(&values).is_err());
    }
}
