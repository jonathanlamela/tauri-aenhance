use std::{
    fs::{self, File},
    io::ErrorKind,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicBool, Ordering},
};

use anyhow::{anyhow, bail, Context, Result};
use base64::{engine::general_purpose::STANDARD, Engine};
use hound::{SampleFormat, WavSpec, WavWriter};
use nnnoiseless::DenoiseState;
use symphonia::core::{
    audio::SampleBuffer, codecs::DecoderOptions, errors::Error as SymphoniaError,
    formats::FormatOptions, io::MediaSourceStream, meta::MetadataOptions, probe::Hint,
};
use tempfile::NamedTempFile;

use crate::models::{AnalyzeAudioResponse, ProcessAudioRequest, ProcessAudioResponse};

const DENOISE_SAMPLE_RATE: u32 = 48_000;
const FRAME_SIZE: usize = DenoiseState::FRAME_SIZE;

struct DecodedAudio {
    format: String,
    sample_rate: u32,
    channels: usize,
    frames: usize,
    samples: Vec<f32>,
}

pub fn analyze_audio_with_progress<F>(path: &str, mut progress: F) -> Result<AnalyzeAudioResponse>
where
    F: FnMut(&str, u8),
{
    progress("preparing", 5);
    let decoded = decode_audio_file(Path::new(path))?;
    let file_name = Path::new(path)
        .file_name()
        .and_then(|v| v.to_str())
        .unwrap_or("audio")
        .to_string();
    progress("done", 100);
    Ok(AnalyzeAudioResponse {
        path: path.to_string(),
        file_name,
        format: decoded.format,
        duration_seconds: decoded.frames as f64 / decoded.sample_rate as f64,
        sample_rate: decoded.sample_rate,
        channels: decoded.channels,
        frames: decoded.frames,
    })
}

pub fn read_audio_bytes(path: &str) -> Result<String> {
    let bytes = fs::read(path).with_context(|| format!("Unable to read file: {path}"))?;
    Ok(STANDARD.encode(bytes))
}

pub fn process_audio_with_progress<F>(
    request: ProcessAudioRequest,
    mut progress: F,
    cancel: &AtomicBool,
) -> Result<ProcessAudioResponse>
where
    F: FnMut(&str, u8),
{
    progress("decoding", 5);
    let decoded = decode_audio_file(Path::new(&request.input_path))?;
    let output_format = request.output_format.trim().to_lowercase();
    let passes = request.passes.max(1).min(3) as usize;

    progress("resampling", 12);

    let channel_data = deinterleave(&decoded.samples, decoded.channels);
    let total_channels = channel_data.len().max(1);
    let mut processed_channels: Vec<Vec<f32>> = Vec::with_capacity(channel_data.len());

    for (index, channel) in channel_data.iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            bail!("cancelled");
        }

        let resampled = linear_resample(channel, decoded.sample_rate, DENOISE_SAMPLE_RATE);
        progress("denoising", 15 + (index as f32 / total_channels as f32 * 65.0) as u8);

        let denoised = denoise_channel(&resampled, passes, cancel, |pass, frame_pct| {
            let ch_frac =
                (index as f32 + (pass as f32 + frame_pct) / passes as f32) / total_channels as f32;
            progress("denoising", (15 + (ch_frac * 65.0) as u8).min(79));
        })?;

        let restored = linear_resample(&denoised, DENOISE_SAMPLE_RATE, decoded.sample_rate);
        let mut result = restored;
        match result.len().cmp(&channel.len()) {
            std::cmp::Ordering::Greater => result.truncate(channel.len()),
            std::cmp::Ordering::Less => result.resize(channel.len(), 0.0),
            std::cmp::Ordering::Equal => {}
        }
        processed_channels.push(result);
    }

    let mut interleaved = interleave(&processed_channels);

    if request.normalize {
        progress("boosting", 81);
        peak_normalize(&mut interleaved, -1.0);
    }

    progress("writing", 85);

    if output_format == "wav" {
        write_wav(
            Path::new(&request.output_path),
            decoded.sample_rate,
            decoded.channels as u16,
            &interleaved,
        )?;
        progress("done", 100);
        return Ok(ProcessAudioResponse {
            output_path: request.output_path,
            output_format,
            ffmpeg_used: false,
        });
    }

    if output_format == "mp3" {
        let temp_file = NamedTempFile::new().context("Unable to create temporary WAV file")?;
        write_wav(
            temp_file.path(),
            decoded.sample_rate,
            decoded.channels as u16,
            &interleaved,
        )?;
        progress("transcoding", 95);
        transcode_to_mp3(temp_file.path(), Path::new(&request.output_path))?;
        progress("done", 100);
        return Ok(ProcessAudioResponse {
            output_path: request.output_path,
            output_format,
            ffmpeg_used: true,
        });
    }

    bail!("Unsupported output format: {}", request.output_format)
}

