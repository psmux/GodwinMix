; The NSIS installer's hooks: Windows Firewall rules for the mixer and its
; plugins, written after the files are in place and removed before they go.
; What the rules are and why is in firewall.ps1 beside this file.
;
; A per machine install runs with the shell folders of all users, so $APPDATA
; is switched to the person running the installer while the script runs: the
; plugins run from that person's AppData.

; ExecWait and not the nsExec plugin: a plugin is a DLL packed into the
; setup unsigned, and the release refuses a setup with anything unsigned in it.
!macro GMX_FIREWALL ACTION
  SetShellVarContext current
  ExecWait '"powershell.exe" -NoProfile -NonInteractive -WindowStyle Hidden -ExecutionPolicy Bypass -File "$INSTDIR\setup\firewall.ps1" -Action ${ACTION} -InstallDir "$INSTDIR" -AppData "$APPDATA"' $0
  !if "${INSTALLMODE}" == "perMachine"
    SetShellVarContext all
  !endif
!macroend

!macro NSIS_HOOK_POSTINSTALL
  !insertmacro GMX_FIREWALL Install
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  !insertmacro GMX_FIREWALL Uninstall
!macroend
