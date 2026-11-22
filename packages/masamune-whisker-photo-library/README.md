# masamune-whisker-photo-library

Read the device photo library from a Whisker app: permission (including limited access), assets
by creation date with their locations, thumbnails and scaled images (the counterpart of Flutter's
`photo_manager` as Segue uses it).

- **iOS**: Photos framework (`PHAsset`, `PHImageManager`; iCloud photos are downloaded when needed).
- **Android**: `MediaStore` images and videos; locations from the originals (EXIF / video
  metadata) with `ACCESS_MEDIA_LOCATION`.

Images are written as JPEG files to the app's cache directory
(`masamune_whisker_photo_library/`) and returned as absolute paths, ready for an image element.

## Setup

### iOS

```rust,ignore
use masamune_whisker_photo_library::cng::MasamunePhotoLibrary;

app.project_plugin::<MasamunePhotoLibrary>(|c| {
    c.photo_library_usage("Used to show the photos you took along today's trail.")
        .prevent_limited_access_alert(true);
});
```

`NSPhotoLibraryUsageDescription` is required (iOS terminates an app that asks without it).
`PHPhotoLibraryPreventAutomaticLimitedAccessAlert` stops iOS from offering to change a limited
selection on every launch; call `present_limited_picker` instead.

### Android

The crate's manifest declares `READ_MEDIA_IMAGES`, `READ_MEDIA_VIDEO`,
`READ_MEDIA_VISUAL_USER_SELECTED` (Android 14+ partial access), `ACCESS_MEDIA_LOCATION` and
`READ_EXTERNAL_STORAGE` (`maxSdkVersion="32"`). Google Play allows the `READ_MEDIA_*` permissions
only for apps whose core feature needs broad photo access and asks for a declaration in the Play
Console.

## Usage

```rust,ignore
use masamune_whisker_photo_library::{self as photos, AssetKind, AssetQuery, PermissionStatus};

let status = match photos::permission_status() {
    PermissionStatus::NotDetermined => photos::request_permission().await?,
    status => status,
};
if status.can_read() {
    let assets = photos::list_assets(&AssetQuery {
        from_ms: Some(day_start_ms),
        to_ms: Some(day_end_ms),
        kinds: vec![AssetKind::Image],
        limit: 50,
        include_location: true,
        ..Default::default()
    })
    .await?;
    for asset in assets {
        let thumb = photos::thumbnail(&asset.id, 200).await?; // thumb.path: a 200x200 JPEG
    }
}
```

## Behavior and limits

- `thumbnail` center-crops to a square; `image` keeps the aspect ratio. Files are reused for the
  same asset, size and mode.
- Android cannot tell "not asked yet" from "denied"; the crate remembers that it asked.
- With `include_location` Android opens every listed original, so page through large ranges with
  `offset` / `limit`. Android reads the requested range in full and sorts in memory (dates come
  from `DATE_TAKEN`, or `DATE_ADDED` when missing).
- `open_asset` shows an asset in the gallery app on Android (`ACTION_VIEW` on its MediaStore URI).
  iOS has no public API to open a given asset in the Photos app, so it returns `false` there.
- Only reading is supported (no saving or deleting).
