use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet, VecDeque};
use std::rc::Rc;

use crate::{
    AudioPlaylistAdapter, EndReason, InterruptMode, InterruptRequest, NativeError, NativeResult,
    PlaylistEvent, PlaylistStatus, QueueItem, Subscription,
};

/// Default length of every source in [`RuntimeAudioPlaylistAdapter`]: three minutes.
const DEFAULT_DURATION_MS: u64 = 180_000;

type Handler = Rc<dyn Fn(PlaylistEvent)>;

struct Announcement {
    id: String,
    uri: String,
    mode: InterruptMode,
    position_ms: u64,
    output_before_ns: Option<u64>,
}

struct State {
    queue: VecDeque<QueueItem>,
    position_ms: u64,
    playing: bool,
    volume: f64,
    announcement: Option<Announcement>,
    durations: HashMap<String, u64>,
    default_duration_ms: u64,
    failing: HashSet<String>,
    log: Vec<PlaylistEvent>,
    /// Time added by [`RuntimeAudioPlaylistAdapter::advance`] on top of the output clock.
    clock_offset_ns: u64,
    report_output: bool,
    /// IDs whose audio reaches the output, for [`PlaylistEvent::OutputStarted`] / `OutputEnded`.
    audible: Vec<String>,
}

impl State {
    fn now_ns(&self) -> u64 {
        crate::output_clock_ns()
            .unwrap_or(u64::MAX)
            .saturating_add(self.clock_offset_ns)
    }

    /// Reports that audible `id` stopped at its deadline: playback is virtual, so the last
    /// frame is presented right before the deadline.
    fn stop_output_at(&mut self, id: &str, deadline: u64, events: &mut Vec<PlaylistEvent>) {
        if let Some(index) = self.audible.iter().position(|audible| audible == id) {
            self.audible.remove(index);
            events.push(PlaylistEvent::OutputEnded {
                id: id.to_owned(),
                last_frame_ns: deadline,
            });
        }
    }

    /// Ends the announcement and the queue items whose output deadline has come.
    fn enforce_deadlines(&mut self, events: &mut Vec<PlaylistEvent>) {
        let now = self.now_ns();
        if let Some((id, deadline)) = self.announcement.as_ref().and_then(|announcement| {
            announcement
                .output_before_ns
                .filter(|deadline| now >= *deadline)
                .map(|deadline| (announcement.id.clone(), deadline))
        }) {
            self.stop_output_at(&id, deadline, events);
            self.end_announcement(false, events);
        }
        loop {
            let mut index = 1;
            while index < self.queue.len() {
                if self.queue[index]
                    .output_before_ns
                    .is_some_and(|deadline| now >= deadline)
                {
                    let item = self.queue.remove(index).expect("index is in range");
                    events.push(PlaylistEvent::ItemEnded {
                        id: item.id,
                        reason: EndReason::Expired,
                    });
                } else {
                    index += 1;
                }
            }
            match self.queue.front() {
                Some(item)
                    if item
                        .output_before_ns
                        .is_some_and(|deadline| now >= deadline) =>
                {
                    let (id, deadline) = (item.id.clone(), item.output_before_ns.unwrap_or(now));
                    self.stop_output_at(&id, deadline, events);
                    self.end_front(EndReason::Expired, events);
                }
                _ => return,
            }
        }
    }

    /// Reports the IDs whose audio started or stopped reaching the output.
    fn reconcile_output(&mut self, events: &mut Vec<PlaylistEvent>) {
        if !self.report_output {
            return;
        }
        let mut audible = Vec::new();
        if self.queue_audible()
            && let Some(item) = self.queue.front()
        {
            audible.push(item.id.clone());
        }
        if let Some(announcement) = &self.announcement {
            audible.push(announcement.id.clone());
        }
        let now = self.now_ns();
        for id in &self.audible {
            if !audible.contains(id) {
                events.push(PlaylistEvent::OutputEnded {
                    id: id.clone(),
                    last_frame_ns: now,
                });
            }
        }
        for id in &audible {
            if !self.audible.contains(id) {
                events.push(PlaylistEvent::OutputStarted {
                    id: id.clone(),
                    first_frame_ns: now,
                    earliest_ns: now,
                    latest_ns: now,
                });
            }
        }
        self.audible = audible;
    }

