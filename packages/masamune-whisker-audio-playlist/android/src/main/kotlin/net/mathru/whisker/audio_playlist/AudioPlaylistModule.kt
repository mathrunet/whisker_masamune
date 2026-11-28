// Gapless queue playback (Android) with announcement interrupts, on Media3 ExoPlayer.
//
// - The queue is one ExoPlayer playlist. `entries` mirrors it, current item first: when ExoPlayer
//   moves on by itself (MEDIA_ITEM_TRANSITION_REASON_AUTO, gapless), the finished items are
//   removed from the playlist so the current item stays at index 0.
// - Announcements play once on a second ExoPlayer. In pause mode the queue pauses (keeping its
//   position) and resumes afterwards when the playlist is still playing; in duck mode its volume
//   is lowered meanwhile.
// - Neither player handles audio focus: masamune-whisker-audio-session owns focus, interruptions
//   and the mediaPlayback foreground service that keeps the process alive in the background.
// - Output deadlines and output events use SystemClock.elapsedRealtimeNanos (CLOCK_BOOTTIME).
//   While something plays, a monitor runs every MONITOR_INTERVAL_NS: it ends items and
//   announcements early enough that no frame is presented at or after their deadline
//   (OUTPUT_LATENCY_NS and two monitor periods are kept as a margin) and reports when audio starts
//   reaching the output (the playback position advances) and when it stops.
// - Everything runs on the main thread (ExoPlayer's application looper).

package net.mathru.whisker.audio_playlist

import android.net.Uri
import android.os.Handler
import android.os.Looper
import android.os.SystemClock
import androidx.media3.common.AudioAttributes
import androidx.media3.common.C
import androidx.media3.common.MediaItem
import androidx.media3.common.PlaybackException
import androidx.media3.common.Player
import androidx.media3.exoplayer.ExoPlayer
import org.json.JSONObject
import rs.whisker.runtime.Module
import rs.whisker.runtime.ModuleDefinition
import rs.whisker.runtime.WhiskerApplication
import rs.whisker.runtime.WhiskerModule
import java.io.File

@WhiskerModule
class AudioPlaylistModule : Module() {
    private class Announcement(
        val id: String,
        val duck: Boolean,
        val duckVolume: Float,
        val player: ExoPlayer,
        var deadline: Long?,
    )

    /** Output of the queue or the announcement: whether [id] reaches the output. */
    private class Output(val id: String, var silentAt: Long, var silentPosition: Long) {
        var audible = false
    }

    private val deadlines = mutableMapOf<String, Long>()
    private val handler = Handler(Looper.getMainLooper())
    private var monitoring = false
    private var queueOutput: Output? = null
    private var announcementOutput: Output? = null
    private val monitorTask = object : Runnable {
        override fun run() {
            if (!monitoring) return
            tick()
            if (monitoring) handler.postDelayed(this, MONITOR_INTERVAL_NS / 1_000_000)
        }
    }

    private var queuePlayer: ExoPlayer? = null
    private val entries = mutableListOf<String>()
    private var playing = false
    private var volume = 1f
    private var announcement: Announcement? = null

