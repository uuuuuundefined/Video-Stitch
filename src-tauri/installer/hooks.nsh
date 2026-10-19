!macro NSIS_HOOK_PREUNINSTALL
  ; Tauri's optional app-data cleanup recursively removes the whole parent.
  ; Always use the owned-runtime hook below, even when that checkbox is selected.
  StrCpy $DeleteAppDataCheckboxState 0
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  ; Remove only the new owned runtime. Legacy directories and exports are preserved.
  Push $0
  Push $1
  Push $2
  System::Call 'kernel32::GetFileAttributesW(w "$LOCALAPPDATA\com.videostitch.desktop\Runtime-v2") i.r2'
  IntOp $2 $2 & 0x400
  IntCmp $2 0 vs_runtime_check_marker vs_runtime_cleanup_done vs_runtime_cleanup_done
  vs_runtime_check_marker:
  System::Call 'kernel32::GetFileAttributesW(w "$LOCALAPPDATA\com.videostitch.desktop\Runtime-v2\.video-stitch-owner") i.r2'
  IntOp $2 $2 & 0x400
  IntCmp $2 0 vs_runtime_read_marker vs_runtime_cleanup_done vs_runtime_cleanup_done
  vs_runtime_read_marker:
  ClearErrors
  FileOpen $0 "$LOCALAPPDATA\com.videostitch.desktop\Runtime-v2\.video-stitch-owner" r
  IfErrors vs_runtime_cleanup_done
  FileRead $0 $1
  FileClose $0
  StrCmp $1 "video-stitch-runtime-v2" 0 vs_runtime_cleanup_done
  RMDir /r "$LOCALAPPDATA\com.videostitch.desktop\Runtime-v2"
  RMDir "$LOCALAPPDATA\com.videostitch.desktop"
  vs_runtime_cleanup_done:
  Pop $2
  Pop $1
  Pop $0
!macroend
