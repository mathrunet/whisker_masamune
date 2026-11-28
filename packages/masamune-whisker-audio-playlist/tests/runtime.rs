use std::cell::RefCell;
use std::rc::Rc;

use masamune_whisker_audio_playlist::{
    AudioPlaylist, EndReason, InterruptOptions, PlaylistEvent, QueueItem,
    RuntimeAudioPlaylistAdapter,
};

fn item(id: &str) -> QueueItem {
    QueueItem::new(id, format!("/cache/{id}.m4a"))
}

fn started(id: &str, remaining: usize) -> PlaylistEvent {
    PlaylistEvent::ItemStarted {
        id: id.into(),
        remaining,
    }
}

fn ended(id: &str, reason: EndReason) -> PlaylistEvent {
    PlaylistEvent::ItemEnded {
        id: id.into(),
        reason,
    }
}

/// A playlist whose sources all last one second.
fn setup() -> (RuntimeAudioPlaylistAdapter, AudioPlaylist) {
    let adapter = RuntimeAudioPlaylistAdapter::new();
    adapter.set_default_duration(1_000);
    let playlist = AudioPlaylist::new(adapter.clone());
    (adapter, playlist)
}

#[test]
fn plays_the_queue_without_gaps() {
    let (adapter, playlist) = setup();
    playlist.set_queue(&[item("a"), item("b")]).unwrap();
    assert_eq!(adapter.take_events(), vec![started("a", 1)]);

    // Paused: time does not move the queue.
    adapter.advance(5_000);
    assert_eq!(playlist.position_ms().unwrap(), 0);

    playlist.play().unwrap();
    adapter.advance(400);
    let status = playlist.status().unwrap();
    assert_eq!(status.current_id.as_deref(), Some("a"));
    assert_eq!(status.position_ms, 400);
    assert_eq!(status.duration_ms, Some(1_000));
    assert!(status.playing);

    // The rest of the step continues in the next item.
    adapter.advance(900);
    assert_eq!(
        adapter.take_events(),
        vec![ended("a", EndReason::Completed), started("b", 0)]
    );
    assert_eq!(playlist.current_id().unwrap().as_deref(), Some("b"));
    assert_eq!(playlist.position_ms().unwrap(), 300);

    adapter.advance(10_000);
    assert_eq!(
        adapter.take_events(),
        vec![
            ended("b", EndReason::Completed),
            PlaylistEvent::QueueDrained
        ]
    );
    let status = playlist.status().unwrap();
    assert_eq!(status.current_id, None);
    assert!(status.queue.is_empty());
    assert!(status.playing);

    // A playing playlist starts appended items right away.
    playlist.append(&[item("c")]).unwrap();
    adapter.advance(250);
    assert_eq!(adapter.take_events(), vec![started("c", 0)]);
    assert_eq!(playlist.position_ms().unwrap(), 250);
}

#[test]
fn handlers_can_top_up_the_queue() {
    let (adapter, playlist) = setup();
    let feeder = playlist.clone();
    let next = Rc::new(RefCell::new(vec!["c", "b"]));
    let _subscription = playlist.on_event(move |event| {
        if let PlaylistEvent::ItemStarted { remaining: 0, .. } = event
            && let Some(id) = next.borrow_mut().pop()
        {
            feeder.append(&[item(id)]).unwrap();
        }
    });
    playlist.set_queue(&[item("a")]).unwrap();
    assert_eq!(playlist.status().unwrap().queue, vec!["a", "b"]);
    playlist.play().unwrap();
    adapter.advance(2_500);
    assert_eq!(
        adapter.take_events(),
        vec![
            started("a", 0),
            ended("a", EndReason::Completed),
            started("b", 0),
            ended("b", EndReason::Completed),
            started("c", 0),
        ]
    );
    assert_eq!(playlist.position_ms().unwrap(), 500);
}