    override fun definition() = ModuleDefinition {
        Name("AudioPlaylist")
        Events("event")

        // setQueue({ items: [{ id, uri }] })
        Function("setQueue") { args ->
            call {
                val json = Bridge.args(args)
                val items = items(json)
                checkUnique(items.map { it.mediaId }, emptyList())
                deadlines.clear()
                deadlines.putAll(itemDeadlines(json))
                setQueue(items)
                true
            }
        }

        // append({ items: [{ id, uri }] })
        Function("append") { args ->
            call {
                val json = Bridge.args(args)
                val items = items(json)
                checkUnique(items.map { it.mediaId }, entries)
                deadlines.putAll(itemDeadlines(json))
                append(items)
                true
            }
        }

        // remove({ id }) -> whether the item was queued
        Function("remove") { args -> call { remove(Bridge.args(args).optString("id")) } }

        Function("play") { _ ->
            call {
                playing = true
                applyPlayback()
                true
            }
        }

        Function("pause") { _ ->
            call {
                playing = false
                applyPlayback()
                true
            }
        }

        Function("stop") { _ ->
            call {
                stop()
                true
            }
        }

        // skipToNext() -> whether a next item exists
        Function("skipToNext") { _ ->
            call {
                if (entries.isEmpty()) {
                    false
                } else {
                    endCurrent("skipped")
                    entries.isNotEmpty()
                }
            }
        }

        // setVolume({ volume })
        Function("setVolume") { args ->
            call {
                val value = Bridge.args(args).optDouble("volume", Double.NaN)
                require(value.isFinite()) { "invalid volume" }
                volume = value.toFloat().coerceIn(0f, 1f)
                applyVolume()
                true
            }
        }

        // status() -> { currentId, positionMs, durationMs, playing, interrupting, queue, volume }
        Function("status") { _ -> call { status() } }

        // interrupt({ id, uri, mode, duckVolume, volume })
        Function("interrupt") { args ->
            call {
                val json = Bridge.args(args)
                val id = json.optString("id")
                require(id.isNotEmpty()) { "invalid announcement" }
                interrupt(
                    id = id,
                    uri = uri(json.optString("uri")),
                    duck = json.optString("mode") == "duck",
                    duckVolume = json.optDouble("duckVolume", 0.2).toFloat().coerceIn(0f, 1f),
                    volume = json.optDouble("volume", 1.0).toFloat().coerceIn(0f, 1f),
                    deadline = if (json.has("outputBeforeNs")) json.getLong("outputBeforeNs") else null,
                )
                true
            }
        }

        // setOutputDeadline({ id, outputBeforeNs }) -> whether the id was queued or announcing
        Function("setOutputDeadline") { args ->
            call {
                val json = Bridge.args(args)
                require(json.has("outputBeforeNs")) { "invalid output deadline" }
                setOutputDeadline(json.optString("id"), json.getLong("outputBeforeNs"))
            }
        }

        // cancelInterrupt() -> whether an announcement was playing
        Function("cancelInterrupt") { _ -> call { endAnnouncement(completed = false) } }
    }

    // MARK: - Queue

    private fun setQueue(items: List<MediaItem>) {
        val player = player()
        entries.firstOrNull()?.let { emit("itemEnded", "id" to it, "reason" to "removed") }
        entries.clear()
        entries.addAll(items.map { it.mediaId })
        if (items.isEmpty()) {
            player.clearMediaItems()
        } else {
            player.setMediaItems(items, true)
            player.prepare()
            startCurrent()
        }
        applyPlayback()
    }

    private fun append(items: List<MediaItem>) {
        val player = player()
        val wasEmpty = entries.isEmpty()
        entries.addAll(items.map { it.mediaId })
        if (wasEmpty) {
            player.setMediaItems(items, true)
            player.prepare()
            startCurrent()
        } else {
            player.addMediaItems(items)
        }
        applyPlayback()
    }

    private fun remove(id: String): Boolean {
        val index = entries.indexOf(id)
        if (index < 0) return false
        if (index == 0) {
            endCurrent("removed")
        } else {
            entries.removeAt(index)
            deadlines.remove(id)
            player().removeMediaItem(index)
        }
        return true
    }

    private fun stop() {
        endAnnouncement(completed = false, resume = false)
        entries.firstOrNull()?.let { emit("itemEnded", "id" to it, "reason" to "stopped") }
        entries.clear()
        playing = false
        deadlines.clear()
        queuePlayer?.let {
            it.playWhenReady = false
            it.clearMediaItems()
        }
        updateOutputs()
        updateMonitor()
    }

    /** Ends the current item with [reason], drops it from the playlist and reports the next one. */
    private fun endCurrent(reason: String) {
        if (entries.isEmpty()) return
        val id = entries.removeAt(0)
        deadlines.remove(id)
        emit("itemEnded", "id" to id, "reason" to reason)
        updateOutputs()
        val player = player()
        if (entries.isEmpty()) {
            player.clearMediaItems()
        } else {
            // The next item becomes current at its start, keeping playWhenReady.
            player.removeMediaItem(0)
            if (player.playbackState == Player.STATE_IDLE) player.prepare()
        }
        startNext()
    }

