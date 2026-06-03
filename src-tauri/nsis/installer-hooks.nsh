!define PIPEMIC_VBCABLE_PACKAGE "${__FILEDIR__}\..\vendor\vb-cable\VBCABLE_Driver_Pack45.zip"

Function PipeMicVbCableInstalled
  Push $0
  StrCpy $0 0

  ${If} ${FileExists} "$WINDIR\System32\drivers\vbaudio_cable64_win10.sys"
  ${OrIf} ${FileExists} "$WINDIR\System32\drivers\vbaudio_cable64_win7.sys"
  ${OrIf} ${FileExists} "$WINDIR\System32\drivers\vbaudio_cable64_vista.sys"
  ${OrIf} ${FileExists} "$WINDIR\System32\drivers\vbaudio_cable_win7.sys"
  ${OrIf} ${FileExists} "$WINDIR\System32\drivers\vbaudio_cable_vista.sys"
    StrCpy $0 1
  ${EndIf}

  Exch $0
FunctionEnd

Function PipeMicInstallVbCable
  Call PipeMicVbCableInstalled
  Pop $0
  ${If} $0 == 1
    DetailPrint "VB-CABLE driver files found; skipping VB-CABLE setup."
    Return
  ${EndIf}

  ${IfNot} ${Silent}
    MessageBox MB_ICONINFORMATION|MB_OK "PipeMic will now open the VB-CABLE driver installer by VB-Audio. Follow the driver installer prompts, then reboot Windows if requested."
  ${EndIf}

  InitPluginsDir
  Delete "$PLUGINSDIR\VBCABLE_Driver_Pack45.zip"
  RMDir /r "$PLUGINSDIR\vb-cable"

  DetailPrint "Preparing VB-CABLE driver installer..."
  File "/oname=$PLUGINSDIR\VBCABLE_Driver_Pack45.zip" "${PIPEMIC_VBCABLE_PACKAGE}"

  FileOpen $1 "$PLUGINSDIR\extract-vb-cable.ps1" w
  FileWrite $1 "param([string]$$Zip,[string]$$Out)$\r$\n"
  FileWrite $1 "if (Test-Path $$Out) { Remove-Item -LiteralPath $$Out -Recurse -Force }$\r$\n"
  FileWrite $1 "New-Item -ItemType Directory -Path $$Out -Force | Out-Null$\r$\n"
  FileWrite $1 "Expand-Archive -LiteralPath $$Zip -DestinationPath $$Out -Force$\r$\n"
  FileClose $1

  nsExec::ExecToLog 'powershell.exe -NoProfile -ExecutionPolicy Bypass -File "$PLUGINSDIR\extract-vb-cable.ps1" "$PLUGINSDIR\VBCABLE_Driver_Pack45.zip" "$PLUGINSDIR\vb-cable"'
  Pop $0
  ${If} $0 != 0
    MessageBox MB_ICONEXCLAMATION|MB_OK "PipeMic installed, but VB-CABLE could not be extracted. Install VB-CABLE manually from https://www.vb-cable.com."
    Return
  ${EndIf}

  ${If} ${RunningX64}
    StrCpy $0 "$PLUGINSDIR\vb-cable\VBCABLE_Setup_x64.exe"
  ${Else}
    StrCpy $0 "$PLUGINSDIR\vb-cable\VBCABLE_Setup.exe"
  ${EndIf}

  ${IfNot} ${FileExists} "$0"
    MessageBox MB_ICONEXCLAMATION|MB_OK "PipeMic installed, but the VB-CABLE setup program was not found in the bundled driver package. Install VB-CABLE manually from https://www.vb-cable.com."
    Return
  ${EndIf}

  DetailPrint "Starting VB-CABLE driver installer..."
  ExecWait '"$0"' $1
  ${If} $1 == 0
    DetailPrint "VB-CABLE driver installer finished."
  ${Else}
    MessageBox MB_ICONEXCLAMATION|MB_OK "PipeMic installed, but the VB-CABLE driver installer exited with code $1. If CABLE Input and CABLE Output do not appear after rebooting, install VB-CABLE manually from https://www.vb-cable.com."
  ${EndIf}
FunctionEnd

!macro NSIS_HOOK_POSTINSTALL
  Call PipeMicInstallVbCable
!macroend
