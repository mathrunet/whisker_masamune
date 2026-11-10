fn main() -> anyhow::Result<()> {
    whisker_plugin::run_as_subprocess(masamune_whisker_image_picker::cng::MasamuneImagePicker)
}
