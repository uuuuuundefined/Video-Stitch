# Media binaries

Release builds require matching, self-contained `ffmpeg-<target>` and `ffprobe-<target>` executables (`.exe` on Windows). Supported targets are `aarch64-apple-darwin` and `x86_64-pc-windows-msvc`.

`npm run media:prepare -- --dev` stages the local tools **for development only**. A Homebrew executable may depend on Homebrew libraries and cannot be shipped as a standalone release. `npm run media:prepare -- --ffmpeg /path --ffprobe /path --target <target> --sha256-ffmpeg <hash> --sha256-ffprobe <hash>` verifies supplied release artifacts and stages them. Keep matching build configuration, sources, notices and licenses with the release materials. Never silently download an unpinned third-party executable.
