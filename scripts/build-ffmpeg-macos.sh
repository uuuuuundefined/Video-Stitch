#!/bin/sh
# 从固定 FFmpeg 源码及已审计静态 x264 构建只依赖 macOS 系统库的组件。
set -eu
export MACOSX_DEPLOYMENT_TARGET=13.0
root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
archive="$root/.tools/media-source/ffmpeg-8.1.2.tar.xz"
expected=464beb5e7bf0c311e68b45ae2f04e9cc2af88851abb4082231742a74d97b524c
actual=$(shasum -a 256 "$archive" | cut -d ' ' -f 1)
test "$actual" = "$expected"
test "$(uname -m)" = arm64
build=$(mktemp -d "${TMPDIR:-/private/tmp}/video-stitch-ffmpeg.XXXXXX")
trap 'rm -rf "$build"' EXIT HUP INT TERM
x264_archive="$root/.tools/media-source/x264-b35605a.tar.gz"
test "$(shasum -a 256 "$x264_archive" | cut -d ' ' -f 1)" = cd71a7515b0e9a012e1ac9b1f8415bebcaf6fc97d4db32286642ac4c0fbe24f9
tar -xf "$x264_archive" -C "$build"
export VIDEO_STITCH_X264_PREFIX="$build/x264-install"
cd "$build/x264-b35605ace3ddf7c1a5d67a2eb553f034aef41d55"
CC=clang CFLAGS=-mmacosx-version-min=13.0 LDFLAGS=-mmacosx-version-min=13.0 \
  ./configure --prefix="$VIDEO_STITCH_X264_PREFIX" --enable-static --disable-cli --disable-opencl \
    --disable-avs --disable-lavf --disable-swscale --disable-ffms --disable-lsmash
make -j "${VIDEO_STITCH_BUILD_JOBS:-4}" install-lib-static install-lib-dev
tar -xf "$archive" -C "$build"
cp "$root/scripts/static-x264-pkg-config.sh" "$build/pkg-config"
cd "$build/ffmpeg-8.1.2"
./configure --prefix="$build/install" --disable-autodetect --disable-everything \
  --disable-shared --enable-static --disable-debug --disable-doc --disable-ffplay \
  --enable-ffmpeg --enable-ffprobe --disable-network --enable-pthreads \
  --enable-avcodec --enable-avformat --enable-avfilter --enable-avdevice --enable-swscale --enable-swresample \
  --enable-gpl --enable-libx264 --enable-videotoolbox --enable-audiotoolbox \
  --pkg-config="$build/pkg-config" \
  --extra-cflags=-mmacosx-version-min=13.0 --extra-ldflags=-mmacosx-version-min=13.0 \
  --enable-protocol=file,pipe --enable-indev=lavfi --enable-demuxer=mov,concat \
  --enable-muxer=mp4,null,rawvideo,framehash \
  --enable-decoder=h264,hevc,aac,mp3,mp3float,pcm_s16le,pcm_s24le,pcm_s32le,pcm_f32le,pcm_f64le,rawvideo,wrapped_avframe \
  --enable-parser=h264,hevc,aac,mpegaudio \
  --enable-encoder=libx264,h264_videotoolbox,aac,pcm_s16le,rawvideo,wrapped_avframe \
  --enable-hwaccel=h264_videotoolbox,hevc_videotoolbox \
  --enable-bsf=h264_mp4toannexb,hevc_mp4toannexb,aac_adtstoasc,extract_extradata,null,h264_metadata \
  --enable-filter=scale,crop,fps,setpts,setsar,settb,format,tpad,trim,xfade,concat,atrim,asetpts,aresample,aformat,adelay,apad,anullsrc,testsrc2,sine,split,asplit,anull,null,setparams
make -j "${VIDEO_STITCH_BUILD_JOBS:-4}" ffmpeg ffprobe
for name in ffmpeg ffprobe; do
  if otool -L "$name" | tail -n +2 | awk '{print $1}' | grep -vE '^(/usr/lib/|/System/Library/)' ; then
    printf '%s\n' 'Unexpected non-system dynamic dependency' >&2
    exit 1
  fi
done
mkdir -p "$root/src-tauri/binaries" "$root/src-tauri/resources/media"
cp ffmpeg "$root/src-tauri/binaries/ffmpeg-aarch64-apple-darwin"
cp ffprobe "$root/src-tauri/binaries/ffprobe-aarch64-apple-darwin"
cp COPYING.GPLv2 "$root/src-tauri/resources/media/FFmpeg-COPYING.GPLv2"
cp config.h "$root/src-tauri/resources/media/ffmpeg-config.h"
cp "$archive" "$root/src-tauri/resources/media/ffmpeg-8.1.2.tar.xz"
cp "$root/.tools/media-source/x264-b35605a.tar.gz" "$root/src-tauri/resources/media/"
cp "$build/x264-b35605ace3ddf7c1a5d67a2eb553f034aef41d55/COPYING" "$root/src-tauri/resources/media/x264-COPYING"
cp "$root/scripts/build-ffmpeg-macos.sh" "$root/src-tauri/resources/media/build-ffmpeg-macos.sh"
cp "$root/scripts/static-x264-pkg-config.sh" "$root/src-tauri/resources/media/static-x264-pkg-config.sh"
if test -f "$root/src-tauri/resources/media/x264-build.rb"; then rm "$root/src-tauri/resources/media/x264-build.rb"; fi
shasum -a 256 "$VIDEO_STITCH_X264_PREFIX/lib/libx264.a" > "$root/src-tauri/resources/media/x264-static.sha256"
node "$root/scripts/prepare-media.mjs" \
  --ffmpeg "$build/ffmpeg-8.1.2/ffmpeg" --ffprobe "$build/ffmpeg-8.1.2/ffprobe" \
  --target aarch64-apple-darwin \
  --sha256-ffmpeg "$(shasum -a 256 ffmpeg | cut -d ' ' -f 1)" \
  --sha256-ffprobe "$(shasum -a 256 ffprobe | cut -d ' ' -f 1)"
cp "$root/src-tauri/binaries/manifest.json" "$root/src-tauri/resources/media/manifest.json"
