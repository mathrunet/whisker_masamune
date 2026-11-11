package net.mathru.whisker.share_receive

/** External streams must be mediated by a ContentProvider and its URI grants. */
internal fun acceptsSharedStream(scheme: String?): Boolean = scheme == "content"
