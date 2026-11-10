fn main() -> anyhow::Result<()> {
    whisker_plugin::run_as_subprocess(masamune_whisker_admob::cng::MasamuneAdmob)
}