#[test]
fn subscriptions_end_when_dropped() {
    let (adapter, playlist) = setup();
    let seen = Rc::new(RefCell::new(Vec::new()));
    let sink = seen.clone();
    let subscription = playlist.on_event(move |event| sink.borrow_mut().push(event));
    playlist.set_queue(&[item("a")]).unwrap();
    drop(subscription);
    playlist.skip_to_next().unwrap();
    assert_eq!(*seen.borrow(), vec![started("a", 0)]);
    assert_eq!(adapter.events().len(), 3);
}

#[test]
fn skips_removes_and_replaces_items() {
    let (adapter, playlist) = setup();
    playlist
        .set_queue(&[item("a"), item("b"), item("c"), item("d")])
        .unwrap();
    playlist.play().unwrap();
    adapter.advance(300);

    assert!(playlist.remove("c").unwrap());
    assert!(!playlist.remove("missing").unwrap());
    assert!(playlist.skip_to_next().unwrap());
    assert_eq!(playlist.position_ms().unwrap(), 0);
    assert!(playlist.remove("b").unwrap());
    assert_eq!(playlist.status().unwrap().queue, vec!["d"]);
    assert!(!playlist.skip_to_next().unwrap());
    assert!(!playlist.skip_to_next().unwrap());
    assert_eq!(
        adapter.take_events(),
        vec![
            started("a", 3),
            ended("a", EndReason::Skipped),
            started("b", 1),
            ended("b", EndReason::Removed),
            started("d", 0),
            ended("d", EndReason::Skipped),
            PlaylistEvent::QueueDrained,
        ]
    );

    playlist.set_queue(&[item("x"), item("y")]).unwrap();
    playlist.set_queue(&[item("z")]).unwrap();
    playlist.set_queue(&[]).unwrap();
    assert_eq!(
        adapter.take_events(),
        vec![
            started("x", 1),
            ended("x", EndReason::Removed),
            started("z", 0),
            ended("z", EndReason::Removed),
        ]
    );
}

#[test]
fn rejects_invalid_and_duplicate_items() {
    let (adapter, playlist) = setup();
    playlist.set_queue(&[item("a")]).unwrap();
    assert!(playlist.append(&[item("a")]).is_err());
    assert!(playlist.append(&[item("b"), item("b")]).is_err());
    assert!(
        playlist
            .append(&[QueueItem::new("c", "http://example.com/c.mp3")])
            .is_err()
    );
    assert!(playlist.set_volume(f64::INFINITY).is_err());
    playlist.append(&[]).unwrap();
    assert_eq!(playlist.status().unwrap().queue, vec!["a"]);
    assert_eq!(adapter.take_events(), vec![started("a", 0)]);
}

#[test]
fn failed_items_are_skipped() {
    let (adapter, playlist) = setup();
    adapter.fail("/cache/b.m4a");
    adapter.fail("/cache/c.m4a");
    playlist
        .set_queue(&[item("a"), item("b"), item("c"), item("d")])
        .unwrap();
    playlist.play().unwrap();
    adapter.advance(1_100);
    let error = |id: &str| PlaylistEvent::Error {
        id: Some(id.into()),
        message: "failed to load the source".into(),
    };
    assert_eq!(
        adapter.take_events(),
        vec![
            started("a", 3),
            ended("a", EndReason::Completed),
            started("b", 2),
            error("b"),
            ended("b", EndReason::Failed),
            started("c", 1),
            error("c"),
            ended("c", EndReason::Failed),
            started("d", 0),
        ]
    );
    assert_eq!(playlist.position_ms().unwrap(), 100);
}

