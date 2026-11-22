#![doc = include_str!("../README.md")]

pub mod cng;

use masamune_whisker_bridge::NativeModule;
pub use masamune_whisker_bridge::{NativeError, NativeResult};
use serde::{Deserialize, Serialize};
use serde_json::json;

fn module() -> NativeModule {
    NativeModule::new(env!("CARGO_PKG_NAME"), "PhotoLibrary")
}

/// Photo library permission.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionStatus {
    /// Not asked yet: [`request_permission`] shows the system prompt.
    NotDetermined,
    /// Refused. Only the system settings can change it.
    Denied,
    /// Blocked by parental controls or a device policy (iOS).
    Restricted,
    /// Only the photos the user selected are readable (iOS 14+, Android 14+).
    Limited,
    /// The whole library is readable.
    Granted,
}

impl PermissionStatus {
    /// Whether at least some photos can be read.
    pub fn can_read(self) -> bool {
        matches!(self, Self::Limited | Self::Granted)
    }

    fn decode(value: &str) -> Self {
        match value {
            "notDetermined" => Self::NotDetermined,
            "restricted" => Self::Restricted,
            "limited" => Self::Limited,
            "granted" => Self::Granted,
            _ => Self::Denied,
        }
    }
}

/// The current permission (read access). Errors read as [`PermissionStatus::Denied`].
pub fn permission_status() -> PermissionStatus {
    module()
        .call::<_, String>("status", &json!({}))
        .map_or(PermissionStatus::Denied, |status| {
            PermissionStatus::decode(&status)
        })
}

/// Asks for read access and returns the resulting status.
///
/// - iOS: `PHPhotoLibrary.requestAuthorization(for: .readWrite)`.
/// - Android 13+: `READ_MEDIA_IMAGES` and `READ_MEDIA_VIDEO` (with
///   `READ_MEDIA_VISUAL_USER_SELECTED` on 14+, where the user may pick photos instead), then
///   `ACCESS_MEDIA_LOCATION` so locations can be read. Android 12 and earlier:
///   `READ_EXTERNAL_STORAGE`.
pub async fn request_permission() -> NativeResult<PermissionStatus> {
    let status: String = module().call_async("requestPermission", &json!({})).await?;
    Ok(PermissionStatus::decode(&status))
}

/// Lets a user with [`PermissionStatus::Limited`] change the selected photos (iOS
/// `presentLimitedLibraryPicker`; Android 14+ asks for the permissions again, which shows the
/// photo picker). Returns the status afterwards.
pub async fn present_limited_picker() -> NativeResult<PermissionStatus> {
    let status: String = module()
        .call_async("presentLimitedPicker", &json!({}))
        .await?;
    Ok(PermissionStatus::decode(&status))
}

/// Opens this app's page in the system settings.
pub fn open_app_settings() -> NativeResult<bool> {
    module().call("openAppSettings", &json!({}))
}

/// Kind of a library asset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AssetKind {
    Image,
    Video,
}

/// A photo or video in the library.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Asset {
    /// Stable identifier for [`thumbnail`] and [`image`] (iOS `localIdentifier`; Android
    /// `image:<id>` / `video:<id>` of MediaStore).
    pub id: String,
    pub kind: AssetKind,
    /// When the photo was taken in UTC milliseconds (the time it was added when unknown).
    pub created_at_ms: i64,
    #[serde(default)]
    pub width: u32,
    #[serde(default)]
    pub height: u32,
    /// Video length in milliseconds (0 for images).
    #[serde(default)]
    pub duration_ms: u64,
    /// Where it was taken (requested with [`AssetQuery::include_location`]).
    #[serde(default)]
    pub latitude: Option<f64>,
    #[serde(default)]
    pub longitude: Option<f64>,
}

/// Which assets [`list_assets`] returns.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetQuery {
    /// Earliest creation time in UTC milliseconds (inclusive). `None` has no lower bound.
    pub from_ms: Option<i64>,
    /// Latest creation time in UTC milliseconds (exclusive). `None` has no upper bound.
    pub to_ms: Option<i64>,
    pub kinds: Vec<AssetKind>,
    /// Newest first (the default) or oldest first.
    pub newest_first: bool,
    /// Number of assets to skip, for paging.
    pub offset: u32,
    /// Most assets to return.
    pub limit: u32,
    /// Read where each asset was taken. On Android this opens every original (EXIF for images,
    /// metadata for videos) and needs `ACCESS_MEDIA_LOCATION`, so keep pages small.
    pub include_location: bool,
}

