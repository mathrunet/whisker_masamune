fn main() -> anyhow::Result<()> {
    whisker_plugin::project::protocol::run_as_subprocess(masamune_whisker_auth0::cng::MasamuneAuth0)
}