fn denoise_channel<P>(
    input: &[f32],
    passes: usize,
    cancel: &AtomicBool,
    mut on_progress: P,
) -> Result<Vec<f32>>
where
    P: FnMut(usize, f32),
{
    let mut current = input.to_vec();
    for pass in 0..passes {
        let total_frames = (current.len() + FRAME_SIZE - 1) / FRAME_SIZE;
        let mut state = DenoiseState::new();
        let mut output = vec![0.0f32; current.len()];
        let mut in_buf = [0.0f32; FRAME_SIZE];
        let mut out_buf = [0.0f32; FRAME_SIZE];
        let mut pos = 0;
        let mut frame_idx = 0usize;

        while pos + FRAME_SIZE <= current.len() {
            if cancel.load(Ordering::Relaxed) {
                bail!("cancelled");
            }
            for (i, &s) in current[pos..pos + FRAME_SIZE].iter().enumerate() {
                in_buf[i] = s * 32768.0;
            }
            state.process_frame(&mut out_buf, &in_buf);
            for (i, &s) in out_buf.iter().enumerate() {
                output[pos + i] = (s / 32768.0).clamp(-1.0, 1.0);
            }
            pos += FRAME_SIZE;
            frame_idx += 1;
            if frame_idx % 64 == 0 {
                on_progress(pass, frame_idx as f32 / total_frames.max(1) as f32);
            }
        }

        if pos < current.len() {
            in_buf = [0.0f32; FRAME_SIZE];
            out_buf = [0.0f32; FRAME_SIZE];
            for (i, &s) in current[pos..].iter().enumerate() {
                in_buf[i] = s * 32768.0;
            }
            state.process_frame(&mut out_buf, &in_buf);
            for (i, &s) in out_buf.iter().take(current.len() - pos).enumerate() {
                output[pos + i] = (s / 32768.0).clamp(-1.0, 1.0);
            }
        }

        on_progress(pass, 1.0);
        current = output;
    }
    Ok(current)
}

fn decode_audio_file(path: &Path) -> Result<DecodedAudio> {
    let file = File::open(path).with_context(|| format!("Unable to open {}", path.display()))?;
    let media_source = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();

    if let Some(ext) = path.extension().and_then(|v| v.to_str()) {
        hint.with_extension(ext);
    }

    let probed = symphonia::default::get_probe().format(
        &hint,
        media_source,
        &FormatOptions::default(),
        &MetadataOptions::default(),
    )?;

    let mut format = probed.format;
    let track = format
        .default_track()
        .context("No default audio track found")?;
    let track_id = track.id;
    let sample_rate = track
        .codec_params
        .sample_rate
        .context("Missing sample rate")?;
    let channels = track
        .codec_params
        .channels
        .context("Missing channel info")?
        .count();
    let mut decoder =
        symphonia::default::get_codecs().make(&track.codec_params, &DecoderOptions::default())?;
    let mut samples = Vec::new();

    loop {
        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(SymphoniaError::IoError(e)) if e.kind() == ErrorKind::UnexpectedEof => break,
            Err(SymphoniaError::ResetRequired) => {
                bail!("Format reset is not supported for this file")
            }
            Err(e) => return Err(e.into()),
        };

        if packet.track_id() != track_id {
            continue;
        }

        let decoded = match decoder.decode(&packet) {
            Ok(d) => d,
            Err(SymphoniaError::DecodeError(_)) => continue,
            Err(SymphoniaError::IoError(e)) if e.kind() == ErrorKind::UnexpectedEof => break,
            Err(e) => return Err(e.into()),
        };

        let mut buffer = SampleBuffer::<f32>::new(decoded.capacity() as u64, *decoded.spec());
        buffer.copy_interleaved_ref(decoded);
        samples.extend_from_slice(buffer.samples());
    }

    if samples.is_empty() {
        bail!("The selected file did not decode to audio samples")
    }

    let frames = samples.len() / channels;
    let format_name = path
        .extension()
        .and_then(|v| v.to_str())
        .unwrap_or("unknown")
        .to_lowercase();

    Ok(DecodedAudio {
        format: format_name,
        sample_rate,
        channels,
        frames,
        samples,
    })
}

