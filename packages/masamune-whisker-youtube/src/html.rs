//! The HTML page that hosts the YouTube IFrame Player inside the native web view.

/// Default page origin: YouTube's privacy-enhanced (no-cookie) domain.
///
/// The page is loaded with this origin as its base URL and passes it to the player as
/// `playerVars.origin`, which YouTube requires for embedded playback.
pub const DEFAULT_ORIGIN: &str = "https://www.youtube-nocookie.com";

/// Name of the iOS script message handler the page posts to.
pub(crate) const IOS_HANDLER: &str = "masamuneWhiskerYoutube";

/// Name of the Android JavaScript interface the page posts to.
pub(crate) const ANDROID_BRIDGE: &str = "MasamuneWhiskerYoutube";

/// Player host for `origin`: the origin itself when it is a YouTube domain, otherwise
/// `https://www.youtube.com`.
pub(crate) fn player_host(origin: &str) -> &str {
    let host = origin
        .split_once("://")
        .map_or(origin, |(_, rest)| rest)
        .split(['/', ':'])
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    if host == "youtube.com"
        || host.ends_with(".youtube.com")
        || host == "youtube-nocookie.com"
        || host.ends_with(".youtube-nocookie.com")
    {
        origin.trim_end_matches('/')
    } else {
        "https://www.youtube.com"
    }
}

/// Normalizes an origin prop: trims it, drops a trailing `/`, and falls back to
/// [`DEFAULT_ORIGIN`] for anything that is not an `http(s)` origin.
pub(crate) fn normalize_origin(origin: &str) -> String {
    let origin = origin.trim().trim_end_matches('/');
    let lower = origin.to_ascii_lowercase();
    if (lower.starts_with("https://") || lower.starts_with("http://"))
        && origin.split_once("://").is_some_and(|(_, rest)| {
            !rest.is_empty() && !rest.contains(['/', '?', '#', '"', '\'', '<', '>', '\\', ' '])
        })
    {
        origin.to_string()
    } else {
        DEFAULT_ORIGIN.to_string()
    }
}

/// Keeps only the characters a YouTube video ID can contain (`A-Z a-z 0-9 - _`).
///
/// The native side applies the same filter before a video ID reaches JavaScript.
pub fn sanitize_video_id(video_id: &str) -> String {
    video_id
        .trim()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .collect()
}

