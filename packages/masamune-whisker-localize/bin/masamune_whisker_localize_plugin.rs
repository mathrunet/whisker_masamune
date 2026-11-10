fn main() -> anyhow::Result<()> {
    whisker_plugin::project::protocol::run_as_subprocess(
        masamune_whisker_localize::cng::MasamuneLocalize,
    )
}
