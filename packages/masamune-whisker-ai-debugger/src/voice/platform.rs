//! WebSocket client and timers: tokio-tungstenite and tokio on native targets, `web_sys::WebSocket`
//! and `setTimeout` on the web.

use std::time::Duration;

use futures_util::future::LocalBoxFuture;

use crate::redact::redact;

/// How long opening the socket may take.
pub(crate) const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// Something the socket received.
#[derive(Debug)]
pub(crate) enum SocketEvent {
    Message(String),
    /// The connection ended, with the close reason or error when there is one.
    Closed(Option<String>),
}

pub(crate) type Spawn<'a> = &'a dyn Fn(LocalBoxFuture<'static, ()>);

/// Milliseconds of wall-clock time, for the coarse voice timeouts.
pub(crate) fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

/// Error text without the single-use token of the URL.
fn safe(error: impl std::fmt::Display) -> String {
    redact(&error.to_string())
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) use native::{Socket, sleep};
#[cfg(target_arch = "wasm32")]
pub(crate) use web::{Socket, sleep};

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use futures_util::stream::SplitSink;
    use futures_util::{FutureExt, SinkExt, StreamExt};
    use tokio_tungstenite::tungstenite::Message;
    use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async};

    use super::*;

    type Stream = WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>;

    pub(crate) async fn sleep(duration: Duration) {
        tokio::time::sleep(duration).await;
    }

    pub(crate) struct Socket {
        sink: SplitSink<Stream, Message>,
    }

    impl Socket {
        /// Opens the socket; `on_event` receives the frames from a spawned task.
        pub(crate) async fn connect(
            url: &str,
            on_event: impl Fn(SocketEvent) + 'static,
            spawn: Spawn<'_>,
        ) -> Result<Self, String> {
            let (stream, _) = tokio::time::timeout(CONNECT_TIMEOUT, connect_async(url))
                .await
                .map_err(|_| "timed out".to_owned())?
                .map_err(safe)?;
            let (sink, mut reader) = stream.split();
            spawn(
                async move {
                    while let Some(frame) = reader.next().await {
                        match frame {
                            Ok(Message::Text(text)) => {
                                on_event(SocketEvent::Message(text.as_str().to_owned()))
                            }
                            Ok(Message::Close(frame)) => {
                                let reason = frame
                                    .map(|frame| frame.reason.as_str().to_owned())
                                    .filter(|reason| !reason.is_empty());
                                return on_event(SocketEvent::Closed(reason));
                            }
                            Ok(_) => {}
                            Err(error) => return on_event(SocketEvent::Closed(Some(safe(error)))),
                        }
                    }
                    on_event(SocketEvent::Closed(None));
                }
                .boxed_local(),
            );
            Ok(Self { sink })
        }

        pub(crate) async fn send(&mut self, text: String) -> Result<(), String> {
            self.sink
                .send(Message::Text(text.into()))
                .await
                .map_err(safe)
        }

        pub(crate) async fn close(&mut self) {
            let _ = self.sink.close().await;
        }
    }
}

#[cfg(target_arch = "wasm32")]
mod web {
    use std::cell::RefCell;
    use std::rc::Rc;

    use futures_channel::oneshot;
    use futures_util::future::{Either, select};
    use wasm_bindgen::JsCast;
    use wasm_bindgen::closure::Closure;

    use super::*;

    /// Resolves after `duration` through `setTimeout`; dropping it clears the timer.
    pub(crate) async fn sleep(duration: Duration) {
        let Some(window) = web_sys::window() else {
            return;
        };
        let (sender, receiver) = oneshot::channel::<()>();
        let callback = Closure::once(move || {
            let _ = sender.send(());
        });
        let millis = duration.as_millis().min(i32::MAX as u128) as i32;
        let Ok(handle) = window.set_timeout_with_callback_and_timeout_and_arguments_0(
            callback.as_ref().unchecked_ref(),
            millis,
        ) else {
            return;
        };
        struct Timer {
            window: web_sys::Window,
            handle: i32,
            _callback: Closure<dyn FnMut()>,
        }
        impl Drop for Timer {
            fn drop(&mut self) {
                self.window.clear_timeout_with_handle(self.handle);
            }
        }
        let _timer = Timer {
            window,
            handle,
            _callback: callback,
        };
        let _ = receiver.await;
    }

