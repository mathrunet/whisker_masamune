package net.mathru.whisker.view_capture

import java.io.File

/** Resolves a single safe filename inside the dedicated capture cache. */
internal fun captureFile(cache: File, fileName: String): File {
    require(fileName.matches(Regex("[A-Za-z0-9][A-Za-z0-9._-]{0,127}")) && !fileName.contains("..")) {
        "invalid capture file name"
    }
    val directory = File(cache, "masamune_whisker_view_capture")
    val file = File(directory, fileName)
    // A dangling symlink is listed by its parent but File.exists() returns false.
    require(file.exists() || directory.list()?.contains(fileName) != true) { "invalid capture symlink" }
    require(directory.canonicalFile == File(cache.canonicalFile, "masamune_whisker_view_capture")) {
        "invalid capture directory"
    }
    require(file.canonicalFile.parentFile == directory.canonicalFile) { "invalid capture path" }
    return file
}