    /// Milliseconds until the next output deadline of what is audible.
    fn to_deadline_ms(&self) -> u64 {
        let now = self.now_ns();
        let front = self
            .queue
            .front()
            .filter(|_| self.queue_audible())
            .and_then(|item| item.output_before_ns);
        let announcement = self
            .announcement
            .as_ref()
            .and_then(|announcement| announcement.output_before_ns);
        [front, announcement]
            .into_iter()
            .flatten()
            .map(|deadline| deadline.saturating_sub(now).div_ceil(1_000_000))
            .min()
            .unwrap_or(u64::MAX)
    }

    fn duration(&self, uri: &str) -> u64 {
        self.durations
            .get(uri)
            .copied()
            .unwrap_or(self.default_duration_ms)
    }

    /// Whether the current item advances with time.
    fn queue_audible(&self) -> bool {
        self.playing
            && !self.queue.is_empty()
            && !matches!(
                self.announcement,
                Some(Announcement {
                    mode: InterruptMode::Pause,
                    ..
                })
            )
    }

    /// Makes the front item current. Sources marked with
    /// [`RuntimeAudioPlaylistAdapter::fail`] fail right after they start, like a native source
    /// that cannot be loaded.
    fn start_front(&mut self, events: &mut Vec<PlaylistEvent>) {
        self.position_ms = 0;
        loop {
            let Some(front) = self.queue.front() else {
                events.push(PlaylistEvent::QueueDrained);
                return;
            };
            events.push(PlaylistEvent::ItemStarted {
                id: front.id.clone(),
                remaining: self.queue.len() - 1,
            });
            if !self.failing.contains(&front.uri) {
                return;
            }
            let id = front.id.clone();
            events.push(PlaylistEvent::Error {
                id: Some(id.clone()),
                message: "failed to load the source".into(),
            });
            events.push(PlaylistEvent::ItemEnded {
                id,
                reason: EndReason::Failed,
            });
            self.queue.pop_front();
        }
    }

    /// Ends the current item with `reason` and moves to the next one.
    fn end_front(&mut self, reason: EndReason, events: &mut Vec<PlaylistEvent>) {
        if let Some(item) = self.queue.pop_front() {
            events.push(PlaylistEvent::ItemEnded {
                id: item.id,
                reason,
            });
            self.start_front(events);
        }
    }

    fn end_announcement(&mut self, completed: bool, events: &mut Vec<PlaylistEvent>) -> bool {
        match self.announcement.take() {
            Some(announcement) => {
                events.push(PlaylistEvent::InterruptEnded {
                    id: announcement.id,
                    completed,
                });
                true
            }
            None => false,
        }
    }

    fn check_duplicates<'a>(&self, ids: impl IntoIterator<Item = &'a str>) -> NativeResult<()> {
        for id in ids {
            if self.queue.iter().any(|item| item.id == id) {
                return Err(NativeError(format!("duplicate queue item id: {id}")));
            }
        }
        Ok(())
    }
}

