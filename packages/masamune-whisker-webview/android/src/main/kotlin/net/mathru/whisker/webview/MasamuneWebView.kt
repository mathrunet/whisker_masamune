// Web view element of masamune-whisker-webview (Android), registered by WebViewModule.
//
// The element view is a FrameLayout hosting the android.webkit.WebView, so the WebView can be
// replaced after its render process ends (onRenderProcessGone) or after the element was detached
// and attached again.
//
// WhiskerUI calls createView() from its constructor, before the properties of this class are
// initialized, so createView() only builds the container and the WebView is created in `init`.
//
// JS bridge: `window.whisker` is defined at document start in the main frame by BRIDGE_SHIM.
// Page -> Rust goes through a WebMessageListener (which reports the frame and origin of the sender)
// or, where that feature is missing, a JavascriptInterface. Rust -> page scripts are built in Rust
// and run with evaluateJavascript.

package net.mathru.whisker.webview

import android.Manifest
import android.annotation.SuppressLint
import android.app.Activity
import android.content.ActivityNotFoundException
import android.content.Context
import android.content.ContextWrapper
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Handler
import android.os.Looper
import android.os.Message
import android.view.ViewGroup
import android.webkit.JavascriptInterface
import android.webkit.PermissionRequest
import android.webkit.RenderProcessGoneDetail
import android.webkit.ValueCallback
import android.webkit.WebChromeClient
import android.webkit.WebResourceError
import android.webkit.WebResourceRequest
import android.webkit.WebSettings
import android.webkit.WebView
import android.webkit.WebViewClient
import android.widget.FrameLayout
import androidx.activity.ComponentActivity
import androidx.activity.result.ActivityResultLauncher
import androidx.activity.result.contract.ActivityResultContracts
import androidx.core.content.ContextCompat
import androidx.webkit.ScriptHandler
import androidx.webkit.WebViewCompat
import androidx.webkit.WebViewFeature
import org.json.JSONArray
import rs.whisker.runtime.WhiskerAppContext
import rs.whisker.runtime.WhiskerContext
import rs.whisker.runtime.WhiskerCustomEvent
import rs.whisker.runtime.WhiskerUI
import java.util.concurrent.atomic.AtomicInteger

class MasamuneWebView(context: WhiskerContext) : WhiskerUI<FrameLayout>(context) {
    companion object {
        /** Name of the native object the shim posts to (WebMessageListener or JavascriptInterface). */
        private const val BRIDGE_NAME = "__masamuneWhiskerBridge"

        /** Defines `window.whisker` before any page script runs, in the main frame only. */
        private val BRIDGE_SHIM = """
            (function() {
              if (window.top !== window) { return; }
              var bridge = window.$BRIDGE_NAME;
              var whisker = window.whisker || {};
              whisker.postMessage = function(data) {
                var s = (typeof data === 'string') ? data : JSON.stringify(data);
                if (bridge) { bridge.postMessage(String(s)); }
              };
              whisker.isMasamuneWebView = true;
              window.whisker = whisker;
            })();
        """.trimIndent()

        private val requestKeys = AtomicInteger(0)

        /** Origin of [uri] (`scheme://host[:port]`, default ports omitted), or null for non-web URLs. */
        fun originOf(uri: Uri?): String? {
            val scheme = uri?.scheme?.lowercase() ?: return null
            val defaultPort = when (scheme) {
                "http" -> 80
                "https" -> 443
                else -> return null
            }
            val host = uri.host?.lowercase()?.takeIf { it.isNotEmpty() } ?: return null
            val port = uri.port
            return if (port == -1 || port == defaultPort) "$scheme://$host" else "$scheme://$host:$port"
        }

        private fun parseStringArray(json: String): List<String>? = try {
            val array = JSONArray(json)
            List(array.length()) { array.getString(it) }
        } catch (_: Throwable) {
            null
        }
    }

    private val mainHandler = Handler(Looper.getMainLooper())
    private var webView: WebView? = null
    private val documentStartSupported = WebViewFeature.isFeatureSupported(WebViewFeature.DOCUMENT_START_SCRIPT)
    private var userScriptHandlers: List<ScriptHandler> = emptyList()

