; Glassdock installer hooks (included by Tauri's NSIS template).
;
; Before files are replaced or removed, a running dock is asked to quit the
; normal way, so it hands the taskbar back itself. On uninstall, everything
; Glassdock changed outside its own folders is undone: the taskbar state,
; the Claude Code status line entry and the start-at-login entry.

!macro GLASSDOCK_QUIT
  IfFileExists "$INSTDIR\glassdock.exe" 0 +3
    ExecWait '"$INSTDIR\glassdock.exe" --quit'
    Sleep 2000
!macroend

!macro NSIS_HOOK_PREINSTALL
  !insertmacro GLASSDOCK_QUIT
!macroend

!macro NSIS_HOOK_POSTINSTALL
  ; A way back that works even when the dock can't be reached.
  CreateShortcut "$SMPROGRAMS\Glassdock - Restore taskbar.lnk" "$INSTDIR\glassdock.exe" "--restore" "$INSTDIR\glassdock.exe" 0
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  !insertmacro GLASSDOCK_QUIT
  IfFileExists "$INSTDIR\glassdock.exe" 0 +2
    ExecWait '"$INSTDIR\glassdock.exe" --uninstall-cleanup'
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  Delete "$SMPROGRAMS\Glassdock - Restore taskbar.lnk"
!macroend
