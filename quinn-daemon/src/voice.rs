#[cfg(feature = "voice")]
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

#[cfg(feature = "voice")]
const TARGET_RATE: u32 = 16_000;
#[cfg(feature = "voice")]
const CAPTURE_SECONDS: u64 = 4;

#[derive(Clone)]
pub struct VoiceEngine {
    #[cfg(feature = "voice")]
    model: Arc<vosk::Model>,
}

impl VoiceEngine {
    pub fn new() -> Result<Self, String> {
        #[cfg(feature = "voice")]
        {
            let path = std::env::var_os("QUINN_STT_MODEL")
                .map(PathBuf::from)
                .or_else(default_model_path)
                .ok_or_else(|| "could not determine the Vosk model path".to_string())?;
            let model_path = path
                .to_str()
                .ok_or_else(|| format!("Vosk model path is not valid UTF-8: {}", path.display()))?;
            let model = vosk::Model::new(model_path.to_owned())
                .ok_or_else(|| format!("could not load Vosk model at {}", path.display()))?;
            return Ok(Self {
                model: Arc::new(model),
            });
        }

        #[cfg(not(feature = "voice"))]
        {
            Err(
                "voice support is not compiled; rebuild quinn-daemon with --features voice"
                    .to_string(),
            )
        }
    }

    pub fn listen(&self) -> Result<String, String> {
        #[cfg(feature = "voice")]
        {
            let (samples, sample_rate) = capture_audio()?;
            let mono_16k = resample_to_16khz(&samples, sample_rate);
            let mut recognizer = vosk::Recognizer::new(&self.model, TARGET_RATE as f32)
                .ok_or_else(|| "could not initialize Vosk recognizer".to_string())?;
            recognizer.set_max_alternatives(0);
            recognizer
                .accept_waveform(&mono_16k)
                .map_err(|e| format!("Vosk rejected microphone audio: {e:?}"))?;
            let result = recognizer.final_result();
            let text = result
                .single()
                .map(|r| r.text.trim().to_string())
                .unwrap_or_default();
            if text.is_empty() {
                Err("no speech was recognized".to_string())
            } else {
                Ok(text)
            }
        }

        #[cfg(not(feature = "voice"))]
        {
            Err("voice support is not compiled".to_string())
        }
    }
}

#[cfg(feature = "voice")]
fn default_model_path() -> Option<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from)?;
    Some(home.join(".local/share/quinn/voice-model"))
}

#[cfg(feature = "voice")]
fn capture_audio() -> Result<(Vec<f32>, u32), String> {
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .ok_or_else(|| "no default microphone was found".to_string())?;
    let supported = device
        .default_input_config()
        .map_err(|e| format!("could not query microphone configuration: {e}"))?;
    let sample_rate = supported.sample_rate().0;
    let channels = supported.channels() as usize;
    if channels == 0 {
        return Err("microphone reported zero channels".to_string());
    }

    let samples = Arc::new(Mutex::new(Vec::<f32>::new()));
    let callback_samples = Arc::clone(&samples);
    let err_fn = |err| eprintln!("Quinn microphone stream error: {err}");
    let config: cpal::StreamConfig = supported.clone().into();

    let stream = match supported.sample_format() {
        cpal::SampleFormat::F32 => device
            .build_input_stream(
                &config,
                move |data: &[f32], _| push_mono(data, channels, &callback_samples),
                err_fn,
                None,
            )
            .map_err(|e| format!("could not open microphone stream: {e}"))?,
        cpal::SampleFormat::I16 => {
            let callback_samples = Arc::clone(&samples);
            device
                .build_input_stream(
                    &config,
                    move |data: &[i16], _| {
                        let mut out = callback_samples.lock().expect("microphone mutex poisoned");
                        for frame in data.chunks(channels) {
                            let sum: f32 = frame.iter().map(|s| *s as f32 / 32768.0).sum();
                            out.push(sum / frame.len() as f32);
                        }
                    },
                    err_fn,
                    None,
                )
                .map_err(|e| format!("could not open microphone stream: {e}"))?
        }
        cpal::SampleFormat::U16 => {
            let callback_samples = Arc::clone(&samples);
            device
                .build_input_stream(
                    &config,
                    move |data: &[u16], _| {
                        let mut out = callback_samples.lock().expect("microphone mutex poisoned");
                        for frame in data.chunks(channels) {
                            let sum: f32 = frame
                                .iter()
                                .map(|s| (*s as f32 / 65535.0) * 2.0 - 1.0)
                                .sum();
                            out.push(sum / frame.len() as f32);
                        }
                    },
                    err_fn,
                    None,
                )
                .map_err(|e| format!("could not open microphone stream: {e}"))?
        }
        format => return Err(format!("unsupported microphone sample format: {format:?}")),
    };

    stream
        .play()
        .map_err(|e| format!("could not start microphone stream: {e}"))?;
    thread::sleep(Duration::from_secs(CAPTURE_SECONDS));
    drop(stream);

    let captured = samples
        .lock()
        .map_err(|_| "microphone sample buffer was poisoned".to_string())?
        .clone();
    if captured.is_empty() {
        return Err("microphone captured no audio".to_string());
    }
    Ok((captured, sample_rate))
}

#[cfg(feature = "voice")]
fn push_mono(data: &[f32], channels: usize, destination: &Arc<Mutex<Vec<f32>>>) {
    let mut out = destination.lock().expect("microphone mutex poisoned");
    for frame in data.chunks(channels) {
        let sum: f32 = frame.iter().copied().sum();
        out.push(sum / frame.len() as f32);
    }
}

#[cfg(feature = "voice")]
fn resample_to_16khz(input: &[f32], source_rate: u32) -> Vec<i16> {
    if source_rate == TARGET_RATE {
        return input
            .iter()
            .map(|sample| (sample.clamp(-1.0, 1.0) * 32767.0) as i16)
            .collect();
    }

    if input.is_empty() {
        return Vec::new();
    }
    let output_len = ((input.len() as u64 * TARGET_RATE as u64) / source_rate as u64) as usize;
    let ratio = source_rate as f64 / TARGET_RATE as f64;
    let mut output = Vec::with_capacity(output_len);
    for i in 0..output_len {
        let position = i as f64 * ratio;
        let left = position.floor() as usize;
        let right = (left + 1).min(input.len().saturating_sub(1));
        let fraction = (position - left as f64) as f32;
        let sample = input[left] * (1.0 - fraction) + input[right] * fraction;
        output.push((sample.clamp(-1.0, 1.0) * 32767.0) as i16);
    }
    output
}
