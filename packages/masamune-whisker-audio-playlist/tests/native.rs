use std::cell::RefCell;
use std::rc::Rc;

use masamune_whisker_audio_playlist::{AudioPlaylist, InterruptOptions, QueueItem};
use serde_json::{Value, json};
use whisker::Owner;
use whisker::WhiskerValue;
use whisker::runtime::module::{ModuleHost, with_module_host};
use whisker::runtime::{RuntimeContext, RuntimeWakeHandle};

type Calls = Rc<RefCell<Vec<(String, Value)>>>;

/// Runs `body` against a fake host that records calls and answers each method like the native
/// module.
fn run(body: impl FnOnce()) -> Vec<(String, Value)> {
    let calls = Calls::default();
    let recorded = calls.clone();
    let host = ModuleHost::new(
        move |name, method, args, _, result| {
            let json = match args.first() {
                Some(WhiskerValue::String(json)) => serde_json::from_str(json).unwrap(),
                _ => Value::Null,
            };
            let method = method.to_string();
            recorded
                .borrow_mut()
                .push((format!("{name}.{method}"), json));
            let value = match method.as_str() {
                "status" => json!({
                    "currentId": "a", "positionMs": 1200, "durationMs": 180000,
                    "playing": true, "interrupting": false, "queue": ["a", "b"], "volume": 1.0,
                }),
                _ => json!(true),
            };
            result(WhiskerValue::String(json!({ "value": value }).to_string()));
            true
        },
        |_, _, _| {},
    );
    let runtime = RuntimeContext::new(RuntimeWakeHandle::new(|| {}));
    runtime.enter(|| {
        with_module_host(&host, || {
            let root = Owner::new(None);
            root.with(body);
            whisker::run_until_stalled();
            root.dispose();
        })
    });
    runtime.shutdown();
    calls.take()
}

#[test]
fn native_calls_use_the_wire_format() {
    let mut interrupt_id = String::new();
    let calls = run(|| {
        let playlist = AudioPlaylist::native();
        playlist
            .set_queue(&[
                QueueItem::new("a", "/cache/a.m4a"),
                QueueItem::new("b", "https://example.com/b.m4a"),
            ])
            .unwrap();
        playlist
            .append(&[QueueItem::new("c", "file:///cache/c.m4a")])
            .unwrap();
        assert!(playlist.remove("c").unwrap());
        playlist.play().unwrap();
        playlist.pause().unwrap();
        assert!(playlist.skip_to_next().unwrap());
        playlist.set_volume(2.0).unwrap();
        assert_eq!(playlist.current_id().unwrap().as_deref(), Some("a"));
        assert_eq!(playlist.position_ms().unwrap(), 1200);
        interrupt_id = playlist
            .interrupt("/cache/announce.m4a", InterruptOptions::duck(0.25))
            .unwrap();
        assert!(playlist.cancel_interrupt().unwrap());
        playlist.stop().unwrap();
        // Rejected before reaching the host.
        assert!(playlist.append(&[QueueItem::new("x", "x.m4a")]).is_err());
    });
    let prefix = "masamune-whisker-audio-playlist:AudioPlaylist.";
    let calls: Vec<(&str, Value)> = calls
        .iter()
        .map(|(method, json)| (method.strip_prefix(prefix).unwrap(), json.clone()))
        .collect();
    assert_eq!(
        calls,
        vec![
            (
                "setQueue",
                json!({ "items": [
                    { "id": "a", "uri": "/cache/a.m4a" },
                    { "id": "b", "uri": "https://example.com/b.m4a" },
                ] })
            ),
            (
                "append",
                json!({ "items": [{ "id": "c", "uri": "file:///cache/c.m4a" }] })
            ),
            ("remove", json!({ "id": "c" })),
            ("play", json!({})),
            ("pause", json!({})),
            ("skipToNext", json!({})),
            ("setVolume", json!({ "volume": 1.0 })),
            ("status", json!({})),
            ("status", json!({})),
            (
                "interrupt",
                json!({
                    "id": interrupt_id, "uri": "/cache/announce.m4a",
                    "mode": "duck", "duckVolume": 0.25, "volume": 1.0,
                })
            ),
            ("cancelInterrupt", json!({})),
            ("stop", json!({})),
        ]
    );
}

#[test]
fn native_sources_emit_every_event_type() {
    let sources = [
        include_str!("../ios/Sources/MasamuneWhiskerAudioPlaylist/AudioPlaylistModule.swift"),
        include_str!(
            "../android/src/main/kotlin/net/mathru/whisker/audio_playlist/AudioPlaylistModule.kt"
        ),
    ];
    for source in sources {
        for name in [
            "\"itemStarted\"",
            "\"itemEnded\"",
            "\"queueDrained\"",
            "\"error\"",
            "\"interruptStarted\"",
            "\"interruptEnded\"",
            "\"completed\"",
            "\"skipped\"",
            "\"removed\"",
            "\"failed\"",
            "\"stopped\"",
        ] {
            assert!(source.contains(name), "{name} is missing");
        }
    }
}