impl Default for AssetQuery {
    fn default() -> Self {
        Self {
            from_ms: None,
            to_ms: None,
            kinds: vec![AssetKind::Image, AssetKind::Video],
            newest_first: true,
            offset: 0,
            limit: 100,
            include_location: false,
        }
    }
}

/// Lists library assets by creation time. Returns an empty list without permission.
pub async fn list_assets(query: &AssetQuery) -> NativeResult<Vec<Asset>> {
    module().call_async("listAssets", query).await
}

/// An image file written by [`thumbnail`] or [`image`].
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct AssetImage {
    /// Absolute path of a JPEG in this app's cache directory
    /// (`<cache>/masamune_whisker_photo_library/`). The OS may clear it later.
    pub path: String,
    #[serde(default)]
    pub width: u32,
    #[serde(default)]
    pub height: u32,
}

/// A square-cropped thumbnail of `size` × `size` pixels (the first frame for videos). Files are
/// reused for the same asset and size.
pub async fn thumbnail(id: &str, size: u32) -> NativeResult<AssetImage> {
    module()
        .call_async(
            "thumbnail",
            &json!({ "id": id, "size": size.max(1), "crop": true }),
        )
        .await
}

/// The image (or a video frame) scaled so its longer side is at most `max_size` pixels, without
/// cropping. Downloads from iCloud when needed on iOS.
pub async fn image(id: &str, max_size: u32) -> NativeResult<AssetImage> {
    module()
        .call_async(
            "thumbnail",
            &json!({ "id": id, "size": max_size.max(1), "crop": false }),
        )
        .await
}

/// Reads where `id` was taken. `None` when the asset has no location or it cannot be read.
pub async fn asset_location(id: &str) -> NativeResult<Option<(f64, f64)>> {
    #[derive(Deserialize)]
    struct LatLng {
        latitude: f64,
        longitude: f64,
    }
    let location: Option<LatLng> = module()
        .call_async("assetLocation", &json!({ "id": id }))
        .await?;
    Ok(location.map(|location| (location.latitude, location.longitude)))
}

/// Opens `id` in the system photo viewer. Returns `false` when it cannot be opened.
///
/// - Android: an `ACTION_VIEW` intent for the asset's MediaStore content URI (with
///   `FLAG_GRANT_READ_URI_PERMISSION`), shown by the gallery app the user picks. `false` when the
///   id is not a MediaStore id or no app can view it.
/// - iOS: always `false`. There is no public API to open a given asset in the Photos app.
pub fn open_asset(id: &str) -> NativeResult<bool> {
    module().call("openAsset", &json!({ "id": id }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_statuses_and_assets() {
        assert_eq!(
            PermissionStatus::decode("limited"),
            PermissionStatus::Limited
        );
        assert!(PermissionStatus::Limited.can_read());
        assert!(!PermissionStatus::NotDetermined.can_read());
        assert_eq!(PermissionStatus::decode("x"), PermissionStatus::Denied);
        let asset: Asset = serde_json::from_str(
            r#"{"id":"image:1","kind":"image","createdAtMs":10,"latitude":35.6,"longitude":139.7}"#,
        )
        .unwrap();
        assert_eq!(asset.kind, AssetKind::Image);
        assert_eq!(asset.latitude, Some(35.6));
        assert_eq!(asset.duration_ms, 0);
        let video: Asset =
            serde_json::from_str(r#"{"id":"v","kind":"video","createdAtMs":1,"latitude":null}"#)
                .unwrap();
        assert_eq!(video.latitude, None);
    }

    #[test]
    fn encodes_queries() {
        let query = AssetQuery {
            from_ms: Some(1),
            to_ms: Some(2),
            kinds: vec![AssetKind::Image],
            ..Default::default()
        };
        assert_eq!(
            serde_json::to_value(&query).unwrap(),
            json!({
                "fromMs": 1, "toMs": 2, "kinds": ["image"], "newestFirst": true,
                "offset": 0, "limit": 100, "includeLocation": false,
            })
        );
    }
}
