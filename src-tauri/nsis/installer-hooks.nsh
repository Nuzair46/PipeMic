!define PIPEMIC_VBCABLE_PACKAGE "${__FILEDIR__}\..\vendor\vb-cable\VBCABLE_Driver_Pack45.zip"
!define PIPEMIC_UNINSTKEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\PipeMic"
!define PIPEMIC_MAIN_EXE "pipemic.exe"

Var PipeMicExistingInstall
Var PipeMicVbCableDetected
Var PipeMicInstallVbCableChoice
Var PipeMicVbCableCheckbox

!macro PIPEMIC_VBCABLE_OPTIONS_PAGE
  Page custom PipeMicVbCableOptionsPage PipeMicVbCableOptionsLeave
!macroend

Function PipeMicExistingInstallDetected
  StrCpy $0 0
  ReadRegStr $1 SHCTX "${PIPEMIC_UNINSTKEY}" "UninstallString"
  ${If} $1 != ""
    StrCpy $0 1
  ${ElseIf} ${FileExists} "$INSTDIR\${PIPEMIC_MAIN_EXE}"
    StrCpy $0 1
  ${EndIf}

  ${If} $0 == 1
    StrCpy $PipeMicExistingInstall 1
  ${EndIf}

  Push $0
FunctionEnd

Function PipeMicVbCableInstalled
  Push $0
  Push $1
  StrCpy $0 0

  ${If} ${FileExists} "$WINDIR\System32\drivers\vbaudio_cable64_win10.sys"
  ${OrIf} ${FileExists} "$WINDIR\System32\drivers\vbaudio_cable64_win7.sys"
  ${OrIf} ${FileExists} "$WINDIR\System32\drivers\vbaudio_cable64_vista.sys"
  ${OrIf} ${FileExists} "$WINDIR\System32\drivers\vbaudio_cable_win7.sys"
  ${OrIf} ${FileExists} "$WINDIR\System32\drivers\vbaudio_cable_vista.sys"
    StrCpy $0 1
  ${EndIf}

  ${If} $0 == 0
    ReadRegStr $1 HKLM "SYSTEM\CurrentControlSet\Services\vbaudio_cable64_win10" "ImagePath"
    ${If} $1 != ""
      StrCpy $0 1
    ${EndIf}
  ${EndIf}
  ${If} $0 == 0
    ReadRegStr $1 HKLM "SYSTEM\CurrentControlSet\Services\vbaudio_cable64_win7" "ImagePath"
    ${If} $1 != ""
      StrCpy $0 1
    ${EndIf}
  ${EndIf}
  ${If} $0 == 0
    ReadRegStr $1 HKLM "SYSTEM\CurrentControlSet\Services\vbaudio_cable64_vista" "ImagePath"
    ${If} $1 != ""
      StrCpy $0 1
    ${EndIf}
  ${EndIf}
  ${If} $0 == 0
    ReadRegStr $1 HKLM "SYSTEM\CurrentControlSet\Services\vbaudio_cable_win7" "ImagePath"
    ${If} $1 != ""
      StrCpy $0 1
    ${EndIf}
  ${EndIf}
  ${If} $0 == 0
    ReadRegStr $1 HKLM "SYSTEM\CurrentControlSet\Services\vbaudio_cable_vista" "ImagePath"
    ${If} $1 != ""
      StrCpy $0 1
    ${EndIf}
  ${EndIf}

  ${If} $0 == 0
    InitPluginsDir
    FileOpen $1 "$PLUGINSDIR\detect-vb-cable.ps1" w
    FileWrite $1 "$$pattern = 'VB-Audio Virtual Cable|VB-CABLE|CABLE Input \(VB-Audio Virtual Cable\)|CABLE Output \(VB-Audio Virtual Cable\)'$\r$\n"
    FileWrite $1 "$$found = $$false$\r$\n"
    FileWrite $1 "$$pnp = Get-PnpDevice -ErrorAction SilentlyContinue | Where-Object { $$_.FriendlyName -match $$pattern -or $$_.InstanceId -match 'VBAUDIO|VB-AUDIO|VBCABLE' }$\r$\n"
    FileWrite $1 "if ($$pnp) { $$found = $$true }$\r$\n"
    FileWrite $1 "if (-not $$found) {$\r$\n"
    FileWrite $1 "  $$sound = Get-CimInstance Win32_SoundDevice -ErrorAction SilentlyContinue | Where-Object { $$_.Name -match $$pattern -or $$_.Manufacturer -match 'VB-Audio' }$\r$\n"
    FileWrite $1 "  if ($$sound) { $$found = $$true }$\r$\n"
    FileWrite $1 "}$\r$\n"
    FileWrite $1 "if ($$found) { exit 0 }$\r$\n"
    FileWrite $1 "exit 1$\r$\n"
    FileClose $1

    nsExec::ExecToLog 'powershell.exe -NoProfile -ExecutionPolicy Bypass -File "$PLUGINSDIR\detect-vb-cable.ps1"'
    Pop $1
    ${If} $1 == 0
      StrCpy $0 1
    ${EndIf}
  ${EndIf}

  Pop $1
  Exch $0
