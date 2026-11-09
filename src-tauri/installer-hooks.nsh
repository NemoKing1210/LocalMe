; LocalMe's uninstall hook (Tauri NSIS "installerHooks").
; The application registers itself for autostart by writing HKCU\...\Run\LocalMe, which the
; installer does not know about, so it must be removed here or a ghost entry pointing at a
; deleted executable stays behind. The StartupApproved value is what Windows' Task Manager
; startup page keeps per entry; older builds wrote it through the autostart plugin.
!macro NSIS_HOOK_PREUNINSTALL
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "LocalMe"
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run" "LocalMe"
!macroend
