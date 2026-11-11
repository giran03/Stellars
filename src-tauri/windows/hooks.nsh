; Stellars NSIS installer hook: after the files are installed, set up the virtual audio cables.
; The app does the work itself (`--setup-cables`): it downloads and installs the WHQL-signed
; VB-CABLE and Hi-Fi Cable when they are missing and applies the Stellars endpoint names. It runs
; hidden, needs no extra prompt (the installer is already elevated) and never blocks the install.
!macro NSIS_HOOK_POSTINSTALL
  DetailPrint "Setting up Stellars virtual audio cables (VB-CABLE, Hi-Fi Cable) and endpoint names..."
  nsExec::ExecToLog '"$INSTDIR\${MAINBINARYNAME}.exe" --setup-cables'
  Pop $0
  ; 3010 = the drivers need a restart before Windows shows the cables: offer it on the last page.
  ${If} $0 == 3010
    SetRebootFlag true
  ${EndIf}
!macroend

; Before the files are removed, undo the audio changes: remove the cables Stellars installed and
; put the Windows default devices and per-app routing back as they were. Skipped while the
; installer replaces an older version (/UPDATE, /P), so an update keeps the cables.
!macro NSIS_HOOK_PREUNINSTALL
  ${GetOptions} $CMDLINE "/UPDATE" $R0
  ${If} ${Errors}
    ${GetOptions} $CMDLINE "/P" $R0
    ${If} ${Errors}
      DetailPrint "Removing Stellars virtual audio cables and restoring your audio settings..."
      nsExec::ExecToLog '"$INSTDIR\${MAINBINARYNAME}.exe" --uninstall-cables'
      Pop $0
      ${If} $0 == 3010
        SetRebootFlag true
      ${EndIf}
    ${EndIf}
  ${EndIf}
!macroend
