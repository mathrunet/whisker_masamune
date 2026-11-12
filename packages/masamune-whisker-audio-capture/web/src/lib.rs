//! Web Host implementation for `masamune-whisker-audio-capture`: `getUserMedia` and a
//! `ScriptProcessorNode`, resampled to PCM16 little-endian mono with linear interpolation.
//!
//! A `ScriptProcessorNode` needs no separately served worklet script. The page must be served
//! over HTTPS (or localhost) for `getUserMedia` to be available.

use std::cell::RefCell;
use std::rc::Rc;

use wasm_bindgen_futures::{JsFuture, spawn_local};
use whisker::runtime::module::{ModuleEventEmitter, ModulePromise};
use whisker_web::wasm_bindgen::closure::Closure;
use whisker_web::wasm_bindgen::{JsCast, JsValue};
use whisker_web::{ModuleDefinition, WhiskerModule, WhiskerValue};

const MODULE_NAME: &str = "masamune-whisker-audio-capture:AudioCapture";
const EVENT: &str = "audio";
const PERMISSION_DENIED: &str = "permission_denied";
/// Frames per `audioprocess` callback (about 85 ms at 48 kHz).
const BUFFER_SIZE: u32 = 4096;

thread_local! {
    static CAPTURE: RefCell<Option<Capture>> = const { RefCell::new(None) };
}

type ProcessCallback = Closure<dyn FnMut(web_sys::AudioProcessingEvent)>;

/// The running capture. The nodes are filled in once `getUserMedia` resolves.
struct Capture {
    id: String,
    context: web_sys::AudioContext,
    stream: Option<web_sys::MediaStream>,
    source: Option<web_sys::MediaStreamAudioSourceNode>,
    processor: Option<web_sys::ScriptProcessorNode>,
    mute: Option<web_sys::GainNode>,
    callback: Option<ProcessCallback>,
    resampler: Rc<RefCell<Resampler>>,
}

impl Capture {
    /// Releases the microphone and the audio graph and returns the samples after the last chunk.
    fn close(self) -> Vec<u8> {
        if let Some(processor) = &self.processor {
            processor.set_onaudioprocess(None);
            let _ = processor.disconnect();
        }
        if let Some(source) = &self.source {
            let _ = source.disconnect();
        }
        if let Some(mute) = &self.mute {
            let _ = mute.disconnect();
        }
        if let Some(stream) = &self.stream {
            stop_tracks(stream);
        }
        let _ = self.context.close();
        drop(self.callback);
        std::mem::take(&mut self.resampler.borrow_mut().pending)
    }
}

struct AudioCaptureModule;

#[whisker_web::WhiskerModule]
impl WhiskerModule for AudioCaptureModule {
    type Definition = ModuleDefinition;

    fn definition() -> Self::Definition {
        ModuleDefinition::new()
            .name(MODULE_NAME)
            .event(EVENT)
            .async_function("start", |args, promise, emitter| {
                let json = args_json(args);
                let id = json["id"].as_str().unwrap_or_default().to_owned();
                let sample_rate = json["sampleRate"].as_u64().unwrap_or(16_000).max(1) as u32;
                let chunk_ms = json["chunkMs"].as_u64().unwrap_or(100).max(1) as u32;
                start(id, sample_rate, chunk_ms, promise, emitter.clone());
            })
            .async_function("stop", |args, promise, _| {
                let id = args_json(args)["id"]
                    .as_str()
                    .unwrap_or_default()
                    .to_owned();
                let value = match finish(&id) {
                    Some(rest) => serde_json::json!({ "data": base64(&rest) }),
                    None => serde_json::Value::Null,
                };
                promise.resolve(ok(value));
            })
            .function("cancel", |args, _| {
                let id = args_json(args)["id"]
                    .as_str()
                    .unwrap_or_default()
                    .to_owned();
                finish(&id);
                ok(serde_json::Value::Bool(true))
            })
    }
}