    private var userAgent = ""
    private var javascriptEnabled = true
    private var grantMediaPermissions = false
    private var allowedOrigins: List<String> = emptyList()
    private var userScripts: List<String> = emptyList()

    /** Last value of the `url` prop that was loaded (null while showing `html`). */
    private var requestedUrl: String? = null
    private var html = ""

    /** Last committed main-frame URL, reloaded when the WebView is recreated. */
    private var currentUrl: String? = null
    private var pendingFileCallback: ValueCallback<Array<Uri>>? = null

    init {
        replaceWebView(null)
    }

    override fun createView(context: Context): FrameLayout = FrameLayout(context)

    // ---- props ------------------------------------------------------------------------------

    fun setUserAgent(value: String) {
        userAgent = value
        webView?.settings?.userAgentString = value.ifEmpty { null }
    }

    @SuppressLint("SetJavaScriptEnabled")
    fun setJavascriptEnabled(enabled: Boolean) {
        javascriptEnabled = enabled
        webView?.settings?.javaScriptEnabled = enabled
    }

    fun setGrantMediaPermissions(grant: Boolean) {
        grantMediaPermissions = grant
    }

    /** Malformed JSON keeps the previous list, so a bad value never opens navigation up. */
    fun setAllowedOrigins(json: String) {
        allowedOrigins = parseStringArray(json) ?: return
    }

    fun setUserScripts(json: String) {
        val scripts = parseStringArray(json) ?: return
        if (scripts == userScripts) return
        userScripts = scripts
        webView?.let { installUserScripts(it) }
    }

    fun setUrl(value: String) {
        if (value.isEmpty()) {
            if (requestedUrl != null) {
                requestedUrl = null
                webView?.let { loadInline(it) }
            }
            return
        }
        if (value == requestedUrl) return
        requestedUrl = value
        webView?.let { load(it, value) }
    }

    fun setHtml(value: String) {
        html = value
        if (requestedUrl == null) webView?.let { loadInline(it) }
    }

    // ---- commands ---------------------------------------------------------------------------

    fun reload() {
        webView?.reload()
    }

    fun goBack() {
        webView?.takeIf { it.canGoBack() }?.goBack()
    }

    fun goForward() {
        webView?.takeIf { it.canGoForward() }?.goForward()
    }

    fun stopLoading() {
        webView?.stopLoading()
    }

    fun evaluateJavascript(script: String) {
        if (script.isNotEmpty()) webView?.evaluateJavascript(script, null)
    }

    fun loadUrlCommand(url: String) {
        webView?.let { load(it, url) }
    }

    // ---- lifecycle --------------------------------------------------------------------------

    override fun onAttachedToWindow() {
        super.onAttachedToWindow()
        if (webView == null) replaceWebView(currentUrl ?: requestedUrl)
    }

    override fun onDetachedFromWindow() {
        super.onDetachedFromWindow()
        // A Host reparent detaches and attaches in the same frame: keep the page in that case.
        mainHandler.post {
            if (!isAttachedToWindow) {
                webView?.let { old ->
                    view().removeView(old)
                    destroy(old)
                }
                webView = null
                cancelFileChooser()
            }
        }
    }

    // ---- WebView setup ----------------------------------------------------------------------

