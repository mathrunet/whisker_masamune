import Foundation

/// Resolves a single safe filename inside the dedicated capture cache.
func captureFileURL(cache: URL, fileName: String) throws -> URL {
    let safe = !fileName.isEmpty && fileName.utf8.count <= 128 &&
        fileName.utf8.allSatisfy { byte in
            (65...90).contains(byte) || (97...122).contains(byte) || (48...57).contains(byte) ||
                byte == 46 || byte == 95 || byte == 45
        } && fileName.utf8.first.map { (65...90).contains($0) || (97...122).contains($0) || (48...57).contains($0) } == true &&
        !fileName.contains("..")
    let directory = cache.appendingPathComponent("masamune_whisker_view_capture", isDirectory: true)
    let file = directory.appendingPathComponent(fileName)
    guard safe,
          (try? FileManager.default.destinationOfSymbolicLink(atPath: file.path)) == nil,
          (try? FileManager.default.destinationOfSymbolicLink(atPath: directory.path)) == nil,
          directory.resolvingSymlinksInPath().standardizedFileURL.path ==
              cache.resolvingSymlinksInPath().appendingPathComponent("masamune_whisker_view_capture", isDirectory: true).standardizedFileURL.path,
          file.resolvingSymlinksInPath().deletingLastPathComponent().standardizedFileURL.path == directory.resolvingSymlinksInPath().standardizedFileURL.path else {
        throw NSError(domain: "MasamuneCapture", code: 1, userInfo: [NSLocalizedDescriptionKey: "invalid capture path"])
    }
    return file
}
