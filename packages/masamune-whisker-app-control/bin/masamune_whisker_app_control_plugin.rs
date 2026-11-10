fn main() -> anyhow::Result<()> {
    whisker_plugin::run_as_subprocess(masamune_whisker_app_control::cng::MasamuneAppControl)
}
