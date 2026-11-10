package net.mathru.whisker.admob

/** Manages native View reattachment and ad load generations and retries. */
internal class BannerLifecycle {
    var adUnitId = ""
        private set
    var widthDp = 0
        private set
    private var attached = false
    private var generation = 0L
    private var retryCount = 0
    private var loading = false
    private var hasAd = false

    fun configure(adUnitId: String = this.adUnitId, widthDp: Int = this.widthDp): Boolean {
        if (this.adUnitId == adUnitId && this.widthDp == widthDp) return false
        this.adUnitId = adUnitId
        this.widthDp = widthDp
        invalidate()
        return true
    }

    fun attach() {
        attached = true
        invalidate()
    }

    fun detach() {
        attached = false
        invalidate()
    }

    private fun invalidate() {
        generation++
        retryCount = 0
        loading = false
        hasAd = false
    }

    fun beginLoad(): Long? {
        if (!attached || adUnitId.isBlank() || widthDp <= 0 || loading) return null
        loading = true
        hasAd = false
        return ++generation
    }

    fun isCurrent(token: Long): Boolean = attached && generation == token

    fun loaded(token: Long): Boolean {
        if (!isCurrent(token) || !loading) return false
        loading = false
        hasAd = true
        retryCount = 0
        return true
    }

    fun failed(token: Long): Long? {
        if (!isCurrent(token) || (!loading && !hasAd)) return null
        loading = false
        hasAd = false
        if (retryCount >= 3) return null
        return 5_000L shl retryCount++
    }
}