    /** Replaces the hosted WebView with a new one and loads [url] (or the current content). */
    private fun replaceWebView(url: String?) {
        webView?.let { old ->
            view().removeView(old)
            destroy(old)
        }
        val wv = createWebView()
        webView = wv
        view().addView(
            wv,
            FrameLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT),
        )
        when {
            url != null -> load(wv, url)
            requestedUrl != null -> load(wv, requestedUrl!!)
            html.isNotEmpty() -> loadInline(wv)
        }
    }

    @SuppressLint("SetJavaScriptEnabled")
    @Suppress("DEPRECATION")
    private fun createWebView(): WebView {
        val wv = WebView(context)
        wv.settings.apply {
            javaScriptEnabled = javascriptEnabled
            userAgentString = userAgent.ifEmpty { null }
            domStorageEnabled = true
            databaseEnabled = true
            cacheMode = WebSettings.LOAD_DEFAULT
            mixedContentMode = WebSettings.MIXED_CONTENT_NEVER_ALLOW
            allowFileAccess = false
            allowContentAccess = false
            allowFileAccessFromFileURLs = false
            allowUniversalAccessFromFileURLs = false
            setSupportZoom(false)
            builtInZoomControls = false
            displayZoomControls = false
            mediaPlaybackRequiresUserGesture = false
            setSupportMultipleWindows(true)
            javaScriptCanOpenWindowsAutomatically = true
        }
        // Opaque background (the WebView default); never transparent.
        wv.setBackgroundColor(android.graphics.Color.WHITE)

        if (WebViewFeature.isFeatureSupported(WebViewFeature.WEB_MESSAGE_LISTENER)) {
            WebViewCompat.addWebMessageListener(wv, BRIDGE_NAME, setOf("*")) { _, message, sourceOrigin, isMainFrame, _ ->
                if (isMainFrame && acceptsMessagesFrom(sourceOrigin.toString())) {
                    emit("message", mapOf("data" to (message.data ?: "")))
                }
            }
        } else {
            wv.addJavascriptInterface(LegacyBridge(), BRIDGE_NAME)
        }
        if (documentStartSupported) {
            WebViewCompat.addDocumentStartJavaScript(wv, BRIDGE_SHIM, setOf("*"))
        }
        userScriptHandlers = emptyList()
        installUserScripts(wv)

        wv.webViewClient = Client()
        wv.webChromeClient = ChromeClient()
        return wv
    }

    /**
     * Document-start scripts run in every frame, so each user script is guarded to the main
     * frame. Top-level `let` / `const` stay inside the guard block; assign globals through `window`.
     */
    private fun installUserScripts(wv: WebView) {
        if (!documentStartSupported) return
        userScriptHandlers.forEach { it.remove() }
        userScriptHandlers = userScripts.map { script ->
            WebViewCompat.addDocumentStartJavaScript(wv, "if (window.top === window) {\n$script\n}", setOf("*"))
        }
    }

    private fun destroy(wv: WebView) {
        wv.stopLoading()
        if (!WebViewFeature.isFeatureSupported(WebViewFeature.WEB_MESSAGE_LISTENER)) {
            wv.removeJavascriptInterface(BRIDGE_NAME)
        }
        wv.webChromeClient = null
        wv.destroy()
        userScriptHandlers = emptyList()
    }

    private fun load(wv: WebView, url: String) {
        val scheme = Uri.parse(url).scheme?.lowercase()
        if (scheme == "http" || scheme == "https" || url == "about:blank") wv.loadUrl(url)
    }

    private fun loadInline(wv: WebView) {
        if (html.isEmpty()) {
            wv.loadUrl("about:blank")
        } else {
            wv.loadDataWithBaseURL(null, html, "text/html", "utf-8", null)
        }
    }

    // ---- policy -----------------------------------------------------------------------------

    private fun isAllowedOrigin(origin: String?): Boolean =
        origin != null && (allowedOrigins.isEmpty() || allowedOrigins.contains(origin))

    /** Messages come from allowed origins, or from the inline `html` page (opaque origin). */
    private fun acceptsMessagesFrom(sourceOrigin: String): Boolean {
        if (sourceOrigin == "null") return requestedUrl == null
        return isAllowedOrigin(originOf(Uri.parse(sourceOrigin)))
    }

    /** Returns true to cancel the navigation. */
    private fun shouldBlock(uri: Uri, isMainFrame: Boolean): Boolean {
        when (uri.scheme?.lowercase()) {
            "http", "https" -> {
                if (isAllowedOrigin(originOf(uri))) return false
                if (isMainFrame) emit("external_navigation", mapOf("url" to uri.toString()))
                return true
            }
            "about" -> return false
            // file:, content:, javascript:, intent:, custom schemes, data: and blob: navigations.
            else -> return true
        }
    }

    private fun emit(name: String, params: Map<String, Any?> = emptyMap()) {
        WhiskerCustomEvent.dispatch(ui = this, name = name, params = params)
    }

    // ---- Activity access --------------------------------------------------------------------

    private fun activity(): ComponentActivity? {
        var current: Context? = context
        while (current is ContextWrapper) {
            if (current is ComponentActivity) return current
            current = current.baseContext
        }
        return WhiskerAppContext.shared.currentActivity as? ComponentActivity
    }

    private fun nextKey() = "masamune_whisker_webview_${requestKeys.incrementAndGet()}"

    // ---- media permissions ------------------------------------------------------------------

    private fun handlePermissionRequest(request: PermissionRequest) {
        if (!grantMediaPermissions || !isAllowedOrigin(originOf(request.origin))) {
            request.deny()
            return
        }
        val resources = request.resources.filter {
            it == PermissionRequest.RESOURCE_VIDEO_CAPTURE || it == PermissionRequest.RESOURCE_AUDIO_CAPTURE
        }
        if (resources.isEmpty()) {
            request.deny()
            return
        }
        val missing = resources.map(::androidPermission).filter { !hasPermission(it) }.distinct()
        val activity = activity()
        if (missing.isEmpty() || activity == null) {
            grantHeld(request, resources)
            return
        }
        var launcher: ActivityResultLauncher<Array<String>>? = null
        launcher = activity.activityResultRegistry.register(
            nextKey(),
            ActivityResultContracts.RequestMultiplePermissions(),
        ) {
            launcher?.unregister()
            grantHeld(request, resources)
        }
        launcher.launch(missing.toTypedArray())
    }

    /** Grants the resources whose runtime permission the app holds, or denies the request. */
    private fun grantHeld(request: PermissionRequest, resources: List<String>) {
        val granted = resources.filter { hasPermission(androidPermission(it)) }
        if (granted.isEmpty()) request.deny() else request.grant(granted.toTypedArray())
    }

    private fun androidPermission(resource: String): String =
        if (resource == PermissionRequest.RESOURCE_VIDEO_CAPTURE) Manifest.permission.CAMERA else Manifest.permission.RECORD_AUDIO

    private fun hasPermission(permission: String): Boolean =
        ContextCompat.checkSelfPermission(context, permission) == PackageManager.PERMISSION_GRANTED

    // ---- file chooser -----------------------------------------------------------------------

    private fun showFileChooser(callback: ValueCallback<Array<Uri>>, params: WebChromeClient.FileChooserParams): Boolean {
        val activity = activity() ?: return false
        cancelFileChooser()
        val intent = try {
            params.createIntent()
        } catch (_: Throwable) {
            callback.onReceiveValue(null)
            return true
        }
        if (params.mode == WebChromeClient.FileChooserParams.MODE_OPEN_MULTIPLE) {
            intent.putExtra(Intent.EXTRA_ALLOW_MULTIPLE, true)
        }
        pendingFileCallback = callback
        var launcher: ActivityResultLauncher<Intent>? = null
        launcher = activity.activityResultRegistry.register(
            nextKey(),
            ActivityResultContracts.StartActivityForResult(),
        ) { result ->
            launcher?.unregister()
            val pending = pendingFileCallback
            pendingFileCallback = null
            if (pending !== callback) return@register
            callback.onReceiveValue(selectedUris(result.resultCode, result.data))
        }
        try {
            launcher.launch(intent)
        } catch (_: ActivityNotFoundException) {
            launcher.unregister()
            pendingFileCallback = null
            callback.onReceiveValue(null)
        }
        return true
    }

    private fun selectedUris(resultCode: Int, data: Intent?): Array<Uri>? {
        if (resultCode != Activity.RESULT_OK || data == null) return null
        val clip = data.clipData
        if (clip != null && clip.itemCount > 0) {
            return Array(clip.itemCount) { clip.getItemAt(it).uri }
        }
        return data.data?.let { arrayOf(it) }
    }

    private fun cancelFileChooser() {
        pendingFileCallback?.onReceiveValue(null)
        pendingFileCallback = null
    }

    // ---- new windows ------------------------------------------------------------------------

    /**
     * Hands `window.open` / `target="_blank"` to Rust without creating a window: a throwaway
     * WebView receives the first navigation of the new window and is destroyed.
     */
    private fun captureNewWindow(resultMsg: Message?): Boolean {
        val transport = resultMsg?.obj as? WebView.WebViewTransport ?: return false
        val probe = WebView(context)
        var finished = false
        fun finish(url: String?) {
            if (finished) return
            finished = true
            if (!url.isNullOrEmpty() && url != "about:blank") emit("open_window", mapOf("url" to url))
            mainHandler.post { probe.destroy() }
        }
        probe.webViewClient = object : WebViewClient() {
            override fun shouldOverrideUrlLoading(view: WebView, request: WebResourceRequest): Boolean {
                finish(request.url.toString())
                return true
            }

            override fun onPageStarted(view: WebView, url: String?, favicon: android.graphics.Bitmap?) {
                if (url != null && url != "about:blank") {
                    view.stopLoading()
                    finish(url)
                }
            }
        }
        // window.open('') never navigates: drop the probe after a while.
        mainHandler.postDelayed({ finish(null) }, 10_000)
        transport.webView = probe
        resultMsg.sendToTarget()
        return true
    }

    // ---- clients ----------------------------------------------------------------------------

    private inner class Client : WebViewClient() {
        override fun onPageStarted(view: WebView, url: String?, favicon: android.graphics.Bitmap?) {
            if (view !== webView) return
            if (!documentStartSupported) {
                view.evaluateJavascript(BRIDGE_SHIM, null)
                userScripts.forEach { view.evaluateJavascript(it, null) }
            }
            currentUrl = url
            emit("load_start", mapOf("url" to (url ?: "")))
        }

        override fun onPageFinished(view: WebView, url: String?) {
            if (view !== webView) return
            emit("load", mapOf("url" to (url ?: "")))
        }

        override fun doUpdateVisitedHistory(view: WebView, url: String?, isReload: Boolean) {
            if (view === webView && url != null) currentUrl = url
        }

        override fun onReceivedError(view: WebView, request: WebResourceRequest, error: WebResourceError) {
            if (view !== webView || !request.isForMainFrame) return
            emit(
                "error",
                mapOf(
                    "url" to request.url.toString(),
                    "code" to error.errorCode,
                    "description" to error.description.toString(),
                ),
            )
        }

        override fun shouldOverrideUrlLoading(view: WebView, request: WebResourceRequest): Boolean =
            shouldBlock(request.url, request.isForMainFrame)

        override fun onRenderProcessGone(view: WebView, detail: RenderProcessGoneDetail): Boolean {
            // The dead WebView must not be used again; replace it outside this callback.
            if (view === webView) {
                webView = null
                mainHandler.post {
                    this@MasamuneWebView.view().removeView(view)
                    view.destroy()
                    replaceWebView(currentUrl ?: requestedUrl)
                    emit("render_process_gone")
                }
            }
            return true
        }
    }

    private inner class ChromeClient : WebChromeClient() {
        override fun onProgressChanged(view: WebView, newProgress: Int) {
            if (view === webView) emit("progress", mapOf("progress" to newProgress / 100.0))
        }

        override fun onCreateWindow(view: WebView, isDialog: Boolean, isUserGesture: Boolean, resultMsg: Message?): Boolean =
            captureNewWindow(resultMsg)

        override fun onPermissionRequest(request: PermissionRequest) {
            handlePermissionRequest(request)
        }

        override fun onShowFileChooser(
            webView: WebView,
            filePathCallback: ValueCallback<Array<Uri>>,
            fileChooserParams: FileChooserParams,
        ): Boolean = showFileChooser(filePathCallback, fileChooserParams)
    }

    /** Fallback bridge without frame information; runs on the JavaBridge thread. */
    private inner class LegacyBridge {
        @JavascriptInterface
        fun postMessage(data: String) {
            mainHandler.post {
                val origin = currentUrl?.let { Uri.parse(it) }?.let { originOf(it) }
                val accepted = if (requestedUrl == null && origin == null) true else isAllowedOrigin(origin)
                if (accepted && webView != null) emit("message", mapOf("data" to data))
            }
        }
    }
}