    type Pending = Rc<RefCell<Option<oneshot::Sender<Result<(), String>>>>>;

    pub(crate) struct Socket {
        socket: web_sys::WebSocket,
        _on_open: Closure<dyn FnMut(web_sys::Event)>,
        _on_error: Closure<dyn FnMut(web_sys::Event)>,
        _on_close: Closure<dyn FnMut(web_sys::CloseEvent)>,
        _on_message: Closure<dyn FnMut(web_sys::MessageEvent)>,
    }

    impl Socket {
        pub(crate) async fn connect(
            url: &str,
            on_event: impl Fn(SocketEvent) + 'static,
            _spawn: Spawn<'_>,
        ) -> Result<Self, String> {
            let socket =
                web_sys::WebSocket::new(url).map_err(|error| safe(format!("{error:?}")))?;
            let on_event = Rc::new(on_event);
            let (sender, receiver) = oneshot::channel();
            let pending: Pending = Rc::new(RefCell::new(Some(sender)));
            let settle = |pending: &Pending, result: Result<(), String>| {
                pending
                    .borrow_mut()
                    .take()
                    .map(|sender| sender.send(result))
                    .is_some()
            };
            let on_open = Closure::<dyn FnMut(web_sys::Event)>::new({
                let pending = pending.clone();
                move |_| {
                    settle(&pending, Ok(()));
                }
            });
            let on_error = Closure::<dyn FnMut(web_sys::Event)>::new({
                let pending = pending.clone();
                let on_event = on_event.clone();
                move |_| {
                    if !settle(&pending, Err("connection failed".into())) {
                        on_event(SocketEvent::Closed(Some("connection error".into())));
                    }
                }
            });
            let on_close = Closure::<dyn FnMut(web_sys::CloseEvent)>::new({
                let pending = pending.clone();
                let on_event = on_event.clone();
                move |event: web_sys::CloseEvent| {
                    let reason = Some(event.reason()).filter(|reason| !reason.is_empty());
                    let message = reason.clone().unwrap_or_else(|| "closed".into());
                    if !settle(&pending, Err(message)) {
                        on_event(SocketEvent::Closed(reason));
                    }
                }
            });
            let on_message = Closure::<dyn FnMut(web_sys::MessageEvent)>::new({
                let on_event = on_event.clone();
                move |event: web_sys::MessageEvent| {
                    if let Some(text) = event.data().as_string() {
                        on_event(SocketEvent::Message(text));
                    }
                }
            });
            socket.set_onopen(Some(on_open.as_ref().unchecked_ref()));
            socket.set_onerror(Some(on_error.as_ref().unchecked_ref()));
            socket.set_onclose(Some(on_close.as_ref().unchecked_ref()));
            socket.set_onmessage(Some(on_message.as_ref().unchecked_ref()));
            let this = Self {
                socket,
                _on_open: on_open,
                _on_error: on_error,
                _on_close: on_close,
                _on_message: on_message,
            };
            let timeout = std::pin::pin!(sleep(CONNECT_TIMEOUT));
            match select(receiver, timeout).await {
                Either::Left((Ok(Ok(())), _)) => Ok(this),
                Either::Left((Ok(Err(error)), _)) => Err(error),
                Either::Left((Err(_), _)) => Err("connection failed".into()),
                Either::Right(_) => Err("timed out".into()),
            }
        }

        pub(crate) async fn send(&mut self, text: String) -> Result<(), String> {
            self.socket
                .send_with_str(&text)
                .map_err(|error| safe(format!("{error:?}")))
        }

        pub(crate) async fn close(&mut self) {
            let _ = self.socket.close();
        }
    }

    impl Drop for Socket {
        fn drop(&mut self) {
            self.socket.set_onopen(None);
            self.socket.set_onerror(None);
            self.socket.set_onclose(None);
            self.socket.set_onmessage(None);
            let _ = self.socket.close();
        }
    }
}
