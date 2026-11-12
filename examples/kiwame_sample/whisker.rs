//! App configuration (CNG). Generates the iOS and Android projects under `gen/`.

fn main() {
    // Same variable as `src/config.rs`; fail fast on a typo instead of building another flavor.
    match std::env::var("KIWAME_SAMPLE_FLAVOR").as_deref() {
        Err(_) | Ok("dev" | "test" | "prod") => {}
        Ok(_) => panic!("KIWAME_SAMPLE_FLAVOR must be dev, test or prod"),
    }
    whisker_cng::run(|app| {
        app.name("Kiwame Sample")
            .bundle_id("net.mathru.kiwamesample")
            .version("0.1.0")
            .build_number(1);
        app.android(|android| {
            android
                .package("net.mathru.kiwamesample")
                .application_id("net.mathru.kiwamesample")
                .launcher_activity(".MainActivity")
                .min_sdk(24)
                .target_sdk(36);
        });
        app.ios(|ios| {
            ios.bundle_id("net.mathru.kiwamesample")
                .scheme("KiwameSample")
                .deployment_target("15.0");
        });
    });
}
