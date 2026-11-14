// YouTube player element of masamune-whisker-youtube (Android), registered by YouTubePlayerModule.
//
// A WebView loads the player page built in Rust (`player_html`) with the `origin` prop as its
// base URL. The page reports `{"event":"ready"|"state"|"error", ...}` through the
// `MasamuneWhiskerYoutube` JavascriptInterface and is driven through `window.whiskerYoutube`.
//
// The element view is a FrameLayout hosting the WebView, so the WebView can be destroyed when the
// element is detached and created again (re-cueing the current video) when it is attached again
// or after its render process ended.
//
// WhiskerUI calls createView() from its constructor, before the properties of this class are
// initialized, so createView() only builds the container and the WebView is created in `init`.
//
// The WebView never takes focus: a native ScrollView around the player scrolls to a child that
// gains focus, which moved the page on the first tap on the video.

package net.mathru.whisker.youtube

import android.annotation.SuppressLint
import android.content.ActivityNotFoundException
import android.content.Context
import android.content.Intent
import android.graphics.Color
import android.graphics.Rect
import android.net.Uri
import android.os.Handler
import android.os.Looper
import android.view.View
import android.view.ViewGroup
import android.webkit.JavascriptInterface
import android.webkit.RenderProcessGoneDetail
import android.webkit.WebChromeClient
import android.webkit.WebResourceRequest
import android.webkit.WebSettings
import android.webkit.WebView
import android.webkit.WebViewClient
import android.widget.FrameLayout
import org.json.JSONObject
import rs.whisker.runtime.WhiskerContext
import rs.whisker.runtime.WhiskerCustomEvent
import rs.whisker.runtime.WhiskerUI

class YouTubePlayerView(context: WhiskerContext) : WhiskerUI<FrameLayout>(context) {
    companion object {
        /** Name of the JavascriptInterface the page posts to. */
        private const val BRIDGE_NAME = "MasamuneWhiskerYoutube"
        private const val DEFAULT_ORIGIN = "https://www.youtube-nocookie.com"

        /** Keeps only `A-Z a-z 0-9 - _`, so the ID is safe inside a JavaScript string literal. */
        fun sanitize(value: String): String =
            value.filter { it in 'a'..'z' || it in 'A'..'Z' || it in '0'..'9' || it == '-' || it == '_' }
    }

    private val mainHandler = Handler(Looper.getMainLooper())
    private var webView: WebView? = null

    /** Incremented for every WebView, so messages of a destroyed one are dropped. */
    private var generation = 0

    private var origin = DEFAULT_ORIGIN
    private var html = ""
    private var autoplay = false
    private var loopVideo = false

    /** Last value of the `video-id` prop. */
    private var videoIdProp = ""

    /** Video shown by the player (re-requested after the page reloads). */
    private var currentVideoId = ""

    /** Video waiting for the player to become ready, and whether to load (play) it. */
    private var pendingVideoId: String? = null
    private var pendingLoad = false
    private var ready = false

    init {
        replaceWebView()
    }

    override fun createView(context: Context): FrameLayout = object : FrameLayout(context) {
        // Keep the WebView's focus and "show this rectangle" requests away from an enclosing
        // ScrollView, which would otherwise scroll on the first tap on the player.
        override fun requestChildRectangleOnScreen(child: View, rectangle: Rect, immediate: Boolean): Boolean =
            false
    }.apply {
        setBackgroundColor(Color.BLACK)
        descendantFocusability = ViewGroup.FOCUS_BLOCK_DESCENDANTS
    }

    // ---- props ------------------------------------------------------------------------------

    fun setOrigin(value: String) {
        origin = value.ifEmpty { DEFAULT_ORIGIN }
    }

    fun setPlayerHtml(value: String) {
        if (value == html) return
        html = value
        loadPage()
    }

    fun setAutoplay(value: Boolean) {
        autoplay = value
    }

    fun setLoopVideo(value: Boolean) {
        loopVideo = value
        if (ready) evaluate("window.whiskerYoutube.setLoop($value);")
    }

    /** The first video is cued (or loaded with autoplay); later changes load the new video. */
    fun setVideoId(value: String) {
        val id = sanitize(value)
        if (id.isEmpty() || id == videoIdProp) return
        val first = currentVideoId.isEmpty() && pendingVideoId == null
        videoIdProp = id
        request(id, autoplay || !first)
    }

    // ---- commands ---------------------------------------------------------------------------

    fun play() = call("play()")

    fun pause() = call("pause()")

    fun cueVideo(value: String) {
        val id = sanitize(value)
        if (id.isNotEmpty()) request(id, false)
    }

    fun loadVideo(value: String) {
        val id = sanitize(value)
        if (id.isNotEmpty()) request(id, true)
    }

    fun seek(seconds: Double) {
        val value = if (seconds.isFinite()) seconds.coerceAtLeast(0.0) else 0.0
        call("seek($value)")
    }

    fun mute() = call("mute()")

    fun unmute() = call("unmute()")

    // ---- lifecycle --------------------------------------------------------------------------

    override fun onAttachedToWindow() {
        super.onAttachedToWindow()
        if (webView == null) replaceWebView()
    }

    override fun onDetachedFromWindow() {
        super.onDetachedFromWindow()
        // A Host reparent detaches and attaches in the same frame: keep the player in that case.
        mainHandler.post {
            if (!isAttachedToWindow) {
                webView?.let { old ->
                    view().removeView(old)
                    destroy(old)
                }
                webView = null
                ready = false
            }
        }
    }