fn deinterleave(samples: &[f32], channels: usize) -> Vec<Vec<f32>> {
    let mut separated = vec![Vec::with_capacity(samples.len() / channels); channels];
    for frame in samples.chunks_exact(channels) {
        for (ch, &v) in frame.iter().enumerate() {
            separated[ch].push(v);
        }
    }
    separated
}

fn interleave(channels: &[Vec<f32>]) -> Vec<f32> {
    if channels.is_empty() {
        return Vec::new();
    }
    let frames = channels[0].len();
    let mut out = Vec::with_capacity(frames * channels.len());
    for f in 0..frames {
        for ch in channels {
            out.push(ch[f]);
        }
    }
    out
}

fn linear_resample(input: &[f32], source_rate: u32, target_rate: u32) -> Vec<f32> {
    if input.is_empty() || source_rate == target_rate {
        return input.to_vec();
    }
    let ratio = target_rate as f64 / source_rate as f64;
    let output_len = ((input.len() as f64) * ratio).round().max(1.0) as usize;
    let mut output = Vec::with_capacity(output_len);
    for i in 0..output_len {
        let src_pos = i as f64 / ratio;
        let left = src_pos.floor() as usize;
        let right = (left + 1).min(input.len() - 1);
        let blend = (src_pos - left as f64) as f32;
        output.push(input[left] * (1.0 - blend) + input[right] * blend);
    }
    output
}

fn write_wav(path: &Path, sample_rate: u32, channels: u16, samples: &[f32]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Unable to create directory {}", parent.display()))?;
    }
    let spec = WavSpec {
        channels,
        sample_rate,
        bits_per_sample: 16,
        sample_format: SampleFormat::Int,
    };
    let mut writer = WavWriter::create(path, spec)
        .with_context(|| format!("Unable to create WAV file {}", path.display()))?;
    for &s in samples {
        writer.write_sample((s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16)?;
    }
    writer.finalize()?;
    Ok(())
}

fn transcode_to_mp3(input_wav: &Path, output_mp3: &Path) -> Result<()> {
    if let Some(parent) = output_mp3.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Unable to create directory {}", parent.display()))?;
    }
    let status = Command::new("ffmpeg")
        .args([
            "-y",
            "-i",
            input_wav.to_str().unwrap_or_default(),
            "-codec:a",
            "libmp3lame",
            "-q:a",
            "2",
            output_mp3.to_str().unwrap_or_default(),
        ])
        .status();
    match status {
        Ok(s) if s.success() => Ok(()),
        Ok(s) => bail!("FFmpeg exited with status {s}"),
        Err(e) if e.kind() == ErrorKind::NotFound => Err(anyhow!(
            "FFmpeg not found. MP3 export requires FFmpeg to be installed."
        )),
        Err(e) => Err(e.into()),
    }
}

// Scale samples so the peak reaches target_dbfs (e.g. -1.0 dBFS)
fn peak_normalize(samples: &mut [f32], target_dbfs: f32) {
    let peak = samples.iter().map(|s| s.abs()).fold(0.0f32, f32::max);
    if peak < 1e-6 {
        return; // silence, nothing to do
    }
    let target_linear = 10.0_f32.powf(target_dbfs / 20.0);
    let gain = target_linear / peak;
    for s in samples {
        *s *= gain;
    }
}