/// In-memory playlist for tests and previews. Nothing plays and no time passes by itself:
/// [`RuntimeAudioPlaylistAdapter::advance`] moves the clock, and the queue behaves like the
/// native player (gapless advance, events, interrupts that restore the queue at the same
/// position).
///
/// Every source lasts three minutes unless set with
/// [`RuntimeAudioPlaylistAdapter::set_duration`]; sources marked with
/// [`RuntimeAudioPlaylistAdapter::fail`] fail when they start. Clones share the same state.
/// Events are delivered synchronously, and handlers may call the adapter again (for example to
/// append items on [`PlaylistEvent::QueueDrained`]).
///
/// ```
/// use masamune_whisker_audio_playlist::{
///     AudioPlaylist, PlaylistEvent, QueueItem, RuntimeAudioPlaylistAdapter,
/// };
///
/// let adapter = RuntimeAudioPlaylistAdapter::new();
/// adapter.set_duration("/a.m4a", 1_000);
/// let playlist = AudioPlaylist::new(adapter.clone());
/// playlist.set_queue(&[QueueItem::new("a", "/a.m4a"), QueueItem::new("b", "/b.m4a")])?;
/// playlist.play()?;
/// adapter.advance(1_500);
/// assert_eq!(playlist.current_id()?.as_deref(), Some("b"));
/// assert_eq!(playlist.position_ms()?, 500);
/// # Ok::<(), masamune_whisker_audio_playlist::NativeError>(())
/// ```
#[derive(Clone)]
pub struct RuntimeAudioPlaylistAdapter {
    state: Rc<RefCell<State>>,
    handlers: Rc<RefCell<Vec<(u64, Handler)>>>,
    next_handler: Rc<Cell<u64>>,
}

impl Default for RuntimeAudioPlaylistAdapter {
    fn default() -> Self {
        Self {
            state: Rc::new(RefCell::new(State {
                queue: VecDeque::new(),
                position_ms: 0,
                playing: false,
                volume: 1.0,
                announcement: None,
                durations: HashMap::new(),
                default_duration_ms: DEFAULT_DURATION_MS,
                failing: HashSet::new(),
                log: Vec::new(),
                clock_offset_ns: 0,
                report_output: false,
                audible: Vec::new(),
            })),
            handlers: Rc::default(),
            next_handler: Rc::default(),
        }
    }
}

impl std::fmt::Debug for RuntimeAudioPlaylistAdapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let state = self.state.borrow();
        f.debug_struct("RuntimeAudioPlaylistAdapter")
            .field("queue", &state.queue)
            .field("position_ms", &state.position_ms)
            .field("playing", &state.playing)
            .finish_non_exhaustive()
    }
}

