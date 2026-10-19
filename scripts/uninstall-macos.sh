#!/bin/sh
# 用户选择卸载时运行。只清理固定私有路径，绝不读取或扫描导出目录。
set -eu
if pgrep -x video-stitch >/dev/null 2>&1; then
  printf '%s\n' '请先退出 Video Stitch。' >&2
  exit 1
fi
/usr/bin/defaults delete com.videostitch.desktop >/dev/null 2>&1 || true
for directory in \
  "$HOME/Library/Application Support/com.videostitch.desktop" \
  "$HOME/Library/Caches/com.videostitch.desktop" \
  "$HOME/Library/WebKit/com.videostitch.desktop" \
  "$HOME/Library/Saved Application State/com.videostitch.desktop.savedState" \
  "$HOME/Library/Preferences/com.videostitch.desktop.plist" \
  "$HOME/Library/HTTPStorages/com.videostitch.desktop"; do
  if test -e "$directory" || test -L "$directory"; then rm -rf -- "$directory"; fi
done
printf '%s\n' '软件私有运行数据已清理。请将 Video Stitch.app 移入废纸篓；所有导出目录保持不变。'
