use masamune_whisker_adapter::{MasamuneAdapter, MasamuneAdapters};
use masamune_whisker_ai_debugger::{
    AiDebugController, AiDebuggerConfig, AiDebuggerMasamuneAdapter,
};
use masamune_whisker_logger::Flavor;
use whisker::Owner;
use whisker::runtime::{RuntimeContext, RuntimeWakeHandle};

#[test]
fn hands_the_controller_to_the_root() {
    let runtime = RuntimeContext::new(RuntimeWakeHandle::new(|| {}));
    runtime.enter(|| {
        let controller = AiDebugController::new(AiDebuggerConfig::new(Flavor::Prod)).unwrap();
        let root = Owner::new(None);
        for _ in 0..2 {
            root.with(|| {
                MasamuneAdapters::new()
                    .adapter(AiDebuggerMasamuneAdapter::new(controller.clone()))
                    .install();
            });
        }
        let installed = AiDebuggerMasamuneAdapter::primary().unwrap();
        assert_eq!(installed.controller.run_id(), controller.run_id());
        assert!(!installed.controller.enabled());
        root.dispose();
    });
    runtime.shutdown();
}
