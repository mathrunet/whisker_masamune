import java.io.File
import net.mathru.whisker.view_capture.captureFile
import net.mathru.whisker.webview.NativeOriginPolicy
import net.mathru.whisker.share_receive.acceptsSharedStream

fun main() {
    val cache = java.nio.file.Files.createTempDirectory("capture-policy").toFile()
    try {
        check(captureFile(cache, "capture-1.png").parentFile == File(cache, "masamune_whisker_view_capture"))
        for (name in listOf("", "../secret", "/tmp/secret", "x/y.png", "x\\y.png", "..", ".hidden", "x\u0000.png", "x\n.png", "a".repeat(129))) {
            check(runCatching { captureFile(cache, name) }.isFailure) { "accepted unsafe name" }
        }
        val directory = File(cache, "masamune_whisker_view_capture").apply { mkdirs() }
        java.nio.file.Files.createSymbolicLink(File(directory, "escape.png").toPath(), File(cache, "private.png").toPath())
        check(runCatching { captureFile(cache, "escape.png") }.isFailure)
        check(!acceptsSharedStream("file"))
        check(!acceptsSharedStream("https"))
        check(!acceptsSharedStream(null))
        check(acceptsSharedStream("content"))
        check(!NativeOriginPolicy.allows("https://a.test", emptyList()))
        val allowed = listOf("https://a.test")
        check(NativeOriginPolicy.allows("https://a.test", allowed))
        check(!NativeOriginPolicy.allows("https://a.test", allowed, false))
        check(!NativeOriginPolicy.allows("https://evil.test", allowed))
        check(!NativeOriginPolicy.allows(null, allowed))
        check(!NativeOriginPolicy.allows("null", allowed))
        println("Android security policies passed")
    } finally { cache.deleteRecursively() }
}
