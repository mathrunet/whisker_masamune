package net.mathru.whisker.view_capture

import android.annotation.TargetApi
import android.app.Activity
import android.content.Context
import android.content.ContextWrapper
import android.graphics.Bitmap
import android.graphics.Rect
import android.os.Handler
import android.os.Looper
import android.view.PixelCopy
import android.view.View
import android.view.ViewGroup
import android.view.ViewTreeObserver

/** Copies a visible subtree from the window compositor instead of redrawing WebView on a software canvas. */
@TargetApi(26)
internal fun captureComposited(target: View, bitmap: Bitmap, completed: (Throwable?) -> Unit) {
    var context: Context = target.context
    while (context is ContextWrapper && context !is Activity && context.baseContext !== context) {
        context = context.baseContext
    }
    val window = (context as? Activity)?.window
        ?: throw IllegalStateException("capture window is unavailable")
    val position = IntArray(2)
    target.getLocationInWindow(position)
    val source = Rect(position[0], position[1], position[0] + bitmap.width, position[1] + bitmap.height)
    val visible = Rect()
    val projected = Rect()
    check(target.isAttachedToWindow && target.getLocalVisibleRect(visible) &&
        visible.contains(0, 0, bitmap.width, bitmap.height) &&
        target.getGlobalVisibleRect(projected) && projected.width() == target.width &&
        projected.height() == target.height &&
        Rect(0, 0, window.decorView.width, window.decorView.height).contains(source)) {
        "composited capture host is not fully visible"
    }

    // A window copy includes sibling overlays, unlike View.draw(). Hide them without unmounting
    // their state, then restore them on every completion path (including a missing frame).
    val hidden = mutableListOf<View>()
    var branch = target
    while (branch !== window.decorView) {
        val parent = branch.parent as? ViewGroup ?: break
        for (index in 0 until parent.childCount) {
            val sibling = parent.getChildAt(index)
            if (sibling !== branch && sibling.visibility == View.VISIBLE) hidden.add(sibling)
        }
        branch = parent
    }
    val handler = Handler(Looper.getMainLooper())
    val observer = window.decorView.viewTreeObserver
    var finished = false
    var requested = false
    lateinit var listener: ViewTreeObserver.OnDrawListener
    lateinit var timeout: Runnable
    fun finish(error: Throwable?) {
        if (finished) return
        finished = true
        handler.removeCallbacks(timeout)
        if (observer.isAlive) observer.removeOnDrawListener(listener)
        hidden.forEach { if (it.visibility == View.INVISIBLE) it.visibility = View.VISIBLE }
        completed(error)
    }
    timeout = Runnable { finish(IllegalStateException("composited capture timed out")) }
    listener = ViewTreeObserver.OnDrawListener {
        if (!requested && !finished) {
            requested = true
            // Run after this draw has submitted the frame with the overlays hidden.
            handler.post {
                if (!finished) {
                    try {
                        check(target.isAttachedToWindow) { "capture host was detached" }
                        // PixelCopy owns the destination until its callback, and has its own timeout.
                        handler.removeCallbacks(timeout)
                        PixelCopy.request(window, source, bitmap, { result ->
                            finish(if (result == PixelCopy.SUCCESS) null else
                                IllegalStateException("composited capture failed ($result)"))
                        }, handler)
                    } catch (error: Exception) {
                        finish(error)
                    }
                }
            }
        }
    }
    try {
        hidden.forEach { it.visibility = View.INVISIBLE }
        observer.addOnDrawListener(listener)
        handler.postDelayed(timeout, 2_000)
        window.decorView.invalidate()
    } catch (error: Exception) {
        finish(error)
    }
}
