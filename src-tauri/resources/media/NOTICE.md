# Media components

FFmpeg 8.1.2 and x264 r3222 (`b35605ace3ddf7c1a5d67a2eb553f034aef41d55`) provide local video processing. This FFmpeg configuration uses GPL-2.0-or-later components. The corresponding FFmpeg and x264 sources, FFmpeg license, build configuration and binary SHA-256 manifest are included in this directory. The application does not contact these projects at runtime.

Upstream: https://ffmpeg.org/ and https://www.videolan.org/developers/x264.html.

macOS build recipe: `scripts/build-ffmpeg-macos.sh` and `scripts/static-x264-pkg-config.sh` in the matching application source tree. x264 source/build information is included with the package. Windows release artifacts must provide equivalent matching build and source materials; a macOS binary is never reused on Windows.

This application is a locally built development distribution until the platform release validation in `docs/VALIDATION.md` has passed. Code signing, notarization and clean-install checks are separate from compilation.
