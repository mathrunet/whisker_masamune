"""Executes the actual native security policies using installed Swift and cached Kotlin; adds no dependencies."""

import os
from pathlib import Path
import shutil
import subprocess
import tempfile


def main():
    cache = Path(os.environ.get("GRADLE_USER_HOME", Path.home() / ".gradle")) / "caches/modules-2/files-2.1"
    coordinates = [
        ("org.jetbrains.kotlin", "kotlin-compiler-embeddable", "2.0.21"),
        ("org.jetbrains.kotlin", "kotlin-stdlib", "2.0.21"),
        ("org.jetbrains.kotlin", "kotlin-script-runtime", "2.0.21"),
        ("org.jetbrains.kotlin", "kotlin-reflect", "1.6.10"),
        ("org.jetbrains.kotlin", "kotlin-daemon-embeddable", "2.0.21"),
        ("org.jetbrains.intellij.deps", "trove4j", "1.0.20200330"),
        ("org.jetbrains.kotlinx", "kotlinx-coroutines-core-jvm", "1.6.4"),
        ("org.jetbrains", "annotations", "13.0"),
    ]
    jars = []
    for group, artifact, version in coordinates:
        matches = list((cache / group / artifact / version).glob("*/*.jar"))
        if not matches:
            raise SystemExit(f"Kotlin cache not found. Set up the existing Android build environment: {artifact}:{version}")
        jars.append(str(matches[0]))
    java_home = os.environ.get("JAVA_HOME")
    java = str(Path(java_home) / "bin/java") if java_home else shutil.which("java")
    if not java:
        raise SystemExit("Set JAVA_HOME to the JDK used by the existing Android build.")
    root = Path(__file__).resolve().parent.parent
    sources = [
        root / "packages/masamune-whisker-view-capture/android/src/main/kotlin/net/mathru/whisker/view_capture/CaptureFilePolicy.kt",
        root / "packages/masamune-whisker-webview/android/src/main/kotlin/net/mathru/whisker/webview/NativeOriginPolicy.kt",
        root / "packages/masamune-whisker-share-receive/android/src/main/kotlin/net/mathru/whisker/share_receive/SharedStreamPolicy.kt",
        root / "tests/security/NativeSecurityTest.kt",
    ]
    with tempfile.TemporaryDirectory(prefix="whisker-security-test-") as temporary:
        output = str(Path(temporary) / "security.jar")
        subprocess.run([java, "-cp", os.pathsep.join(jars), "org.jetbrains.kotlin.cli.jvm.K2JVMCompiler",
                        "-no-stdlib", "-no-reflect", "-classpath", os.pathsep.join(jars[1:]),
                        "-d", output, *map(str, sources)], check=True)
        subprocess.run([java, "-cp", output + os.pathsep + jars[1], "NativeSecurityTestKt"], check=True)
        serial = os.environ.get("SECURITY_ANDROID_SERIAL")
        if serial:
            sdk = Path(os.environ.get("ANDROID_HOME", Path.home() / "Library/Android/sdk"))
            d8 = sorted((sdk / "build-tools").glob("*/d8"))[-1]
            dex = Path(temporary) / "dex"
            dex.mkdir()
            subprocess.run([str(d8), "--min-api", "26", "--output", str(dex), output, jars[1]], check=True)
            remote_name = Path(temporary).name
            remote = "/data/local/tmp/" + remote_name
            adb = [str(sdk / "platform-tools/adb"), "-s", serial]
            subprocess.run([*adb, "shell", "mkdir", remote], check=True)
            try:
                subprocess.run([*adb, "push", str(dex / "classes.dex"), remote + "/classes.dex"], check=True)
                subprocess.run([*adb, "shell", f"CLASSPATH={remote}/classes.dex app_process -Djava.io.tmpdir={remote} /system/bin NativeSecurityTestKt"], check=True)
            finally:
                subprocess.run([*adb, "shell", f"cd /data/local/tmp && rm -r -- {remote_name}"], check=True)
        swift_output = str(Path(temporary) / "swift-security")
        subprocess.run(["xcrun", "swiftc",
                        str(root / "packages/masamune-whisker-view-capture/ios/Sources/MasamuneWhiskerViewCapture/CaptureFilePolicy.swift"),
                        str(root / "packages/masamune-whisker-webview/ios/Sources/MasamuneWhiskerWebview/NativeOriginPolicy.swift"),
                        str(root / "tests/security/main.swift"), "-o", swift_output], check=True)
        subprocess.run([swift_output], check=True)


if __name__ == "__main__":
    main()
