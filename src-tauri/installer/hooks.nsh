!macro NSIS_HOOK_POSTUNINSTALL
  ; Only the application-owned WebView directory. Never enumerate export directories.
  RMDir /r "$LOCALAPPDATA\com.videostitch.desktop"
!macroend