    /** Reports the new current item, or that the queue ran dry. */
    private fun startNext() {
        if (entries.isEmpty()) {
            emit("queueDrained")
        } else {
            startCurrent()
            applyPlayback()
        }
    }

    private fun startCurrent() {
        val id = entries.firstOrNull() ?: return
        emit("itemStarted", "id" to id, "remaining" to entries.size - 1)
    }

    private val queueListener = object : Player.Listener {
        override fun onMediaItemTransition(mediaItem: MediaItem?, reason: Int) {
            if (reason != Player.MEDIA_ITEM_TRANSITION_REASON_AUTO) return
            val player = queuePlayer ?: return
            // ExoPlayer moved on by itself: everything before the new item has finished.
            val finished = player.currentMediaItemIndex.coerceAtMost(entries.size)
            repeat(finished) {
                val id = entries.removeAt(0)
                deadlines.remove(id)
                emit("itemEnded", "id" to id, "reason" to "completed")
            }
            updateOutputs()
            if (finished > 0) player.removeMediaItems(0, finished)
            startCurrent()
        }

        override fun onPlaybackStateChanged(state: Int) {
            if (state != Player.STATE_ENDED || entries.isEmpty()) return
            // The last item finished.
            val ids = entries.toList()
            entries.clear()
            deadlines.clear()
            ids.forEach { emit("itemEnded", "id" to it, "reason" to "completed") }
            queuePlayer?.clearMediaItems()
            updateOutputs()
            emit("queueDrained")
        }

        override fun onPlayerError(error: PlaybackException) {
            val player = queuePlayer ?: return
            val id = player.currentMediaItem?.mediaId
            val index = id?.let { entries.indexOf(it) } ?: -1
            emit("error", "id" to id, "message" to error.errorCodeName)
            if (index < 0) return
            // Items before the failed one finished.
            repeat(index) {
                emit("itemEnded", "id" to entries.removeAt(0), "reason" to "completed")
            }
            if (index > 0) player.removeMediaItems(0, index)
            endCurrent("failed")
        }
    }

    private fun player(): ExoPlayer {
        queuePlayer?.let { return it }
        val context = WhiskerApplication.appContext ?: throw IllegalStateException("no context")
        val player = ExoPlayer.Builder(context)
            .setAudioAttributes(
                AudioAttributes.Builder()
                    .setUsage(C.USAGE_MEDIA)
                    .setContentType(C.AUDIO_CONTENT_TYPE_MUSIC)
                    .build(),
                false,
            )
            .setWakeMode(C.WAKE_MODE_NETWORK)
            .build()
        player.addListener(queueListener)
        player.volume = volume
        queuePlayer = player
        return player
    }

    /** Plays the queue when the playlist is playing and no pausing announcement runs. */
    private fun applyPlayback() {
        enforceDeadlines()
        val pausedByAnnouncement = announcement?.let { !it.duck } ?: false
        queuePlayer?.playWhenReady = playing && !pausedByAnnouncement && entries.isNotEmpty()
        updateMonitor()
    }

    private fun applyVolume() {
        val duck = announcement?.takeIf { it.duck }?.duckVolume ?: 1f
        queuePlayer?.volume = volume * duck
    }

    private fun status(): JSONObject {
        val player = queuePlayer
        val current = entries.firstOrNull()
        val duration = player?.duration?.takeIf { current != null && it != C.TIME_UNSET }
        return JSONObject()
            .put("currentId", current ?: JSONObject.NULL)
            .put("positionMs", if (current != null && player != null) player.currentPosition.coerceAtLeast(0) else 0)
            .put("durationMs", duration ?: JSONObject.NULL)
            .put("playing", playing)
            .put("interrupting", announcement != null)
            .put("queue", Bridge.wrap(entries))
            .put("volume", volume.toDouble())
    }