#[test]
fn pause_interrupt_resumes_at_the_same_position() {
    let (adapter, playlist) = setup();
    adapter.set_duration("/cache/announce.m4a", 400);
    playlist.set_queue(&[item("a"), item("b")]).unwrap();
    playlist.play().unwrap();
    adapter.advance(300);
    adapter.take_events();

    let id = playlist
        .interrupt("/cache/announce.m4a", InterruptOptions::default())
        .unwrap();
    assert!(playlist.status().unwrap().interrupting);
    adapter.advance(200);
    // The queue is paused while the announcement plays.
    assert_eq!(playlist.position_ms().unwrap(), 300);
    assert!(playlist.status().unwrap().playing);
    adapter.advance(300);
    assert_eq!(
        adapter.take_events(),
        vec![
            PlaylistEvent::InterruptStarted { id: id.clone() },
            PlaylistEvent::InterruptEnded {
                id,
                completed: true
            },
        ]
    );
    // Resumed at 300 ms, then 100 ms of the step after the announcement.
    assert_eq!(playlist.position_ms().unwrap(), 400);
    assert!(!playlist.status().unwrap().interrupting);
}

#[test]
fn duck_interrupt_keeps_the_queue_playing() {
    let (adapter, playlist) = setup();
    playlist.set_queue(&[item("a")]).unwrap();
    playlist.play().unwrap();
    playlist.set_volume(0.8).unwrap();
    let id = playlist
        .interrupt(
            "https://example.com/announce.m4a",
            InterruptOptions::duck(0.2),
        )
        .unwrap();
    adapter.advance(500);
    assert_eq!(playlist.position_ms().unwrap(), 500);
    assert_eq!(playlist.status().unwrap().volume, 0.8);
    adapter.finish_interrupt();
    assert_eq!(
        adapter.take_events().last(),
        Some(&PlaylistEvent::InterruptEnded {
            id,
            completed: true
        })
    );
}

#[test]
fn pausing_during_an_interrupt_keeps_the_queue_paused() {
    let (adapter, playlist) = setup();
    adapter.set_duration("/cache/announce.m4a", 100);
    playlist.set_queue(&[item("a")]).unwrap();
    playlist.play().unwrap();
    playlist
        .interrupt("/cache/announce.m4a", InterruptOptions::default())
        .unwrap();
    playlist.pause().unwrap();
    adapter.advance(500);
    assert!(!playlist.status().unwrap().interrupting);
    assert_eq!(playlist.position_ms().unwrap(), 0);
    playlist.play().unwrap();
    adapter.advance(50);
    assert_eq!(playlist.position_ms().unwrap(), 50);
}

#[test]
fn replaced_cancelled_and_failed_interrupts_report_incomplete() {
    let (adapter, playlist) = setup();
    adapter.fail("/cache/broken.m4a");
    playlist.set_queue(&[item("a")]).unwrap();
    playlist.play().unwrap();
    adapter.take_events();

    let first = playlist
        .interrupt("/cache/one.m4a", InterruptOptions::default())
        .unwrap();
    let second = playlist
        .interrupt("/cache/two.m4a", InterruptOptions::default())
        .unwrap();
    assert_ne!(first, second);
    assert!(playlist.cancel_interrupt().unwrap());
    assert!(!playlist.cancel_interrupt().unwrap());
    let third = playlist
        .interrupt("/cache/broken.m4a", InterruptOptions::default())
        .unwrap();
    assert!(
        playlist
            .interrupt("http://example.com/x.mp3", InterruptOptions::default())
            .is_err()
    );
    assert_eq!(
        adapter.take_events(),
        vec![
            PlaylistEvent::InterruptStarted { id: first.clone() },
            PlaylistEvent::InterruptEnded {
                id: first,
                completed: false
            },
            PlaylistEvent::InterruptStarted { id: second.clone() },
            PlaylistEvent::InterruptEnded {
                id: second,
                completed: false
            },
            PlaylistEvent::InterruptStarted { id: third.clone() },
            PlaylistEvent::Error {
                id: Some(third.clone()),
                message: "failed to load the source".into()
            },
            PlaylistEvent::InterruptEnded {
                id: third,
                completed: false
            },
        ]
    );
    adapter.advance(100);
    assert_eq!(playlist.position_ms().unwrap(), 100);
}

