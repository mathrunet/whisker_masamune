use std::cell::RefCell;
use std::rc::Rc;

use masamune_whisker_adapter::MasamuneAdapters;
use masamune_whisker_audio_session::{
    AudioSessionConfig, AudioSessionMasamuneAdapter, PlaybackState, set_playback_state,
};
use whisker::Owner;
use whisker::WhiskerValue;
use whisker::runtime::module::{ModuleHost, with_module_host};
use whisker::runtime::{RuntimeContext, RuntimeWakeHandle};

type Calls = Rc<RefCell<Vec<(String, String)>>>;

fn host(calls: Calls) -> Rc<ModuleHost> {
    ModuleHost::new(
        move |name, method, args, _, result| {
            let json = match args.first() {
                Some(WhiskerValue::String(json)) => json.clone(),
                _ => String::new(),
            };
            calls.borrow_mut().push((format!("{name}.{method}"), json));
            result(WhiskerValue::String(r#"{"value":true}"#.into()));
            true
        },
        |_, _, _| {},
    )
}

fn run(body: impl FnOnce()) -> Vec<(String, String)> {
    let calls = Calls::default();
    let host = host(calls.clone());
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
fn adapter_configures_the_session() {
    let calls = run(|| {
        MasamuneAdapters::new()
            .adapter(AudioSessionMasamuneAdapter::new(AudioSessionConfig {
                channel_id: "audio".into(),
                small_icon: "mipmap/ic_notification".into(),
                ..AudioSessionConfig::music()
            }))
            .install()
    });
    assert_eq!(calls.len(), 1);
    assert_eq!(
        calls[0].0,
        "masamune-whisker-audio-session:AudioSession.configure"
    );
    let config: serde_json::Value = serde_json::from_str(&calls[0].1).unwrap();
    assert_eq!(config["channelId"], "audio");
    assert_eq!(config["smallIcon"], "mipmap/ic_notification");
    assert_eq!(config["mixWithOthers"], false);
}

#[test]
fn invalid_speed_falls_back_to_normal() {
    let calls = run(|| {
        set_playback_state(&PlaybackState {
            playing: true,
            speed: f64::NAN,
            ..Default::default()
        })
        .unwrap();
    });
    let state: serde_json::Value = serde_json::from_str(&calls[0].1).unwrap();
    assert_eq!(state["speed"], 1.0);
    assert_eq!(state["playing"], true);
}