FunctionEnd

Function PipeMicVbCableOptionsPage
  StrCpy $PipeMicInstallVbCableChoice 0
  StrCpy $PipeMicExistingInstall 0
  StrCpy $PipeMicVbCableDetected 0

  ${If} ${Silent}
    Abort
  ${EndIf}

  ${GetOptions} $CMDLINE "/P" $0
  ${IfNot} ${Errors}
    Abort
  ${EndIf}

  Call PipeMicExistingInstallDetected
  Pop $0

  Call PipeMicVbCableInstalled
  Pop $0
  ${If} $0 == 1
    StrCpy $PipeMicVbCableDetected 1
  ${EndIf}

  ${If} $PipeMicVbCableDetected == 1
    !insertmacro MUI_HEADER_TEXT "Optional VB-CABLE Driver" "VB-CABLE appears to already be installed."
  ${ElseIf} $PipeMicExistingInstall == 1
    !insertmacro MUI_HEADER_TEXT "Optional VB-CABLE Driver" "Choose whether this update should install VB-CABLE."
  ${Else}
    !insertmacro MUI_HEADER_TEXT "Optional VB-CABLE Driver" "Choose whether PipeMic should install VB-CABLE."
  ${EndIf}

  nsDialogs::Create 1018
  Pop $0
  ${IfThen} $0 == error ${|} Abort ${|}
  ${IfThen} $(^RTL) = 1 ${|} nsDialogs::SetRTL $(^RTL) ${|}

  ${If} $PipeMicVbCableDetected == 1
    ${NSD_CreateLabel} 0 0 100% 42u "VB-CABLE appears to already be installed. You can leave this unchecked unless you want to run the VB-CABLE installer again."
  ${ElseIf} $PipeMicExistingInstall == 1
    ${NSD_CreateLabel} 0 0 100% 36u "PipeMic can use VB-CABLE as a virtual microphone output. Leave this unchecked if your existing virtual audio cable already works."
  ${Else}
    ${NSD_CreateLabel} 0 0 100% 36u "PipeMic can use VB-CABLE as a virtual microphone output. Install it if you do not already have another virtual audio cable."
  ${EndIf}
  Pop $0

  ${NSD_CreateCheckbox} 0 52u 100% 14u "Install VB-CABLE after PipeMic"
  Pop $PipeMicVbCableCheckbox
  ${If} $PipeMicVbCableDetected == 1
    SendMessage $PipeMicVbCableCheckbox ${BM_SETCHECK} ${BST_UNCHECKED} 0
  ${ElseIf} $PipeMicExistingInstall == 1
    SendMessage $PipeMicVbCableCheckbox ${BM_SETCHECK} ${BST_UNCHECKED} 0
  ${Else}
    SendMessage $PipeMicVbCableCheckbox ${BM_SETCHECK} ${BST_CHECKED} 0
  ${EndIf}

  nsDialogs::Show
FunctionEnd

Function PipeMicVbCableOptionsLeave
  SendMessage $PipeMicVbCableCheckbox ${BM_GETCHECK} 0 0 $PipeMicInstallVbCableChoice
  ${If} $PipeMicInstallVbCableChoice == 1
    Call PipeMicInstallVbCable
  ${EndIf}
FunctionEnd

Function PipeMicInstallVbCable
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

!macro NSIS_HOOK_PREINSTALL
  Call PipeMicExistingInstallDetected
  Pop $0

  ${If} $PipeMicExistingInstall == 1
  ${AndIf} ${FileExists} "$INSTDIR\${MAINBINARYNAME}.exe"
    DetailPrint "Existing PipeMic executable found; closing PipeMic before update."
    !insertmacro CheckIfAppIsRunning "${MAINBINARYNAME}.exe" "${PRODUCTNAME}"

    ClearErrors
    Delete "$INSTDIR\${MAINBINARYNAME}.exe"
    ${If} ${Errors}
    ${OrIf} ${FileExists} "$INSTDIR\${MAINBINARYNAME}.exe"
      MessageBox MB_ICONEXCLAMATION|MB_OK "PipeMic could not remove the old executable. Close PipeMic and try the installer again."
      Abort
    ${Else}
      DetailPrint "Removed old PipeMic executable before reinstall."
    ${EndIf}
  ${EndIf}
!macroend
