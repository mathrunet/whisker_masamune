use std::cell::{Cell, RefCell};
use std::rc::Rc;

use crate::{MessagingAdapter, MessagingFuture, MessagingSubscription, PushMessage};

type TokenHandler = Rc<dyn Fn(String)>;
type MessageHandler = Rc<dyn Fn(PushMessage)>;

#[derive(Default)]
struct State {
    permission: bool,
    token: Option<String>,
    initial_message: Option<PushMessage>,
    deleted_count: u32,
    next_id: u64,
    token_listeners: Vec<(u64, TokenHandler)>,
    message_listeners: Vec<(u64, MessageHandler)>,
    opened_listeners: Vec<(u64, MessageHandler)>,
}

/// In-memory [`MessagingAdapter`] for tests and UI lanes (no native SDK or network calls).
///
/// Clones share the same state.
#[derive(Clone, Default)]
pub struct RuntimeMessagingAdapter {
    state: Rc<RefCell<State>>,
    permission_requests: Rc<Cell<u32>>,
}

impl RuntimeMessagingAdapter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the result returned by [`MessagingAdapter::request_permission`].
    pub fn with_permission(self, granted: bool) -> Self {
        self.state.borrow_mut().permission = granted;
        self
    }

    /// Sets the initial registration token returned by [`MessagingAdapter::token`].
    pub fn with_token(self, token: impl Into<String>) -> Self {
        self.state.borrow_mut().token = Some(token.into());
        self
    }

    /// Sets the launch message returned once by [`MessagingAdapter::initial_message`].
    pub fn with_initial_message(self, message: PushMessage) -> Self {
        self.state.borrow_mut().initial_message = Some(message);
        self
    }

    /// Number of times `request_permission` has been called.
    pub fn permission_requests(&self) -> u32 {
        self.permission_requests.get()
    }

    /// Number of times `delete_token` has been called.
    pub fn deleted_count(&self) -> u32 {
        self.state.borrow().deleted_count
    }

    /// Updates the stored token and notifies all token-refresh listeners.
    pub fn emit_token_refresh(&self, token: impl Into<String>) {
        let token = token.into();
        let listeners: Vec<TokenHandler> = {
            let mut state = self.state.borrow_mut();
            state.token = Some(token.clone());
            state
                .token_listeners
                .iter()
                .map(|(_, cb)| cb.clone())
                .collect()
        };
        for listener in listeners {
            listener(token.clone());
        }
    }

    /// Delivers a foreground message to all `on_message` listeners.
    pub fn emit_message(&self, message: PushMessage) {
        let listeners: Vec<MessageHandler> = self
            .state
            .borrow()
            .message_listeners
            .iter()
            .map(|(_, cb)| cb.clone())
            .collect();
        for listener in listeners {
            listener(message.clone());
        }
    }

    /// Delivers a tapped notification to all `on_message_opened` listeners.
    pub fn emit_message_opened(&self, message: PushMessage) {
        let listeners: Vec<MessageHandler> = self
            .state
            .borrow()
            .opened_listeners
            .iter()
            .map(|(_, cb)| cb.clone())
            .collect();
        for listener in listeners {
            listener(message.clone());
        }
    }
}

struct RemoveGuard {
    state: Rc<RefCell<State>>,
    kind: ListenerKind,
    id: u64,
}

enum ListenerKind {
    Token,
    Message,
    Opened,
}

impl Drop for RemoveGuard {
    fn drop(&mut self) {
        let mut state = self.state.borrow_mut();
        match self.kind {
            ListenerKind::Token => state.token_listeners.retain(|(id, _)| *id != self.id),
            ListenerKind::Message => state.message_listeners.retain(|(id, _)| *id != self.id),
            ListenerKind::Opened => state.opened_listeners.retain(|(id, _)| *id != self.id),
        }
    }
}

impl MessagingAdapter for RuntimeMessagingAdapter {
    fn request_permission(&self) -> MessagingFuture<'_, bool> {
        self.permission_requests
            .set(self.permission_requests.get() + 1);
        let granted = self.state.borrow().permission;
        Box::pin(async move { Ok(granted) })
    }

    fn token(&self) -> MessagingFuture<'_, Option<String>> {
        let token = self.state.borrow().token.clone();
        Box::pin(async move { Ok(token) })
    }

    fn delete_token(&self) -> MessagingFuture<'_, ()> {
        {
            let mut state = self.state.borrow_mut();
            state.token = None;
            state.deleted_count += 1;
        }
        Box::pin(async { Ok(()) })
    }

    fn initial_message(&self) -> MessagingFuture<'_, Option<PushMessage>> {
        let message = self.state.borrow_mut().initial_message.take();
        Box::pin(async move { Ok(message) })
    }

    fn on_token_refresh(&self, handler: TokenHandler) -> MessagingSubscription {
        let mut state = self.state.borrow_mut();
        state.next_id += 1;
        let id = state.next_id;
        state.token_listeners.push((id, handler));
        MessagingSubscription::new(RemoveGuard {
            state: self.state.clone(),
            kind: ListenerKind::Token,
            id,
        })
    }

    fn on_message(&self, handler: MessageHandler) -> MessagingSubscription {
        let mut state = self.state.borrow_mut();
        state.next_id += 1;
        let id = state.next_id;
        state.message_listeners.push((id, handler));
        MessagingSubscription::new(RemoveGuard {
            state: self.state.clone(),
            kind: ListenerKind::Message,
            id,
        })
    }

    fn on_message_opened(&self, handler: MessageHandler) -> MessagingSubscription {
        let mut state = self.state.borrow_mut();
        state.next_id += 1;
        let id = state.next_id;
        state.opened_listeners.push((id, handler));
        MessagingSubscription::new(RemoveGuard {
            state: self.state.clone(),
            kind: ListenerKind::Opened,
            id,
        })
    }
}