/// Quotes `value` as a JavaScript string literal that is also safe inside a `<script>` block.
fn js_string(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for c in value.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '<' => out.push_str("\\u003C"),
            '>' => out.push_str("\\u003E"),
            '&' => out.push_str("\\u0026"),
            '\u{2028}' => out.push_str("\\u2028"),
            '\u{2029}' => out.push_str("\\u2029"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04X}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Builds the page that hosts the YouTube IFrame Player for `origin`.
///
/// The page loads `https://www.youtube.com/iframe_api`, creates a full-size `YT.Player`
/// (`playsinline`, no related videos from other channels, controls on, fullscreen button off,
/// `enablejsapi`, `origin`) and reports `onReady` / `onStateChange` / `onError` to the native
/// side as JSON (`{"event":"ready"}`, `{"event":"state","state":1}`,
/// `{"event":"error","code":150}`). The native element drives the player through
/// `window.whiskerYoutube` (`cue`, `load`, `play`, `pause`, `seek`, `mute`, `unmute`,
/// `setLoop`). An invalid `origin` falls back to [`DEFAULT_ORIGIN`].
pub fn player_html(origin: &str) -> String {
    let origin = normalize_origin(origin);
    let host = player_host(&origin).to_string();
    TEMPLATE
        .replace("__ORIGIN__", &js_string(&origin))
        .replace("__HOST__", &js_string(&host))
        .replace("__IOS_HANDLER__", IOS_HANDLER)
        .replace("__ANDROID_BRIDGE__", ANDROID_BRIDGE)
}

const TEMPLATE: &str = r#"<!DOCTYPE html>
<html>
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1, maximum-scale=1, user-scalable=no">
<style>
html, body { margin: 0; padding: 0; width: 100%; height: 100%; overflow: hidden; background-color: #000; }
#player { position: absolute; top: 0; left: 0; width: 100%; height: 100%; }
</style>
</head>
<body>
<div id="player"></div>
<script>
(function() {
  var player = null;
  var ready = false;
  var loop = false;
  function post(message) {
    var json = JSON.stringify(message);
    try {
      if (window.webkit && window.webkit.messageHandlers && window.webkit.messageHandlers.__IOS_HANDLER__) {
        window.webkit.messageHandlers.__IOS_HANDLER__.postMessage(json);
        return;
      }
      if (window.__ANDROID_BRIDGE__) {
        window.__ANDROID_BRIDGE__.postMessage(json);
      }
    } catch (e) {}
  }
  function call(name, args) {
    if (!ready || !player || typeof player[name] !== 'function') { return; }
    try { player[name].apply(player, args || []); } catch (e) {}
  }
  window.whiskerYoutube = {
    cue: function(id) { call('cueVideoById', [{ videoId: id }]); },
    load: function(id) { call('loadVideoById', [{ videoId: id }]); },
    play: function() { call('playVideo'); },
    pause: function() { call('pauseVideo'); },
    seek: function(seconds) { call('seekTo', [seconds, true]); },
    mute: function() { call('mute'); },
    unmute: function() { call('unMute'); },
    setLoop: function(value) { loop = !!value; }
  };
  window.onYouTubeIframeAPIReady = function() {
    player = new YT.Player('player', {
      width: '100%',
      height: '100%',
      host: __HOST__,
      playerVars: {
        playsinline: 1,
        rel: 0,
        modestbranding: 1,
        controls: 1,
        fs: 0,
        enablejsapi: 1,
        origin: __ORIGIN__
      },
      events: {
        onReady: function() {
          ready = true;
          post({ event: 'ready' });
        },
        onStateChange: function(event) {
          post({ event: 'state', state: event.data });
          if (loop && event.data === 0) {
            call('seekTo', [0, true]);
            call('playVideo');
          }
        },
        onError: function(event) {
          post({ event: 'error', code: event.data });
        }
      }
    });
  };
  var tag = document.createElement('script');
  tag.src = 'https://www.youtube.com/iframe_api';
  tag.onerror = function() { post({ event: 'error', code: -1 }); };
  document.head.appendChild(tag);
})();
</script>
</body>
</html>
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn html_embeds_origin_and_player_vars() {
        let html = player_html(DEFAULT_ORIGIN);
        assert!(html.contains("https://www.youtube.com/iframe_api"));
        assert!(html.contains(r#"origin: "https://www.youtube-nocookie.com""#));
        assert!(html.contains(r#"host: "https://www.youtube-nocookie.com""#));
        assert!(html.contains("playsinline: 1"));
        assert!(html.contains("enablejsapi: 1"));
        assert!(html.contains("rel: 0"));
        assert!(html.contains("fs: 0"));
        assert!(html.contains("window.webkit.messageHandlers.masamuneWhiskerYoutube"));
        assert!(html.contains("window.MasamuneWhiskerYoutube.postMessage"));
        assert!(!html.contains("__"));
    }

    #[test]
    fn non_youtube_origin_uses_default_host() {
        let html = player_html("https://example.com/");
        assert!(html.contains(r#"origin: "https://example.com""#));
        assert!(html.contains(r#"host: "https://www.youtube.com""#));
    }

    #[test]
    fn invalid_origin_falls_back_to_default() {
        for origin in [
            "",
            "javascript:alert(1)",
            "https://",
            "https://a.com/\"</script>",
            "file:///etc",
        ] {
            assert_eq!(normalize_origin(origin), DEFAULT_ORIGIN, "{origin}");
        }
        assert_eq!(
            normalize_origin(" https://www.youtube.com/ "),
            "https://www.youtube.com"
        );
        assert_eq!(
            normalize_origin("http://localhost:8080"),
            "http://localhost:8080"
        );
    }

    #[test]
    fn player_host_accepts_youtube_domains_only() {
        assert_eq!(
            player_host("https://www.youtube.com"),
            "https://www.youtube.com"
        );
        assert_eq!(player_host(DEFAULT_ORIGIN), DEFAULT_ORIGIN);
        assert_eq!(
            player_host("https://youtube.com.evil.example"),
            "https://www.youtube.com"
        );
        assert_eq!(
            player_host("https://notyoutube.com"),
            "https://www.youtube.com"
        );
    }

    #[test]
    fn video_ids_are_sanitized() {
        assert_eq!(sanitize_video_id(" dQw4w9WgXcQ "), "dQw4w9WgXcQ");
        assert_eq!(sanitize_video_id("a'b\"c);d-_e"), "abcd-_e");
        assert_eq!(sanitize_video_id(""), "");
    }

    #[test]
    fn js_strings_are_escaped() {
        assert_eq!(js_string("a\"b\\c"), r#""a\"b\\c""#);
        assert_eq!(js_string("</script>"), "\"\\u003C/script\\u003E\"");
        assert_eq!(js_string("\u{1}"), r#""\u0001""#);
    }
}