impl RuntimeAudioPlaylistAdapter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Length of the source at `uri` (queue items and announcements).
    pub fn set_duration(&self, uri: &str, duration_ms: u64) {
        self.state
            .borrow_mut()
            .durations
            .insert(uri.to_owned(), duration_ms);
    }

    /// Length of sources without [`RuntimeAudioPlaylistAdapter::set_duration`].
    pub fn set_default_duration(&self, duration_ms: u64) {
        self.state.borrow_mut().default_duration_ms = duration_ms;
    }

    /// Makes the source at `uri` fail when it starts (queue items) or when it is interrupted
    /// with (announcements).
    pub fn fail(&self, uri: &str) {
        self.state.borrow_mut().failing.insert(uri.to_owned());
    }

    /// Reports [`PlaylistEvent::OutputStarted`] and [`PlaylistEvent::OutputEnded`] (off by
    /// default). The runtime player has no output latency, so the first and last frames are
    /// presented at the moment the item becomes audible or silent.
    pub fn set_output_reporting(&self, report: bool) {
        self.update(|state, _| state.report_output = report);
    }

    /// Current time on the adapter's output clock: [`crate::output_clock_ns`] plus the time
    /// moved by [`RuntimeAudioPlaylistAdapter::advance`]. Output deadlines are compared with it.
    pub fn now_ns(&self) -> u64 {
        self.state.borrow().now_ns()
    }

    /// Moves the clock by `ms` milliseconds: the current item (while audible) and the
    /// announcement advance, finished items hand over to the next one without a gap, output
    /// deadlines end what reaches them, and the events are delivered in order.
    pub fn advance(&self, ms: u64) {
        let mut left = ms;
        loop {
            let mut events = Vec::new();
            {
                let mut state = self.state.borrow_mut();
                state.enforce_deadlines(&mut events);
                state.reconcile_output(&mut events);
                let to_deadline = state.to_deadline_ms();
                let audible = state.queue_audible();
                let to_item_end = match state.queue.front() {
                    Some(item) if audible => {
                        state.duration(&item.uri).saturating_sub(state.position_ms)
                    }
                    _ => u64::MAX,
                };
                let to_announcement_end = match &state.announcement {
                    Some(announcement) => state
                        .duration(&announcement.uri)
                        .saturating_sub(announcement.position_ms),
                    None => u64::MAX,
                };
                let step = left
                    .min(to_item_end)
                    .min(to_announcement_end)
                    .min(to_deadline);
                left -= step;
                state.clock_offset_ns = state
                    .clock_offset_ns
                    .saturating_add(step.saturating_mul(1_000_000));
                if audible {
                    state.position_ms += step;
                }
                if let Some(announcement) = &mut state.announcement {
                    announcement.position_ms += step;
                }
                if step == to_item_end {
                    state.end_front(EndReason::Completed, &mut events);
                }
                if step == to_announcement_end {
                    state.end_announcement(true, &mut events);
                }
                state.enforce_deadlines(&mut events);
                state.reconcile_output(&mut events);
            }
            let ended = !events.is_empty();
            self.emit(events);
            if left == 0 && !ended {
                return;
            }
        }
    }

    /// Plays the current item to its end right away (also while paused).
    pub fn finish_current(&self) {
        self.update(|state, events| state.end_front(EndReason::Completed, events));
    }

    /// Plays the announcement to its end right away.
    pub fn finish_interrupt(&self) {
        self.update(|state, events| {
            state.end_announcement(true, events);
        });
    }

    /// Every event delivered so far, oldest first.
    pub fn events(&self) -> Vec<PlaylistEvent> {
        self.state.borrow().log.clone()
    }

    /// Returns and forgets the events delivered so far.
    pub fn take_events(&self) -> Vec<PlaylistEvent> {
        std::mem::take(&mut self.state.borrow_mut().log)
    }

    /// Runs `change` on the state and delivers the events it produced.
    fn update<T>(&self, change: impl FnOnce(&mut State, &mut Vec<PlaylistEvent>) -> T) -> T {
        let mut events = Vec::new();
        let result = {
            let mut state = self.state.borrow_mut();
            let result = change(&mut state, &mut events);
            state.enforce_deadlines(&mut events);
            state.reconcile_output(&mut events);
            result
        };
        self.emit(events);
        result
    }

    fn emit(&self, events: Vec<PlaylistEvent>) {
        for event in events {
            self.state.borrow_mut().log.push(event.clone());
            let handlers: Vec<Handler> = self
                .handlers
                .borrow()
                .iter()
                .map(|(_, handler)| handler.clone())
                .collect();
            for handler in handlers {
                handler(event.clone());
            }
        }
    }
}

/// Removes a handler of [`RuntimeAudioPlaylistAdapter`] when dropped.
struct HandlerGuard {
    id: u64,
    handlers: Rc<RefCell<Vec<(u64, Handler)>>>,
}

impl Drop for HandlerGuard {
    fn drop(&mut self) {
        self.handlers.borrow_mut().retain(|(id, _)| *id != self.id);
    }
}

impl AudioPlaylistAdapter for RuntimeAudioPlaylistAdapter {
    fn set_queue(&self, items: &[QueueItem]) -> NativeResult<()> {
        crate::validate_items(items)?;
        self.update(|state, events| {
            if let Some(item) = state.queue.pop_front() {
                events.push(PlaylistEvent::ItemEnded {
                    id: item.id,
                    reason: EndReason::Removed,
                });
            }
            state.queue = items.iter().cloned().collect();
            state.position_ms = 0;
            if !state.queue.is_empty() {
                state.start_front(events);
            }
        });
        Ok(())
    }

    fn append(&self, items: &[QueueItem]) -> NativeResult<()> {
        crate::validate_items(items)?;
        self.update(|state, events| {
            state.check_duplicates(items.iter().map(|item| item.id.as_str()))?;
            let was_empty = state.queue.is_empty();
            state.queue.extend(items.iter().cloned());
            if was_empty && !state.queue.is_empty() {
                state.start_front(events);
            }
            Ok(())
        })
    }

