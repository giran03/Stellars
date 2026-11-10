; Stellars NSIS installer hook: after the files are installed, set up the virtual audio cables.
; The app does the work itself (`--setup-cables`): it downloads and installs the WHQL-signed
; VB-CABLE and Hi-Fi Cable when they are missing and applies the Stellars endpoint names. It runs
; hidden, needs no extra prompt (the installer is already elevated) and never blocks the install.
!macro NSIS_HOOK_POSTINSTALL
  DetailPrint "Setting up Stellars virtual audio cables (VB-CABLE, Hi-Fi Cable) and endpoint names..."
  nsExec::ExecToLog '"$INSTDIR\${MAINBINARYNAME}.exe" --setup-cables'
  Pop $0
!macroend