fn start(
    id: String,
    sample_rate: u32,
    chunk_ms: u32,
    promise: ModulePromise,
    emitter: ModuleEventEmitter,
) {
    if let Some(previous) = CAPTURE.with(|slot| slot.borrow_mut().take()) {
        previous.close();
    }
    // Created inside the user's tap: Safari keeps a context created later suspended.
    let context = match web_sys::AudioContext::new() {
        Ok(context) => context,
        Err(error) => return promise.reject(js_message(&error)),
    };
    let _ = context.resume();
    let media = web_sys::window()
        .map(|window| window.navigator())
        .and_then(|navigator| navigator.media_devices().ok());
    let Some(media) = media else {
        let _ = context.close();
        return promise.reject("start_failed: getUserMedia is not available");
    };
    let constraints = web_sys::MediaStreamConstraints::new();
    constraints.set_audio(&JsValue::TRUE);
    let request = match media.get_user_media_with_constraints(&constraints) {
        Ok(request) => request,
        Err(error) => {
            let _ = context.close();
            return promise.reject(js_message(&error));
        }
    };
    let chunk_bytes = (sample_rate as usize * 2 * chunk_ms as usize / 1000).max(2);
    let resampler = Rc::new(RefCell::new(Resampler::new(
        f64::from(context.sample_rate()),
        f64::from(sample_rate),
        chunk_bytes,
    )));
    CAPTURE.with(|slot| {
        *slot.borrow_mut() = Some(Capture {
            id: id.clone(),
            context,
            stream: None,
            source: None,
            processor: None,
            mute: None,
            callback: None,
            resampler,
        });
    });
    spawn_local(async move {
        let stream = match JsFuture::from(request).await {
            Ok(stream) => stream.unchecked_into::<web_sys::MediaStream>(),
            Err(error) => {
                finish(&id);
                return promise.reject(permission_message(&error));
            }
        };
        let connected = CAPTURE.with(|slot| {
            let mut slot = slot.borrow_mut();
            match slot.as_mut() {
                Some(capture) if capture.id == id => {
                    Some(connect(capture, stream.clone(), &emitter))
                }
                _ => None,
            }
        });
        match connected {
            Some(Ok(())) => promise.resolve(ok(serde_json::Value::Bool(true))),
            Some(Err(error)) => {
                finish(&id);
                promise.reject(js_message(&error));
            }
            None => {
                // Stopped while the permission prompt was open.
                stop_tracks(&stream);
                promise.reject("cancelled");
            }
        }
    });
}

/// Wires microphone → processor → muted gain → destination (some browsers only process audio
/// that reaches the destination).
fn connect(
    capture: &mut Capture,
    stream: web_sys::MediaStream,
    emitter: &ModuleEventEmitter,
) -> Result<(), JsValue> {
    capture.stream = Some(stream.clone());
    let source = capture.context.create_media_stream_source(&stream)?;
    let processor = capture
        .context
        .create_script_processor_with_buffer_size_and_number_of_input_channels_and_number_of_output_channels(
            BUFFER_SIZE,
            1,
            1,
        )?;
    let mute = capture.context.create_gain()?;
    mute.gain().set_value(0.0);
    let resampler = capture.resampler.clone();
    let emitter = emitter.clone();
    let id = capture.id.clone();
    let callback: ProcessCallback = Closure::new(move |event: web_sys::AudioProcessingEvent| {
        let Ok(buffer) = event.input_buffer() else {
            return;
        };
        let Ok(samples) = buffer.get_channel_data(0) else {
            return;
        };
        let chunks = resampler.borrow_mut().push(&samples);
        for chunk in chunks {
            let payload = serde_json::json!({ "id": id, "data": base64(&chunk) });
            emitter.emit(EVENT, WhiskerValue::String(payload.to_string()));
        }
    });
    processor.set_onaudioprocess(Some(callback.as_ref().unchecked_ref()));
    source.connect_with_audio_node(&processor)?;
    processor.connect_with_audio_node(&mute)?;
    mute.connect_with_audio_node(&capture.context.destination())?;
    capture.source = Some(source);
    capture.processor = Some(processor);
    capture.mute = Some(mute);
    capture.callback = Some(callback);
    Ok(())
}

/// Stops the capture `id` and returns its remaining audio, or `None` when it is not running.
fn finish(id: &str) -> Option<Vec<u8>> {
    let capture = CAPTURE.with(|slot| {
        let mut slot = slot.borrow_mut();
        match slot.as_ref() {
            Some(capture) if capture.id == id => slot.take(),
            _ => None,
        }
    })?;
    Some(capture.close())
}

fn stop_tracks(stream: &web_sys::MediaStream) {
    for track in stream.get_tracks().iter() {
        if let Ok(track) = track.dyn_into::<web_sys::MediaStreamTrack>() {
            track.stop();
        }
    }
}

