!macro NSIS_HOOK_PREUNINSTALL
  ; Tauri's whole-directory cleanup can contain exports from older versions.
  StrCpy $DeleteAppDataCheckboxState 0
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  ; Keep the ownership marker until every payload entry has been removed.
  Push $0
  Push $1
  Push $2
  Push $3
  Push $4
  Push $5
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
  FindFirst $3 $4 "$LOCALAPPDATA\com.videostitch.desktop\Runtime-v2\*.*"
  vs_runtime_next_entry:
  StrCmp $4 "" vs_runtime_payload_done
  StrCmp $4 "." vs_runtime_find_next
  StrCmp $4 ".." vs_runtime_find_next
  StrCmp $4 ".video-stitch-owner" vs_runtime_find_next
  ClearErrors
  System::Call 'kernel32::GetFileAttributesW(w "$LOCALAPPDATA\com.videostitch.desktop\Runtime-v2\$4") i.r5'
  IntOp $2 $5 & 0x400
  IntCmp $2 0 vs_runtime_plain_entry vs_runtime_payload_failed vs_runtime_payload_failed
  vs_runtime_plain_entry:
  IntOp $5 $5 & 0x10
  IntCmp $5 0 vs_runtime_delete_file vs_runtime_delete_directory vs_runtime_delete_directory
  vs_runtime_delete_file:
  Delete "$LOCALAPPDATA\com.videostitch.desktop\Runtime-v2\$4"
  Goto vs_runtime_check_delete
  vs_runtime_delete_directory:
  RMDir /r "$LOCALAPPDATA\com.videostitch.desktop\Runtime-v2\$4"
  vs_runtime_check_delete:
  IfErrors vs_runtime_payload_failed
  vs_runtime_find_next:
  FindNext $3 $4
  Goto vs_runtime_next_entry
  vs_runtime_payload_done:
  FindClose $3
  ClearErrors
  Delete "$LOCALAPPDATA\com.videostitch.desktop\Runtime-v2\.video-stitch-owner"
  RMDir "$LOCALAPPDATA\com.videostitch.desktop\Runtime-v2"
  IfErrors vs_runtime_restore_marker
  RMDir "$LOCALAPPDATA\com.videostitch.desktop"
  ClearErrors
  Goto vs_runtime_cleanup_done
  vs_runtime_restore_marker:
  FileOpen $0 "$LOCALAPPDATA\com.videostitch.desktop\Runtime-v2\.video-stitch-owner" w
  IfErrors vs_runtime_cleanup_failed
  FileWrite $0 "video-stitch-runtime-v2"
  FileClose $0
  Goto vs_runtime_cleanup_failed
  vs_runtime_payload_failed:
  FindClose $3
  vs_runtime_cleanup_failed:
  MessageBox MB_ICONEXCLAMATION|MB_OK "私有运行文件仍被占用或无法确认归属，已保留。导出视频与恢复记录不受影响。" /SD IDOK
  ClearErrors
  vs_runtime_cleanup_done:
  Pop $5
  Pop $4
  Pop $3
  Pop $2
  Pop $1
  Pop $0
!macroend