    // MARK: - Announcements

    private fun interrupt(id: String, uri: Uri, duck: Boolean, duckVolume: Float, volume: Float, deadline: Long?) {
        endAnnouncement(completed = false, resume = false)
        val context = WhiskerApplication.appContext ?: throw IllegalStateException("no context")
        val player = ExoPlayer.Builder(context)
            .setAudioAttributes(
                AudioAttributes.Builder()
                    .setUsage(C.USAGE_MEDIA)
                    .setContentType(C.AUDIO_CONTENT_TYPE_SPEECH)
                    .build(),
                false,
            )
            .setWakeMode(C.WAKE_MODE_NETWORK)
            .build()
        val next = Announcement(id, duck, duckVolume, player, deadline)
        player.addListener(object : Player.Listener {
            override fun onPlaybackStateChanged(state: Int) {
                if (state == Player.STATE_ENDED && announcement === next) endAnnouncement(completed = true)
            }

            override fun onPlayerError(error: PlaybackException) {
                if (announcement !== next) return
                emit("error", "id" to id, "message" to error.errorCodeName)
                endAnnouncement(completed = false)
            }
        })
        player.volume = volume
        player.setMediaItem(MediaItem.fromUri(uri))
        announcement = next
        applyVolume()
        applyPlayback()
        emit("interruptStarted", "id" to id)
        if (deadline != null && now() + GUARD_NS >= deadline) {
            // Too close to its deadline to be presented in full: it never starts.
            endAnnouncement(completed = false)
            return
        }
        player.prepare()
        player.playWhenReady = true
        updateMonitor()
    }

    /** Stops the announcement and, with [resume], restores the queue. Returns whether one ran. */
    private fun endAnnouncement(completed: Boolean, resume: Boolean = true): Boolean {
        val current = announcement ?: return false
        announcement = null
        current.player.release()
        applyVolume()
        if (resume) applyPlayback()
        updateOutputs()
        emit("interruptEnded", "id" to current.id, "completed" to completed)
        updateMonitor()
        return true
    }

    // MARK: - Output deadlines and output events

    private fun setOutputDeadline(id: String, deadline: Long): Boolean {
        val current = announcement
        if (current != null && current.id == id) {
            current.deadline = current.deadline?.let { minOf(it, deadline) } ?: deadline
        } else if (id in entries) {
            deadlines[id] = deadlines[id]?.let { minOf(it, deadline) } ?: deadline
        } else {
            return false
        }
        tick()
        return true
    }

    /** Runs the monitor while something plays or could start playing. */
    private fun updateMonitor() {
        val needed = announcement != null || (playing && entries.isNotEmpty()) ||
            queueOutput?.audible == true || announcementOutput?.audible == true
        if (needed && !monitoring) {
            monitoring = true
            handler.post(monitorTask)
        } else if (!needed && monitoring) {
            monitoring = false
            handler.removeCallbacks(monitorTask)
        }
    }

    private fun tick() {
        enforceDeadlines()
        updateOutputs()
        updateMonitor()
    }

    private fun enforceDeadlines() {
        val limit = now() + GUARD_NS
        announcement?.deadline?.let { if (limit >= it) endAnnouncement(completed = false) }
        var index = 1
        while (index < entries.size) {
            val id = entries[index]
            val deadline = deadlines[id]
            if (deadline != null && limit >= deadline) {
                entries.removeAt(index)
                deadlines.remove(id)
                player().removeMediaItem(index)
                emit("itemEnded", "id" to id, "reason" to "expired")
            } else {
                index++
            }
        }
        while (true) {
            val deadline = entries.firstOrNull()?.let { deadlines[it] } ?: break
            if (limit < deadline) break
            endCurrent("expired")
        }
    }

