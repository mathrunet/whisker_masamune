"""Tests the production Android compositor capture on an explicitly selected emulator; installs no dependencies.

Usage: CAPTURE_ANDROID_SERIAL=emulator-5554 python3 scripts/test-android-capture.py
Requires the existing Android SDK/JDK, cached Kotlin 2.0.21, ffmpeg, and a debug keystore.
The disposable test app uses a local video and no application data or authentication.
Evidence is saved under target/android-capture/.
"""

import os
from pathlib import Path
import shutil
import subprocess
import zipfile


def main():
    root = Path(__file__).resolve().parent.parent
    sdk = Path(os.environ.get("ANDROID_HOME", Path.home() / "Library/Android/sdk"))
    serial = os.environ.get("CAPTURE_ANDROID_SERIAL", "")
    if not serial.startswith("emulator-"):
        raise SystemExit("Set CAPTURE_ANDROID_SERIAL to the explicit test emulator ID.")
    java_home = Path(os.environ.get("JAVA_HOME", "/Applications/Android Studio.app/Contents/jbr/Contents/Home"))
    tools = sdk / "build-tools/36.0.0"
    android = sdk / "platforms/android-36/android.jar"
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
            raise SystemExit(f"Existing Kotlin build cache is required: {artifact}:{version}")
        jars.append(str(matches[0]))
    ffmpeg = shutil.which("ffmpeg")
    if not ffmpeg:
        raise SystemExit("ffmpeg must already be installed to create the local video fixture.")
    output = root / "target/android-capture"
    for name in ("classes", "assets", "dex"):
        (output / name).mkdir(parents=True, exist_ok=True)
    fixture = root / "tests/capture"
    shutil.copyfile(fixture / "fixture.html", output / "assets/fixture.html")

    def run(*args, **kwargs):
        return subprocess.run(list(map(str, args)), check=True, **kwargs)

    run(ffmpeg, "-loglevel", "error", "-f", "lavfi", "-i", "color=c=lime:s=320x180:r=10",
        "-t", "4", "-c:v", "libx264", "-pix_fmt", "yuv420p", "-y", output / "assets/green.mp4")
    run(java_home / "bin/java", "-cp", os.pathsep.join(jars), "org.jetbrains.kotlin.cli.jvm.K2JVMCompiler",
        "-no-stdlib", "-no-reflect", "-jvm-target", "17", "-classpath", os.pathsep.join([str(android), *jars[1:]]),
        "-d", output / "capture.jar",
        root / "packages/masamune-whisker-view-capture/android/src/main/kotlin/net/mathru/whisker/view_capture/CompositedCapture.kt")
    run(java_home / "bin/javac", "--release", "17", "-classpath",
        os.pathsep.join([str(android), str(output / "capture.jar"), jars[1]]), "-d", output / "classes",
        *sorted(fixture.glob("*.java")))
    run(tools / "d8", "--lib", android, "--min-api", "26", "--output", output / "dex",
        output / "capture.jar", jars[1], *sorted((output / "classes").rglob("*.class")))
    run(tools / "aapt2", "link", "-I", android, "--manifest", fixture / "AndroidManifest.xml",
        "-A", output / "assets", "-o", output / "test.apk")
    with zipfile.ZipFile(output / "test.apk", "a") as apk:
        apk.write(output / "dex/classes.dex", "classes.dex")
    run(tools / "apksigner", "sign", "--ks", Path.home() / ".android/debug.keystore",
        "--ks-pass", "pass:android", "--out", output / "test-signed.apk", output / "test.apk")
    adb = [str(sdk / "platform-tools/adb"), "-s", serial]
    package = "net.mathru.whisker.capturetest"
    run(*adb, "install", "--no-incremental", "-r", output / "test-signed.apk")
    try:
        result = run(*adb, "shell", "am", "instrument", "-w", package + "/.CaptureTest",
                     capture_output=True, text=True, timeout=120)
        print(result.stdout)
        (output / "result.txt").write_text(result.stdout)
        if "INSTRUMENTATION_RESULT: result=PASS:" not in result.stdout:
            raise SystemExit("Native capture regression test failed. See target/android-capture/result.txt.")
        for name in ("software.png", "composited.png"):
            with (output / name).open("wb") as png:
                run(*adb, "exec-out", "run-as", package, "cat", "files/" + name, stdout=png)
        print(f"Evidence: {output}")
    finally:
        run(*adb, "uninstall", package)


if __name__ == "__main__":
    main()