#[test]
fn stop_clears_the_queue_and_the_announcement() {
    let (adapter, playlist) = setup();
    playlist.set_queue(&[item("a"), item("b")]).unwrap();
    playlist.play().unwrap();
    let id = playlist
        .interrupt("/cache/announce.m4a", InterruptOptions::duck(0.3))
        .unwrap();
    adapter.take_events();
    playlist.stop().unwrap();
    assert_eq!(
        adapter.take_events(),
        vec![
            PlaylistEvent::InterruptEnded {
                id,
                completed: false
            },
            ended("a", EndReason::Stopped),
        ]
    );
    let status = playlist.status().unwrap();
    assert!(!status.playing && !status.interrupting);
    assert!(status.queue.is_empty());
    adapter.finish_current();
    assert!(adapter.take_events().is_empty());
}

#[test]
fn output_deadlines_end_items_before_their_frames_reach_it() {
    let (adapter, playlist) = setup();
    adapter.set_output_reporting(true);
    let deadline = adapter.now_ns() + 400_000_000;
    playlist
        .set_queue(&[
            item("a").with_output_before_ns(deadline),
            item("b").with_output_before_ns(deadline),
            item("c"),
        ])
        .unwrap();
    playlist.play().unwrap();
    adapter.take_events();
    adapter.advance(1_000);
    let events = adapter.take_events();
    assert!(events.contains(&ended("a", EndReason::Expired)));
    let last = events.iter().find_map(|event| match event {
        PlaylistEvent::OutputEnded { id, last_frame_ns } if id == "a" => Some(*last_frame_ns),
        _ => None,
    });
    assert_eq!(last, Some(deadline));
    // "b" shares the deadline and never starts; "c" has none and plays.
    assert!(events.contains(&ended("b", EndReason::Expired)));
    assert!(!events.iter().any(|event| matches!(
        event,
        PlaylistEvent::OutputStarted { id, .. } if id == "b"
    )));
    assert_eq!(playlist.current_id().unwrap().as_deref(), Some("c"));
}

#[test]
fn reports_output_and_tightens_deadlines() {
    let (adapter, playlist) = setup();
    playlist.set_queue(&[item("a")]).unwrap();
    adapter.set_output_reporting(true);
    adapter.take_events();
    playlist.play().unwrap();
    let events = adapter.take_events();
    assert!(matches!(
        &events[..],
        [PlaylistEvent::OutputStarted { id, earliest_ns, first_frame_ns, latest_ns }]
            if id == "a" && earliest_ns <= first_frame_ns && first_frame_ns <= latest_ns
    ));
    // A later deadline does not extend an earlier one.
    let now = adapter.now_ns();
    assert!(
        playlist
            .set_output_deadline("a", now + 100_000_000)
            .unwrap()
    );
    assert!(
        playlist
            .set_output_deadline("a", now + 900_000_000)
            .unwrap()
    );
    assert!(!playlist.set_output_deadline("x", now).unwrap());
    adapter.advance(200);
    let events = adapter.take_events();
    assert!(matches!(&events[0], PlaylistEvent::OutputEnded { id, .. } if id == "a"));
    assert_eq!(events[1], ended("a", EndReason::Expired));
    assert!(matches!(&events[2], PlaylistEvent::QueueDrained));

    // Announcements end unfinished at their deadline.
    let id = playlist
        .interrupt(
            "/cache/n.m4a",
            InterruptOptions::default().with_output_before_ns(adapter.now_ns() + 50_000_000),
        )
        .unwrap();
    adapter.advance(100);
    let events = adapter.take_events();
    assert!(events.contains(&PlaylistEvent::InterruptEnded {
        id: id.clone(),
        completed: false
    }));
    assert!(events.iter().any(
        |event| matches!(event, PlaylistEvent::OutputStarted { id: started, .. } if *started == id)
    ));
}