    /** Reports `outputStarted` once the position of an audible item advances and `outputEnded` as soon as it stops. */
    private fun updateOutputs() {
        val now = now()
        val pausedByAnnouncement = announcement?.let { !it.duck } ?: false
        val queueTarget = if (playing && !pausedByAnnouncement) entries.firstOrNull() else null
        val queue = queuePlayer
        queueOutput = update(
            queueOutput,
            queueTarget,
            queue?.currentPosition ?: 0,
            queue != null && queue.isPlaying && queue.currentMediaItem?.mediaId == queueTarget,
            now,
        )
        val current = announcement
        announcementOutput = update(
            announcementOutput,
            current?.id,
            current?.player?.currentPosition ?: 0,
            current?.player?.isPlaying == true,
            now,
        )
    }

    private fun update(output: Output?, target: String?, position: Long, running: Boolean, now: Long): Output? {
        var next = output
        if (next != null && (next.id != target || (next.audible && !running))) {
            if (next.audible) emit("outputEnded", "id" to next.id, "lastFrameNs" to now + OUTPUT_LATENCY_NS)
            next = null
        }
        if (target == null) return null
        val tracked = next ?: Output(target, now, position)
        if (!tracked.audible) {
            if (running && position > tracked.silentPosition) {
                // Frames rendered after `silentAt` reach the output OUTPUT_LATENCY_NS later.
                val earliest = tracked.silentAt
                val latest = now + OUTPUT_LATENCY_NS
                val played = (position - tracked.silentPosition) * 1_000_000
                val estimate = (latest - played).coerceIn(earliest, latest)
                emit(
                    "outputStarted",
                    "id" to target,
                    "firstFrameNs" to estimate,
                    "earliestNs" to earliest,
                    "latestNs" to latest,
                )
                tracked.audible = true
            } else {
                tracked.silentAt = now
                tracked.silentPosition = position
            }
        }
        return tracked
    }

    private fun now(): Long = SystemClock.elapsedRealtimeNanos()

    // MARK: - Helpers

    private fun emit(type: String, vararg fields: Pair<String, Any?>) {
        Bridge.onMain { sendEvent("event", Bridge.event(mapOf("type" to type, *fields))) }
    }

    private inline fun call(block: () -> Any?) = try {
        check(Looper.myLooper() == Looper.getMainLooper()) { "AudioPlaylist must be called on the main thread" }
        Bridge.ok(block())
    } catch (error: Throwable) {
        Bridge.err(error)
    }

    private fun items(json: JSONObject): List<MediaItem> {
        val raw = json.optJSONArray("items") ?: throw IllegalArgumentException("items are missing")
        return (0 until raw.length()).map { index ->
            val item = raw.getJSONObject(index)
            val id = item.optString("id")
            require(id.isNotEmpty()) { "invalid queue item" }
            MediaItem.Builder().setMediaId(id).setUri(uri(item.optString("uri"))).build()
        }
    }

    private fun itemDeadlines(json: JSONObject): Map<String, Long> {
        val raw = json.optJSONArray("items") ?: return emptyMap()
        return (0 until raw.length()).mapNotNull { index ->
            val item = raw.getJSONObject(index)
            if (item.has("outputBeforeNs")) item.optString("id") to item.getLong("outputBeforeNs") else null
        }.toMap()
    }

    private fun checkUnique(ids: List<String>, existing: List<String>) {
        val seen = existing.toMutableSet()
        ids.forEach { require(seen.add(it)) { "duplicate queue item id: $it" } }
    }

    /** `https://` URLs, `file://` URLs and absolute paths (validated in Rust as well). */
    private fun uri(value: String): Uri {
        if (value.startsWith("/")) return Uri.fromFile(File(value))
        val uri = Uri.parse(value)
        require(uri.scheme?.lowercase() in setOf("https", "file")) { "invalid audio source" }
        return uri
    }

    private companion object {
        const val MONITOR_INTERVAL_NS = 40_000_000L

        /** Conservative bound for the audio output latency (Media3 does not expose it). */
        const val OUTPUT_LATENCY_NS = 150_000_000L
        const val GUARD_NS = OUTPUT_LATENCY_NS + 2 * MONITOR_INTERVAL_NS + 20_000_000L
    }
}