    // ---- player -----------------------------------------------------------------------------

    private fun replaceWebView() {
        webView?.let { old ->
            view().removeView(old)
            destroy(old)
        }
        generation += 1
        val wv = createWebView(generation)
        webView = wv
        view().addView(
            wv,
            FrameLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT),
        )
        loadPage()
    }

    @SuppressLint("SetJavaScriptEnabled", "AddJavascriptInterface")
    private fun createWebView(generation: Int): WebView {
        val wv = WebView(context)
        wv.settings.apply {
            javaScriptEnabled = true
            domStorageEnabled = true
            mediaPlaybackRequiresUserGesture = false
            cacheMode = WebSettings.LOAD_DEFAULT
            mixedContentMode = WebSettings.MIXED_CONTENT_NEVER_ALLOW
            allowFileAccess = false
            allowContentAccess = false
            setSupportZoom(false)
            builtInZoomControls = false
            displayZoomControls = false
            // `target="_blank"` links navigate the main frame and are handed to the system.
            setSupportMultipleWindows(false)
        }
        wv.setBackgroundColor(Color.BLACK)
        wv.isVerticalScrollBarEnabled = false
        wv.isHorizontalScrollBarEnabled = false
        wv.overScrollMode = View.OVER_SCROLL_NEVER
        // The player has no text input; touches work without focus.
        wv.isFocusable = false
        wv.isFocusableInTouchMode = false
        wv.addJavascriptInterface(Bridge(generation), BRIDGE_NAME)
        wv.webViewClient = Client()
        wv.webChromeClient = ChromeClient()
        return wv
    }

    private fun destroy(wv: WebView) {
        wv.stopLoading()
        wv.removeJavascriptInterface(BRIDGE_NAME)
        wv.webChromeClient = null
        wv.destroy()
    }

    private fun loadPage() {
        val wv = webView ?: return
        if (html.isEmpty()) return
        ready = false
        // Re-request the shown video once the new page is ready.
        if (pendingVideoId == null && currentVideoId.isNotEmpty()) {
            pendingVideoId = currentVideoId
            pendingLoad = autoplay
        }
        wv.loadDataWithBaseURL(origin, html, "text/html", "utf-8", null)
    }

    private fun request(id: String, load: Boolean) {
        pendingVideoId = id
        pendingLoad = load
        flushPendingVideo()
    }

    private fun flushPendingVideo() {
        if (!ready) return
        val id = pendingVideoId ?: return
        pendingVideoId = null
        currentVideoId = id
        evaluate("window.whiskerYoutube.${if (pendingLoad) "load" else "cue"}('$id');")
    }

    private fun call(expression: String) {
        if (ready) evaluate("window.whiskerYoutube.$expression;")
    }

    private fun evaluate(script: String) {
        webView?.evaluateJavascript(script, null)
    }

    private fun handleMessage(json: String) {
        val message = try {
            JSONObject(json)
        } catch (_: Throwable) {
            return
        }
        when (message.optString("event")) {
            "ready" -> {
                ready = true
                if (loopVideo) evaluate("window.whiskerYoutube.setLoop(true);")
                flushPendingVideo()
                emit("ready")
            }
            "state" -> emit("state_change", mapOf("state" to message.optInt("state", -1)))
            "error" -> emit("error", mapOf("code" to message.optInt("code", -1)))
        }
    }

    private fun emit(name: String, params: Map<String, Any?> = emptyMap()) {
        WhiskerCustomEvent.dispatch(ui = this, name = name, params = params)
    }

    /** Opens [uri] in the system (YouTube app or browser). */
    private fun openExternally(uri: Uri) {
        val scheme = uri.scheme?.lowercase()
        if (scheme != "http" && scheme != "https") return
        try {
            context.startActivity(Intent(Intent.ACTION_VIEW, uri).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
        } catch (_: ActivityNotFoundException) {
        }
    }

    // ---- WebView callbacks ------------------------------------------------------------------

    /** Called on a WebView background thread; handled on the main thread. */
    private inner class Bridge(private val generation: Int) {
        @JavascriptInterface
        fun postMessage(json: String?) {
            val body = json ?: return
            mainHandler.post {
                if (generation == this@YouTubePlayerView.generation && webView != null) handleMessage(body)
            }
        }
    }

    private inner class Client : WebViewClient() {
        /** Only the player page stays in the main frame; links open in the system. */
        override fun shouldOverrideUrlLoading(view: WebView, request: WebResourceRequest): Boolean {
            if (!request.isForMainFrame) return false
            openExternally(request.url)
            return true
        }

        override fun onRenderProcessGone(view: WebView, detail: RenderProcessGoneDetail): Boolean {
            val gone = view
            if (gone === webView) {
                webView = null
                ready = false
                this@YouTubePlayerView.view().removeView(gone)
                destroy(gone)
                if (isAttachedToWindow) replaceWebView()
            } else {
                gone.destroy()
            }
            return true
        }
    }

    private inner class ChromeClient : WebChromeClient() {
        /** Fullscreen is not supported: the request is dismissed right away. */
        override fun onShowCustomView(view: View?, callback: CustomViewCallback?) {
            callback?.onCustomViewHidden()
        }

        override fun onHideCustomView() {}
    }
}
