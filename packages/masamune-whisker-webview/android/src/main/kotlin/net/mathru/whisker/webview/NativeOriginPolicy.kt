package net.mathru.whisker.webview

/** Native privileges are opt-in, and never granted to an iframe or opaque origin. */
internal object NativeOriginPolicy {
    fun allows(origin: String?, allowedOrigins: List<String>, isMainFrame: Boolean = true): Boolean =
        isMainFrame && origin != null && allowedOrigins.contains(origin)
}
