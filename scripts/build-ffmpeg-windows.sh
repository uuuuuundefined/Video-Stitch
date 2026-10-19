#!/bin/sh
# 在 Windows MSYS2 MINGW64 中从固定源码构建只依赖系统 DLL 的软件编码基础组件。
set -eu
test "${MSYSTEM:-}" = MINGW64 || { printf '%s\n' 'Run this recipe in Windows MSYS2 MINGW64.' >&2; exit 1; }
test "$(gcc -dumpmachine)" = x86_64-w64-mingw32
root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
archive="$root/.tools/media-source/ffmpeg-8.1.2.tar.xz"
x264_archive="$root/.tools/media-source/x264-b35605a.tar.gz"
test "$(sha256sum "$archive" | cut -d ' ' -f 1)" = 464beb5e7bf0c311e68b45ae2f04e9cc2af88851abb4082231742a74d97b524c
test "$(sha256sum "$x264_archive" | cut -d ' ' -f 1)" = cd71a7515b0e9a012e1ac9b1f8415bebcaf6fc97d4db32286642ac4c0fbe24f9
build=$(mktemp -d /tmp/video-stitch-ffmpeg.XXXXXX)
trap 'rm -rf "$build"' EXIT HUP INT TERM
tar -xf "$x264_archive" -C "$build"
cd "$build/x264-b35605ace3ddf7c1a5d67a2eb553f034aef41d55"
./configure --prefix="$build/x264-install" --host=x86_64-w64-mingw32 \
  --enable-static --disable-cli --disable-opencl --disable-avs --disable-lavf \
  --disable-swscale --disable-ffms --disable-lsmash
make -j "${VIDEO_STITCH_BUILD_JOBS:-4}" install-lib-static install-lib-dev
export PKG_CONFIG_LIBDIR="$build/x264-install/lib/pkgconfig"
export PKG_CONFIG_PATH=
tar -xf "$archive" -C "$build"
cd "$build/ffmpeg-8.1.2"
./configure --prefix="$build/install" --target-os=mingw32 --arch=x86_64 \
  --disable-autodetect --disable-everything --disable-shared --enable-static \
  --disable-debug --disable-doc --disable-ffplay --enable-ffmpeg --enable-ffprobe \
  --disable-network --enable-w32threads --enable-gpl --enable-libx264 \
  --enable-avcodec --enable-avformat --enable-avfilter --enable-avdevice \
  --enable-swscale --enable-swresample --pkg-config=pkg-config --pkg-config-flags=--static \
  --extra-cflags=-D_WIN32_WINNT=0x0A00 --extra-ldflags='-static -static-libgcc' \
  --enable-protocol=file,pipe --enable-indev=lavfi --enable-demuxer=mov,concat \
  --enable-muxer=mp4,null,rawvideo,framehash \
  --enable-decoder=h264,hevc,aac,mp3,mp3float,pcm_s16le,pcm_s24le,pcm_s32le,pcm_f32le,pcm_f64le,rawvideo,wrapped_avframe \
  --enable-parser=h264,hevc,aac,mpegaudio \
  --enable-encoder=libx264,aac,pcm_s16le,rawvideo,wrapped_avframe \
  --enable-bsf=h264_mp4toannexb,hevc_mp4toannexb,aac_adtstoasc,extract_extradata,null,h264_metadata \
  --enable-filter=scale,crop,fps,setpts,setsar,settb,format,tpad,trim,xfade,concat,atrim,asetpts,aresample,aformat,adelay,apad,anullsrc,testsrc2,sine,split,asplit,anull,null,setparams
make -j "${VIDEO_STITCH_BUILD_JOBS:-4}" ffmpeg.exe ffprobe.exe
node "$root/scripts/prepare-media.mjs" \
  --target x86_64-pc-windows-msvc --ffmpeg "$build/ffmpeg-8.1.2/ffmpeg.exe" \
  --ffprobe "$build/ffmpeg-8.1.2/ffprobe.exe" \
  --sha256-ffmpeg "$(sha256sum ffmpeg.exe | cut -d ' ' -f 1)" \
  --sha256-ffprobe "$(sha256sum ffprobe.exe | cut -d ' ' -f 1)"
resources="$root/src-tauri/resources/media"
cp COPYING.GPLv2 "$resources/FFmpeg-COPYING.GPLv2"
cp config.h "$resources/ffmpeg-config.h"
cp "$archive" "$x264_archive" "$resources/"
cp "$build/x264-b35605ace3ddf7c1a5d67a2eb553f034aef41d55/COPYING" "$resources/x264-COPYING"
cp "$root/scripts/build-ffmpeg-windows.sh" "$resources/"
gcc --version > "$resources/windows-compiler.txt"
sha256sum "$build/x264-install/lib/libx264.a" > "$resources/x264-static.sha256"
cd "$root"
node scripts/check-release.mjs --target x86_64-pc-windows-msvc
