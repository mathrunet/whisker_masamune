//! One voice input from the microphone button to the last committed transcript
//! (`AIDebugVoiceSession` of the Flutter package).
//!
//! SamuraiAI issues a single-use token; the app captures the microphone with
//! `masamune-whisker-audio-capture` and streams it straight to ElevenLabs Scribe. SamuraiAI never
//! relays audio.

use std::collections::VecDeque;
use std::rc::Rc;
use std::time::Duration;

use futures_channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};
use futures_util::future::LocalBoxFuture;
use futures_util::{FutureExt, StreamExt};
use masamune_whisker_audio_capture::{
    AudioCapture, AudioCaptureEvent, AudioCaptureOptions, NativeError, is_permission_denied,
};

use super::core::{Output, VoiceCore};
use super::platform::{Socket, SocketEvent, now_ms, sleep};
use super::protocol::{CHUNK_MS, token_error_message};
use crate::controller::AiDebugController;

const TICK: Duration = Duration::from_millis(250);

/// Progress of a voice input.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum VoiceState {
    Idle,
    /// Issuing the token, asking for the microphone and connecting.
    Connecting,
    Recording,
    /// Waiting for the final committed transcript.
    Stopping,
}

/// Where a session reports to. Every callback runs on the UI thread.
#[derive(Clone)]
pub(crate) struct VoiceCallbacks {
    pub(crate) state: Rc<dyn Fn(VoiceState)>,
    pub(crate) partial: Rc<dyn Fn(String)>,
    pub(crate) committed: Rc<dyn Fn(String)>,
    pub(crate) error: Rc<dyn Fn(String)>,
}

enum Event {
    Audio(Vec<u8>),
    AudioFailed(String),
    Socket(SocketEvent),
    Tick,
    Stop,
    Cancel,
}

/// Controls a running session. Dropping it cancels the session.
pub(crate) struct VoiceHandle {
    sender: UnboundedSender<Event>,
}

impl VoiceHandle {
    /// Commits the remaining audio and waits for the last transcript.
    pub(crate) fn stop(&self) {
        let _ = self.sender.unbounded_send(Event::Stop);
    }

    /// Closes everything without waiting.
    pub(crate) fn cancel(&self) {
        let _ = self.sender.unbounded_send(Event::Cancel);
    }
}

impl Drop for VoiceHandle {
    fn drop(&mut self) {
        self.cancel();
    }
}

/// Starts a session on the controller's task spawner (attached by the overlay).
pub(crate) fn start(controller: &AiDebugController, callbacks: VoiceCallbacks) -> VoiceHandle {
    let (sender, receiver) = unbounded();
    let session = Session {
        controller: controller.clone(),
        callbacks,
        sender: sender.clone(),
        receiver,
        backlog: VecDeque::new(),
    };
    (session.callbacks.state)(VoiceState::Connecting);
    controller.spawn(session.run());
    VoiceHandle { sender }
}

struct Session {
    controller: AiDebugController,
    callbacks: VoiceCallbacks,
    sender: UnboundedSender<Event>,
    receiver: UnboundedReceiver<Event>,
    /// Events taken out of the queue early (while stopping) and not handled yet.
    backlog: VecDeque<Event>,
}

impl Session {
    async fn run(mut self) {
        if let Err(message) = self.transcribe().await {
            (self.callbacks.error)(message);
        }
        (self.callbacks.partial)(String::new());
        (self.callbacks.state)(VoiceState::Idle);
    }

    /// Whether the user stopped or cancelled while connecting.
    fn interrupted(&mut self) -> bool {
        while let Ok(event) = self.receiver.try_recv() {
            match event {
                Event::Stop | Event::Cancel => return true,
                other => self.backlog.push_back(other),
            }
        }
        false
    }

