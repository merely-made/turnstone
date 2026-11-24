// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use std::path::Path;

pub(crate) fn duration_ms(path: &Path) -> Result<u64, String> {
    let file = match hound::WavReader::open(path) {
        Ok(file) => file,
        Err(error) => {
            #[cfg(windows)]
            if let Ok(length) = Device::open(path).and_then(|device| device.length()) {
                return Ok(length);
            }
            return Err(error.to_string());
        },
    };
    let rate = u64::from(file.spec().sample_rate);
    if file.duration() == 0 || rate == 0 {
        return Err("The WAV has no playable frames.".into());
    }
    Ok((u64::from(file.duration()) * 1000).div_ceil(rate))
}

#[cfg(windows)]
mod windows {
    use std::os::windows::ffi::OsStrExt;
    use std::path::Path;
    use std::sync::atomic::{AtomicBool, Ordering};

    #[link(name = "winmm")]
    unsafe extern "system" {
        fn mciSendStringW(
            command: *const u16,
            output: *mut u16,
            length: u32,
            callback: isize,
        ) -> u32;
        fn mciGetErrorStringW(error: u32, output: *mut u16, length: u32) -> i32;
    }

    pub(crate) struct Device {
        alias: String,
    }

    fn issue(command: &str) -> Result<String, String> {
        let wide: Vec<_> = command.encode_utf16().chain([0]).collect();
        let mut output = [0u16; 256];
        // Both APIs synchronously read/write the supplied NUL-terminated strings.
        let error = unsafe { mciSendStringW(wide.as_ptr(), output.as_mut_ptr(), 256, 0) };
        if error != 0 {
            output.fill(0);
            unsafe { mciGetErrorStringW(error, output.as_mut_ptr(), 256) };
        }
        let end = output
            .iter()
            .position(|word| *word == 0)
            .unwrap_or(output.len());
        let text = String::from_utf16_lossy(&output[..end]);
        if error == 0 {
            Ok(text)
        } else {
            Err(format!("Audio playback: {text} ({error})"))
        }
    }

    impl Device {
        pub(crate) fn open(path: &Path) -> Result<Self, String> {
            let device = Self {
                alias: format!("turnstone{}", uuid::Uuid::new_v4().simple()),
            };
            device.open_file(path)?;
            Ok(device)
        }

        pub(crate) fn length(&self) -> Result<u64, String> {
            self.command("status", "length")?
                .parse()
                .map_err(|_| "Invalid audio device duration.".into())
        }

        fn open_file(&self, path: &Path) -> Result<(), String> {
            let path = String::from_utf16(&path.as_os_str().encode_wide().collect::<Vec<_>>())
                .map_err(|_| "Cannot represent recording path for playback.".to_string())?;
            issue(&format!(
                "open \"{path}\" type waveaudio alias {}",
                self.alias
            ))?;
            if let Err(error) = self.command("set", "time format milliseconds") {
                let _ = self.command("close", "");
                return Err(error);
            }
            Ok(())
        }

        fn command(&self, verb: &str, args: &str) -> Result<String, String> {
            issue(&format!("{verb} {} {args}", self.alias))
        }

        pub(crate) fn position(&self) -> Result<u64, String> {
            self.command("status", "position")?
                .parse()
                .map_err(|_| "Invalid audio device position.".into())
        }

        pub(crate) fn playing(&self) -> Result<bool, String> {
            self.command("status", "mode").map(|mode| mode == "playing")
        }

        pub(crate) fn seek(&self, ms: u64) -> Result<(), String> {
            self.command("seek", &format!("to {ms}")).map(drop)
        }

        pub(crate) fn play(&self) -> Result<(), String> {
            self.command("play", "").map(drop)
        }

        pub(crate) fn pause(&self) -> Result<(), String> {
            self.command("pause", "").map(drop)
        }
    }

    impl Drop for Device {
        fn drop(&mut self) {
            let _ = self.command("close", "");
        }
    }

    pub(crate) fn convert(
        source: &Path,
        cancel: &AtomicBool,
    ) -> Result<tempfile::NamedTempFile, String> {
        let target = tempfile::Builder::new()
            .suffix(".wav")
            .tempfile()
            .map_err(|error| error.to_string())?;
        stream_pcm(source, target.path(), cancel).map_err(|error| error.to_string())?;
        Ok(target)
    }

    fn stream_pcm(source: &Path, target: &Path, cancel: &AtomicBool) -> Result<(), hound::Error> {
        let mut reader = hound::WavReader::open(source)?;
        let spec = reader.spec();
        let channels = usize::from(spec.channels);
        let output_channels = spec.channels.min(2);
        let mut writer = hound::WavWriter::create(
            target,
            hound::WavSpec {
                channels: output_channels,
                bits_per_sample: 16,
                sample_format: hound::SampleFormat::Int,
                ..spec
            },
        )?;
        let mut mix = [0.0; 2];
        let mut index = 0usize;
        let mut write = |sample: f64| -> Result<(), hound::Error> {
            if index % 4096 == 0 && cancel.load(Ordering::Relaxed) {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::Interrupted,
                    "Preview closed.",
                )
                .into());
            }
            let channel = index % channels;
            mix[channel % 2] += if sample.is_finite() { sample } else { 0.0 };
            index += 1;
            if channel + 1 == channels {
                for (output, sum) in mix[..usize::from(output_channels)].iter().enumerate() {
                    let contributors = (channels + 1 - output) / 2;
                    let normalized = (sum / contributors as f64).clamp(-1.0, 1.0);
                    writer.write_sample((normalized * 32767.0).round() as i16)?;
                }
                mix = [0.0; 2];
            }
            Ok(())
        };
        match spec.sample_format {
            hound::SampleFormat::Float => {
                for sample in reader.samples::<f32>() {
                    write(f64::from(sample?))?;
                }
            },
            hound::SampleFormat::Int => {
                let full_scale = 2f64.powi(i32::from(spec.bits_per_sample) - 1);
                for sample in reader.samples::<i32>() {
                    write(f64::from(sample?) / full_scale)?;
                }
            },
        }
        writer.finalize()
    }
}

#[cfg(windows)]
pub(super) use windows::{Device, convert};

#[cfg(not(windows))]
pub(crate) fn convert(
    _: &Path,
    _: &std::sync::atomic::AtomicBool,
) -> Result<tempfile::NamedTempFile, String> {
    Err("PCM device conversion is available on Windows.".into())
}

#[cfg(not(windows))]
pub(crate) struct Device;

#[cfg(not(windows))]
impl Device {
    pub(crate) fn open(_: &Path) -> Result<Self, String> {
        Err("Audio preview is available on Windows.".into())
    }
    pub(crate) fn position(&self) -> Result<u64, String> {
        Ok(0)
    }
    pub(crate) fn playing(&self) -> Result<bool, String> {
        Ok(false)
    }
    pub(crate) fn seek(&self, _: u64) -> Result<(), String> {
        Ok(())
    }
    pub(crate) fn play(&self) -> Result<(), String> {
        Ok(())
    }
    pub(crate) fn pause(&self) -> Result<(), String> {
        Ok(())
    }
}
