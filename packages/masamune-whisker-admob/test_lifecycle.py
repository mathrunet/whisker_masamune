"""Runs regression tests with the existing Kotlin compiler cache from the Android build, without adding dependencies."""

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
    root = Path(__file__).resolve().parent
    sources = [root / "android/src/main/kotlin/net/mathru/whisker/admob/BannerLifecycle.kt",
               root / "src/test/kotlin/net/mathru/whisker/admob/BannerLifecycleTest.kt"]
    with tempfile.TemporaryDirectory(prefix="whisker-admob-test-") as temporary:
        output = str(Path(temporary) / "lifecycle.jar")
        subprocess.run([java, "-cp", os.pathsep.join(jars), "org.jetbrains.kotlin.cli.jvm.K2JVMCompiler",
                        "-no-stdlib", "-no-reflect", "-classpath", os.pathsep.join(jars[1:]),
                        "-d", output, *map(str, sources)], check=True)
        subprocess.run([java, "-cp", output + os.pathsep + jars[1],
                        "net.mathru.whisker.admob.BannerLifecycleTestKt"], check=True)


if __name__ == "__main__":
    main()