pub fn boost_volume(path: &str, gain_db: f32) -> Result<()> {
    use hound::WavReader;

    let mut reader = WavReader::open(path)
        .with_context(|| format!("Unable to open {path}"))?;
    let spec = reader.spec();

    let gain = 10.0_f32.powf(gain_db / 20.0);

    let samples: Vec<f32> = match spec.sample_format {
        SampleFormat::Int => reader
            .samples::<i16>()
            .map(|s| s.map(|v| (v as f32 / i16::MAX as f32) * gain))
            .collect::<hound::Result<_>>()?,
        SampleFormat::Float => reader
            .samples::<f32>()
            .map(|s| s.map(|v| v * gain))
            .collect::<hound::Result<_>>()?,
    };

    let samples: Vec<f32> = samples.into_iter().map(|s| s.clamp(-1.0, 1.0)).collect();

    write_wav(Path::new(path), spec.sample_rate, spec.channels, &samples)
}

pub fn ensure_output_path(path: &str, format: &str) -> Result<PathBuf> {
    let p = PathBuf::from(path);
    if p.extension().is_none() {
        return Ok(if format.eq_ignore_ascii_case("mp3") {
            p.with_extension("mp3")
        } else {
            p.with_extension("wav")
        });
    }
    Ok(p)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicBool;

    // ── peak_normalize ──────────────────────────────────────────────────────

    #[test]
    fn peak_normalize_scales_to_target() {
        let mut samples = vec![0.0f32, 0.5, -0.5, 0.25];
        peak_normalize(&mut samples, -1.0);
        let target = 10.0_f32.powf(-1.0 / 20.0);
        let peak = samples.iter().map(|s| s.abs()).fold(0.0f32, f32::max);
        assert!((peak - target).abs() < 1e-5, "peak={peak} expected≈{target}");
    }

    #[test]
    fn peak_normalize_silence_is_noop() {
        let mut samples = vec![0.0f32, 0.0, 0.0];
        let before = samples.clone();
        peak_normalize(&mut samples, -1.0);
        assert_eq!(samples, before);
    }

    #[test]
    fn peak_normalize_already_at_target() {
        let target = 10.0_f32.powf(-1.0 / 20.0);
        let mut samples = vec![target, -target];
        peak_normalize(&mut samples, -1.0);
        let peak = samples.iter().map(|s| s.abs()).fold(0.0f32, f32::max);
        assert!((peak - target).abs() < 1e-5);
    }

    // ── linear_resample ─────────────────────────────────────────────────────

    #[test]
    fn linear_resample_same_rate_is_identity() {
        let input = vec![0.1f32, 0.2, 0.3, 0.4];
        let output = linear_resample(&input, 44_100, 44_100);
        assert_eq!(output, input);
    }

    #[test]
    fn linear_resample_empty_input() {
        let output = linear_resample(&[], 44_100, 48_000);
        assert!(output.is_empty());
    }

    #[test]
    fn linear_resample_upsample_increases_length() {
        let input = vec![0.0f32; 100];
        let output = linear_resample(&input, 44_100, 48_000);
        // 100 * (48000/44100) ≈ 109
        assert!(output.len() > input.len());
    }

    #[test]
    fn linear_resample_downsample_decreases_length() {
        let input = vec![0.0f32; 100];
        let output = linear_resample(&input, 48_000, 44_100);
        assert!(output.len() < input.len());
    }

    #[test]
    fn linear_resample_constant_signal_stays_constant() {
        let input = vec![0.5f32; 200];
        let output = linear_resample(&input, 44_100, 48_000);
        for s in &output {
            assert!((*s - 0.5).abs() < 1e-5, "expected 0.5, got {s}");
        }
    }

    // ── deinterleave / interleave ────────────────────────────────────────────

    #[test]
    fn deinterleave_stereo() {
        let interleaved = vec![1.0f32, 2.0, 3.0, 4.0, 5.0, 6.0];
        let channels = deinterleave(&interleaved, 2);
        assert_eq!(channels[0], vec![1.0, 3.0, 5.0]);
        assert_eq!(channels[1], vec![2.0, 4.0, 6.0]);
    }

    #[test]
    fn deinterleave_mono() {
        let samples = vec![1.0f32, 2.0, 3.0];
        let channels = deinterleave(&samples, 1);
        assert_eq!(channels[0], samples);
    }

    #[test]
    fn interleave_stereo() {
        let channels = vec![vec![1.0f32, 3.0, 5.0], vec![2.0, 4.0, 6.0]];
        let out = interleave(&channels);
        assert_eq!(out, vec![1.0f32, 2.0, 3.0, 4.0, 5.0, 6.0]);
    }

    #[test]
    fn interleave_empty() {
        let out = interleave(&[]);
        assert!(out.is_empty());
    }

    #[test]
    fn deinterleave_interleave_roundtrip() {
        let original = vec![0.1f32, 0.2, 0.3, 0.4, 0.5, 0.6];
        let channels = deinterleave(&original, 2);
        let restored = interleave(&channels);
        assert_eq!(original, restored);
    }

    // ── ensure_output_path ───────────────────────────────────────────────────

    #[test]
    fn ensure_output_path_preserves_existing_extension() {
        let p = ensure_output_path("/tmp/out.wav", "wav").unwrap();
        assert_eq!(p.extension().and_then(|e| e.to_str()), Some("wav"));
    }

    #[test]
    fn ensure_output_path_adds_wav_extension() {
        let p = ensure_output_path("/tmp/out", "wav").unwrap();
        assert_eq!(p.extension().and_then(|e| e.to_str()), Some("wav"));
    }

    #[test]
    fn ensure_output_path_adds_mp3_extension() {
        let p = ensure_output_path("/tmp/out", "mp3").unwrap();
        assert_eq!(p.extension().and_then(|e| e.to_str()), Some("mp3"));
    }

    // ── boost_volume (integration with tempfile) ─────────────────────────────

    #[test]
    fn boost_volume_increases_amplitude() {
        use hound::{SampleFormat, WavSpec, WavWriter};
        use tempfile::NamedTempFile;

        let tmp = NamedTempFile::with_suffix(".wav").unwrap();
        let path = tmp.path().to_str().unwrap();

        let spec = WavSpec {
            channels: 1,
            sample_rate: 44_100,
            bits_per_sample: 16,
            sample_format: SampleFormat::Int,
        };
        {
            let mut w = WavWriter::create(path, spec).unwrap();
            for _ in 0..100 {
                w.write_sample(16384i16).unwrap();
            }
            w.finalize().unwrap();
        }

        boost_volume(path, 6.0).unwrap();

        use hound::WavReader;
        let mut r = WavReader::open(path).unwrap();
        let samples: Vec<i16> = r.samples::<i16>().map(|s| s.unwrap()).collect();
        // +6 dB ≈ ×2 linear; 16384 * 2 = 32768 → clamped to 32767
        assert!(samples[0] >= 30000, "expected boosted sample, got {}", samples[0]);
    }

    // ── denoise_channel: smoke tests ─────────────────────────────────────────

    #[test]
    fn denoise_channel_processes_silent_input() {
        let cancel = AtomicBool::new(false);
        let input = vec![0.0f32; FRAME_SIZE * 3];
        let output = denoise_channel(&input, 1, &cancel, |_, _| {}).unwrap();
        assert_eq!(output.len(), input.len());
        let max = output.iter().map(|s| s.abs()).fold(0.0f32, f32::max);
        assert!(max < 0.05, "unexpected loud output for silent input: {max}");
    }

    #[test]
    fn denoise_channel_two_passes() {
        let cancel = AtomicBool::new(false);
        let input = vec![0.0f32; FRAME_SIZE * 5];
        let out = denoise_channel(&input, 2, &cancel, |_, _| {}).unwrap();
        assert_eq!(out.len(), input.len());
    }

    #[test]
    fn denoise_channel_cancels_immediately() {
        let cancel = AtomicBool::new(true);
        let input = vec![0.0f32; FRAME_SIZE * 10];
        let result = denoise_channel(&input, 1, &cancel, |_, _| {});
        assert!(result.is_err());
    }
}
