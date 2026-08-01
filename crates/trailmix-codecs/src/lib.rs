//! Feature-gated file decoding adapters for trail mix.

use std::{
    error::Error as StdError,
    fmt,
    fs::File,
    path::{Path, PathBuf},
};

use symphonia::core::{
    audio::sample::Sample,
    codecs::audio::AudioDecoderOptions,
    errors::Error as SymphoniaError,
    formats::{FormatOptions, TrackType, probe::Hint},
    io::{MediaSourceStream, MediaSourceStreamOptions},
    meta::MetadataOptions,
};

/// Fully decoded mono PCM and source information.
#[derive(Debug, Clone, PartialEq)]
pub struct DecodedAudio {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
    pub source_channels: usize,
}

impl DecodedAudio {
    #[must_use]
    pub fn duration_seconds(&self) -> f64 {
        if self.sample_rate == 0 {
            return 0.0;
        }
        self.samples.len() as f64 / f64::from(self.sample_rate)
    }
}

#[derive(Debug)]
pub enum DecodeError {
    Open {
        path: PathBuf,
        source: std::io::Error,
    },
    UnsupportedFormat(String),
    NoAudioTrack,
    MissingCodecParameters,
    UnsupportedCodec(String),
    Stream(String),
    EmptyAudio,
    SampleRateChanged {
        initial: u32,
        encountered: u32,
    },
}

impl fmt::Display for DecodeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Open { path, source } => {
                write!(formatter, "could not open {}: {source}", path.display())
            }
            Self::UnsupportedFormat(message) => {
                write!(formatter, "unsupported audio format: {message}")
            }
            Self::NoAudioTrack => formatter.write_str("file contains no audio track"),
            Self::MissingCodecParameters => {
                formatter.write_str("audio track has no codec parameters")
            }
            Self::UnsupportedCodec(message) => {
                write!(formatter, "unsupported audio codec: {message}")
            }
            Self::Stream(message) => write!(formatter, "audio stream failed: {message}"),
            Self::EmptyAudio => formatter.write_str("audio track decoded to no samples"),
            Self::SampleRateChanged {
                initial,
                encountered,
            } => write!(
                formatter,
                "sample rate changed while decoding from {initial} Hz to {encountered} Hz"
            ),
        }
    }
}

impl StdError for DecodeError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            Self::Open { source, .. } => Some(source),
            _ => None,
        }
    }
}

/// Decode the default audio track and downmix it to mono `f32` PCM.
///
/// Enabled formats depend on this crate's Cargo features. `common-codecs`
/// enables MP3, FLAC, AIFF, WAV, AAC-in-MP4, and ALAC-in-MP4 support.
///
/// # Errors
///
/// Returns [`DecodeError`] when the file cannot be opened, its format or codec
/// is unavailable under the enabled features, or its audio stream is invalid.
pub fn decode_file(path: impl AsRef<Path>) -> Result<DecodedAudio, DecodeError> {
    let path = path.as_ref();
    let file = File::open(path).map_err(|source| DecodeError::Open {
        path: path.to_path_buf(),
        source,
    })?;
    let stream = MediaSourceStream::new(Box::new(file), MediaSourceStreamOptions::default());

    let mut hint = Hint::new();
    if let Some(extension) = path.extension().and_then(|value| value.to_str()) {
        hint.with_extension(extension);
    }

    let mut format = symphonia::default::get_probe()
        .probe(
            &hint,
            stream,
            FormatOptions::default(),
            MetadataOptions::default(),
        )
        .map_err(|error| DecodeError::UnsupportedFormat(error.to_string()))?;
    let track = format
        .default_track(TrackType::Audio)
        .ok_or(DecodeError::NoAudioTrack)?;
    let codec_parameters = track
        .codec_params
        .as_ref()
        .ok_or(DecodeError::MissingCodecParameters)?
        .audio()
        .ok_or(DecodeError::MissingCodecParameters)?;
    let mut decoder = symphonia::default::get_codecs()
        .make_audio_decoder(codec_parameters, &AudioDecoderOptions::default())
        .map_err(|error| DecodeError::UnsupportedCodec(error.to_string()))?;
    let track_id = track.id;

    let mut mono = Vec::new();
    let mut interleaved = Vec::new();
    let mut sample_rate = None;
    let mut source_channels = 0;

    loop {
        let packet = match format.next_packet() {
            Ok(Some(packet)) => packet,
            Ok(None) => break,
            Err(SymphoniaError::ResetRequired) => {
                return Err(DecodeError::Stream(
                    "chained streams are not supported".to_owned(),
                ));
            }
            Err(error) => return Err(DecodeError::Stream(error.to_string())),
        };
        if packet.track_id != track_id {
            continue;
        }

        let audio_buffer = match decoder.decode(&packet) {
            Ok(audio_buffer) => audio_buffer,
            Err(SymphoniaError::DecodeError(_) | SymphoniaError::IoError(_)) => continue,
            Err(error) => return Err(DecodeError::Stream(error.to_string())),
        };
        let packet_sample_rate = audio_buffer.spec().rate();
        if let Some(initial) = sample_rate {
            if initial != packet_sample_rate {
                return Err(DecodeError::SampleRateChanged {
                    initial,
                    encountered: packet_sample_rate,
                });
            }
        } else {
            sample_rate = Some(packet_sample_rate);
        }

        let channels = audio_buffer.spec().channels().count();
        if channels == 0 {
            continue;
        }
        source_channels = source_channels.max(channels);
        interleaved.resize(audio_buffer.samples_interleaved(), f32::MID);
        audio_buffer.copy_to_slice_interleaved(&mut interleaved);
        mono.extend(
            interleaved
                .chunks_exact(channels)
                .map(|frame| frame.iter().sum::<f32>() / channels as f32),
        );
    }

    if mono.is_empty() {
        return Err(DecodeError::EmptyAudio);
    }
    Ok(DecodedAudio {
        samples: mono,
        sample_rate: sample_rate.unwrap_or_default(),
        source_channels,
    })
}

#[cfg(all(test, feature = "wav"))]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use hound::{SampleFormat, WavSpec, WavWriter};

    use super::*;

    #[test]
    fn decodes_stereo_wav_to_mono() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("trailmix-codecs-{nonce}.wav"));
        let mut writer = WavWriter::create(
            &path,
            WavSpec {
                channels: 2,
                sample_rate: 8_000,
                bits_per_sample: 16,
                sample_format: SampleFormat::Int,
            },
        )
        .expect("create WAV");
        writer.write_sample(i16::MAX).expect("left sample");
        writer.write_sample(i16::MIN).expect("right sample");
        writer.write_sample(16_384_i16).expect("left sample");
        writer.write_sample(16_384_i16).expect("right sample");
        writer.finalize().expect("finalize WAV");

        let decoded = decode_file(&path).expect("decode WAV");
        std::fs::remove_file(path).expect("remove WAV");

        assert_eq!(decoded.sample_rate, 8_000);
        assert_eq!(decoded.source_channels, 2);
        assert_eq!(decoded.samples.len(), 2);
        assert!(decoded.samples[0].abs() < 0.001);
        assert!((decoded.samples[1] - 0.5).abs() < 0.001);
    }
}