/// Linear-interpolation resampler from the context rate to PCM16 at the target rate.
struct Resampler {
    /// Input samples per output sample.
    step: f64,
    /// Position of the next output sample; 0 is the last sample of the previous buffer.
    position: f64,
    last: f32,
    chunk_bytes: usize,
    pending: Vec<u8>,
}

impl Resampler {
    fn new(input_rate: f64, output_rate: f64, chunk_bytes: usize) -> Self {
        Self {
            step: (input_rate / output_rate).max(f64::MIN_POSITIVE),
            position: 1.0,
            last: 0.0,
            chunk_bytes,
            pending: Vec::new(),
        }
    }

    /// Adds input samples and returns the chunks that became full.
    fn push(&mut self, input: &[f32]) -> Vec<Vec<u8>> {
        if input.is_empty() {
            return Vec::new();
        }
        let last = self.last;
        let sample = |index: usize| if index == 0 { last } else { input[index - 1] };
        let available = input.len() as f64;
        let mut position = self.position;
        while position < available {
            let index = position.floor() as usize;
            let fraction = (position - index as f64) as f32;
            let value = sample(index) * (1.0 - fraction) + sample(index + 1) * fraction;
            let pcm = (value.clamp(-1.0, 1.0) * f32::from(i16::MAX)) as i16;
            self.pending.extend_from_slice(&pcm.to_le_bytes());
            position += self.step;
        }
        self.position = position - available;
        self.last = input[input.len() - 1];
        let mut chunks = Vec::new();
        while self.pending.len() >= self.chunk_bytes {
            let rest = self.pending.split_off(self.chunk_bytes);
            chunks.push(std::mem::replace(&mut self.pending, rest));
        }
        chunks
    }
}

fn args_json(args: &[WhiskerValue]) -> serde_json::Value {
    match args.first() {
        Some(WhiskerValue::String(json)) => serde_json::from_str(json).unwrap_or_default(),
        _ => serde_json::Value::Null,
    }
}

fn ok(value: serde_json::Value) -> WhiskerValue {
    WhiskerValue::String(serde_json::json!({ "value": value }).to_string())
}

/// Base64 through `btoa` (every byte becomes one Latin-1 character).
fn base64(bytes: &[u8]) -> String {
    let binary: String = bytes.iter().map(|&byte| char::from(byte)).collect();
    web_sys::window()
        .and_then(|window| window.btoa(&binary).ok())
        .unwrap_or_default()
}

fn js_message(error: &JsValue) -> String {
    if let Some(exception) = error.dyn_ref::<web_sys::DomException>() {
        return exception.message();
    }
    error.as_string().unwrap_or_else(|| format!("{error:?}"))
}

fn permission_message(error: &JsValue) -> String {
    match error.dyn_ref::<web_sys::DomException>() {
        Some(exception)
            if matches!(
                exception.name().as_str(),
                "NotAllowedError" | "SecurityError"
            ) =>
        {
            PERMISSION_DENIED.to_owned()
        }
        _ => js_message(error),
    }
}

#[cfg(test)]
mod tests {
    use super::Resampler;

    #[test]
    fn keeps_the_rate_when_equal() {
        let mut resampler = Resampler::new(16_000.0, 16_000.0, 4);
        // The last sample is interpolated with the next buffer, so it comes out one call later.
        let chunks = resampler.push(&[0.5, -0.5, 1.0]);
        let samples = |chunk: &[u8]| {
            chunk
                .chunks(2)
                .map(|pair| i16::from_le_bytes([pair[0], pair[1]]))
                .collect::<Vec<_>>()
        };
        assert_eq!(chunks.len(), 1);
        assert_eq!(samples(&chunks[0]), vec![16383, -16383]);
        assert!(resampler.pending.is_empty());
        let chunks = resampler.push(&[0.0]);
        assert!(chunks.is_empty());
        assert_eq!(samples(&resampler.pending), vec![32767]);
    }

    #[test]
    fn downsamples_by_three() {
        let mut resampler = Resampler::new(48_000.0, 16_000.0, 1_000);
        let chunks = resampler.push(&[0.0; 4_800]);
        // 1600 samples: three chunks of 1000 bytes and 200 bytes left.
        assert_eq!(chunks.len(), 3);
        assert!(chunks.iter().all(|chunk| chunk.len() == 1_000));
        assert_eq!(resampler.pending.len(), 200);
    }
}