    async fn transcribe(&mut self) -> Result<(), String> {
        let token = self
            .controller
            .speech_token()
            .await
            .map_err(|error| token_error_message(&error))?;
        if self.interrupted() {
            return Ok(());
        }

        let audio = self.sender.clone();
        let capture = AudioCapture::start(
            AudioCaptureOptions {
                sample_rate: token.sample_rate,
                chunk_ms: CHUNK_MS,
            },
            move |event| {
                let _ = audio.unbounded_send(match event {
                    AudioCaptureEvent::Chunk(pcm) => Event::Audio(pcm),
                    AudioCaptureEvent::Error(message) => Event::AudioFailed(message),
                });
            },
        )
        .await
        .map_err(|error| capture_error_message(&error))?;
        let mut capture = Some(capture);
        if self.interrupted() {
            return Ok(());
        }

        // The microphone runs while connecting; the core buffers what it hears.
        let socket_events = self.sender.clone();
        let controller = self.controller.clone();
        let mut socket = Socket::connect(
            &token.url,
            move |event| {
                let _ = socket_events.unbounded_send(Event::Socket(event));
            },
            &move |future: LocalBoxFuture<'static, ()>| controller.spawn(future),
        )
        .await
        .map_err(|error| format!("音声認識サーバーへ接続できませんでした: {error}"))?;

        let ticks = self.sender.clone();
        self.controller.spawn(
            async move {
                loop {
                    sleep(TICK).await;
                    if ticks.unbounded_send(Event::Tick).is_err() {
                        break;
                    }
                }
            }
            .boxed_local(),
        );

        let mut core = VoiceCore::new(token.sample_rate, token.keyterm_library.clone());
        let mut outputs = core.connected(now_ms());
        (self.callbacks.state)(VoiceState::Recording);
        loop {
            if self.carry_out(&mut socket, outputs).await {
                break;
            }
            let event = match self.backlog.pop_front() {
                Some(event) => event,
                None => match self.receiver.next().await {
                    Some(event) => event,
                    None => break,
                },
            };
            let now = now_ms();
            outputs = match event {
                Event::Audio(pcm) => core.audio(&pcm, now),
                Event::AudioFailed(message) => core.audio_failed(&message),
                Event::Socket(SocketEvent::Message(text)) => core.server(&text),
                Event::Socket(SocketEvent::Closed(reason)) => core.closed(reason),
                Event::Tick => core.tick(now),
                Event::Cancel => vec![Output::Finish],
                Event::Stop if core.is_stopping() => Vec::new(),
                Event::Stop => {
                    (self.callbacks.state)(VoiceState::Stopping);
                    let mut rest = Vec::new();
                    if let Some(capture) = capture.take() {
                        let tail = capture.stop().await.unwrap_or_default();
                        // Chunks delivered before the stop result belong before its tail.
                        while let Ok(event) = self.receiver.try_recv() {
                            match event {
                                Event::Audio(pcm) => rest.extend_from_slice(&pcm),
                                other => self.backlog.push_back(other),
                            }
                        }
                        rest.extend_from_slice(&tail);
                    }
                    core.stop(&rest, now_ms())
                }
            };
        }
        drop(capture);
        socket.close().await;
        Ok(())
    }

    /// Carries out the core's outputs; returns true when the session is over.
    async fn carry_out(&mut self, socket: &mut Socket, outputs: Vec<Output>) -> bool {
        for output in outputs {
            match output {
                Output::Send(text) => {
                    if let Err(error) = socket.send(text).await {
                        (self.callbacks.error)(format!(
                            "音声認識サーバーへ送信できませんでした: {error}"
                        ));
                        return true;
                    }
                }
                Output::Partial(text) => (self.callbacks.partial)(text),
                Output::Committed(text) => (self.callbacks.committed)(text),
                Output::Error(message) => (self.callbacks.error)(message),
                Output::Finish => return true,
            }
        }
        false
    }
}

fn capture_error_message(error: &NativeError) -> String {
    if is_permission_denied(error) {
        "マイクの使用が許可されていません。端末の設定でマイクを許可してください".into()
    } else {
        format!("音声入力でエラーが起きました: {error}")
    }
}
