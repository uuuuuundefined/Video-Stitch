#!/bin/sh
# 仅向 FFmpeg 提供指定静态 x264 的编译参数，不搜索或安装全局包。
set -eu
: "${VIDEO_STITCH_X264_PREFIX:?Set VIDEO_STITCH_X264_PREFIX}"
test -f "$VIDEO_STITCH_X264_PREFIX/lib/libx264.a"
case " $* " in
  *" --version "*) printf '%s\n' '1.0.0' ;;
  *" --cflags "*) printf '%s\n' "-I$VIDEO_STITCH_X264_PREFIX/include" ;;
  *" --libs "*) printf '%s\n' "$VIDEO_STITCH_X264_PREFIX/lib/libx264.a -lpthread -lm" ;;
  *" --variable=includedir "*) printf '%s\n' "$VIDEO_STITCH_X264_PREFIX/include" ;;
  *" --modversion "*) printf '%s\n' '0.165.3222' ;;
  *" x264 "*) exit 0 ;;
  *) exit 1 ;;
esac