    fn remove(&self, id: &str) -> NativeResult<bool> {
        Ok(self.update(|state, events| {
            match state.queue.iter().position(|item| item.id == id) {
                Some(0) => state.end_front(EndReason::Removed, events),
                Some(index) => {
                    state.queue.remove(index);
                }
                None => return false,
            }
            true
        }))
    }

    fn play(&self) -> NativeResult<()> {
        self.update(|state, _| state.playing = true);
        Ok(())
    }

    fn pause(&self) -> NativeResult<()> {
        self.update(|state, _| state.playing = false);
        Ok(())
    }

    fn stop(&self) -> NativeResult<()> {
        self.update(|state, events| {
            state.end_announcement(false, events);
            if let Some(item) = state.queue.pop_front() {
                events.push(PlaylistEvent::ItemEnded {
                    id: item.id,
                    reason: EndReason::Stopped,
                });
            }
            state.queue.clear();
            state.position_ms = 0;
            state.playing = false;
        });
        Ok(())
    }

    fn skip_to_next(&self) -> NativeResult<bool> {
        Ok(self.update(|state, events| {
            if state.queue.is_empty() {
                return false;
            }
            state.end_front(EndReason::Skipped, events);
            !state.queue.is_empty()
        }))
    }

    fn set_volume(&self, volume: f64) -> NativeResult<()> {
        self.state.borrow_mut().volume = volume;
        Ok(())
    }

    fn status(&self) -> NativeResult<PlaylistStatus> {
        let state = self.state.borrow();
        let current = state.queue.front();
        Ok(PlaylistStatus {
            current_id: current.map(|item| item.id.clone()),
            position_ms: if current.is_some() {
                state.position_ms
            } else {
                0
            },
            duration_ms: current.map(|item| state.duration(&item.uri)),
            playing: state.playing,
            interrupting: state.announcement.is_some(),
            queue: state.queue.iter().map(|item| item.id.clone()).collect(),
            volume: state.volume,
        })
    }

    fn interrupt(&self, request: &InterruptRequest) -> NativeResult<()> {
        self.update(|state, events| {
            state.end_announcement(false, events);
            state.announcement = Some(Announcement {
                id: request.id.clone(),
                uri: request.uri.clone(),
                mode: request.mode,
                position_ms: 0,
                output_before_ns: request.output_before_ns,
            });
            events.push(PlaylistEvent::InterruptStarted {
                id: request.id.clone(),
            });
            if state.failing.contains(&request.uri) {
                events.push(PlaylistEvent::Error {
                    id: Some(request.id.clone()),
                    message: "failed to load the source".into(),
                });
                state.end_announcement(false, events);
            }
        });
        Ok(())
    }

    fn cancel_interrupt(&self) -> NativeResult<bool> {
        Ok(self.update(|state, events| state.end_announcement(false, events)))
    }

    fn set_output_deadline(&self, id: &str, output_before_ns: u64) -> NativeResult<bool> {
        Ok(self.update(|state, _| {
            let tighten = |current: &mut Option<u64>| {
                *current = Some(current.map_or(output_before_ns, |c| c.min(output_before_ns)));
            };
            if let Some(announcement) = state.announcement.as_mut().filter(|a| a.id == id) {
                tighten(&mut announcement.output_before_ns);
                return true;
            }
            match state.queue.iter_mut().find(|item| item.id == id) {
                Some(item) => {
                    tighten(&mut item.output_before_ns);
                    true
                }
                None => false,
            }
        }))
    }

    fn subscribe(&self, handler: Rc<dyn Fn(PlaylistEvent)>) -> Subscription {
        let id = self.next_handler.get();
        self.next_handler.set(id + 1);
        self.handlers.borrow_mut().push((id, handler));
        Subscription::new(HandlerGuard {
            id,
            handlers: self.handlers.clone(),
        })
    }
}
