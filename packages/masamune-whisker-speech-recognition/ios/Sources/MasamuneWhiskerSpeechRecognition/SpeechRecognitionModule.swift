// iOS stub of masamune-whisker-speech-recognition. The recognizer is not implemented on iOS yet:
// `availability` reports it as unavailable and `start` rejects with "unsupported".

import Foundation
import WhiskerModule

@WhiskerModule
public final class SpeechRecognitionModule: Module {
    public override func definition() -> ModuleDefinition {
        ModuleDefinition {
            Name("SpeechRecognition")
            Events("speech")

            Function("availability") { _ in
                Bridge.ok(["available": false, "onDevice": false, "externalAudio": false])
            }

            AsyncFunction("start") { _, promise in
                Bridge.reject(promise, "unsupported: speech recognition is not implemented on iOS")
            }

            Function("feed") { _ in Bridge.ok(true) }

            Function("stop") { _ in Bridge.ok(true) }
        }
    }
}
