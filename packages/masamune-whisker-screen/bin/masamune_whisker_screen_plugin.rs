fn main() -> anyhow::Result<()> {
    whisker_plugin::run_as_subprocess(masamune_whisker_screen::cng::MasamuneScreen)
}
