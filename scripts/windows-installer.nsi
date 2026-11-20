Unicode true
!include "MUI2.nsh"
!include "LogicLib.nsh"
!include "FileFunc.nsh"
!include "x64.nsh"
!include "WinVer.nsh"

!ifndef APP_VERSION
  !error "APP_VERSION must be set by package-installer.ps1"
!endif
Name "OpenCowork"
OutFile "${OUTPUT_FILE}"
InstallDir "$LOCALAPPDATA\Programs\OpenCowork"
InstallDirRegKey HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\OpenCowork" "InstallLocation"
RequestExecutionLevel user
SetCompressor /SOLID lzma
SetCompressorDictSize 32
ShowInstDetails nevershow
ShowUninstDetails nevershow
VIProductVersion "${APP_VERSION}.0"
VIAddVersionKey /LANG=2052 "ProductName" "OpenCowork"
VIAddVersionKey /LANG=2052 "FileDescription" "OpenCowork Windows Installer"
VIAddVersionKey /LANG=2052 "FileVersion" "${APP_VERSION}"
VIAddVersionKey /LANG=2052 "LegalCopyright" "OpenCowork contributors"

Var TestMode
Var RegistrationName
Var WorkspaceDir
Var WebViewPresent

!define MUI_ABORTWARNING
!define MUI_WELCOMEPAGE_TEXT "安装 OpenCowork 到当前用户账户，并创建桌面和开始菜单快捷方式。$\r$\n$\r$\n首次补齐运行组件时需要联网。"
!define MUI_FINISHPAGE_RUN "$INSTDIR\opencowork-desktop.exe"
!define MUI_FINISHPAGE_RUN_TEXT "启动 OpenCowork"
!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "SimpChinese"
!insertmacro MUI_LANGUAGE "English"

Function .onInit
  ${IfNot} ${AtLeastWin10}
    MessageBox MB_OK|MB_ICONSTOP "OpenCowork 需要 Windows 10 或 Windows 11。"
    SetErrorLevel 1
    Abort
  ${EndIf}
  ${IfNot} ${RunningX64}
    MessageBox MB_OK|MB_ICONSTOP "OpenCowork 需要 64 位 Windows。"
    SetErrorLevel 1
    Abort
  ${EndIf}
  SetShellVarContext current
  SetRegView 64
  StrCpy $TestMode "0"
  StrCpy $RegistrationName "OpenCowork"
  ${GetParameters} $R0
  ClearErrors
  ${GetOptions} $R0 "/TESTMODE" $R1
  ${IfNot} ${Errors}
    StrCpy $TestMode "1"
    StrCpy $RegistrationName "OpenCowork-InstallerTest"
  ${EndIf}
  ${If} $TestMode != "1"
    check_running_app:
      FindWindow $R0 "" "OpenCowork Desktop"
      ${If} $R0 != 0
        IfSilent running_app_silent
        MessageBox MB_RETRYCANCEL|MB_ICONEXCLAMATION "请先关闭 OpenCowork，然后点击重试。" IDRETRY check_running_app
        Abort
        running_app_silent:
          SetErrorLevel 1
          Abort
      ${EndIf}
  ${EndIf}
FunctionEnd

Function un.onInit
  ReadINIStr $TestMode "$INSTDIR\installation.ini" "Installation" "TestMode"
  ${If} $TestMode != "1"
    check_running_app:
      FindWindow $R0 "" "OpenCowork Desktop"
      ${If} $R0 != 0
        IfSilent running_app_silent
        MessageBox MB_RETRYCANCEL|MB_ICONEXCLAMATION "请先关闭 OpenCowork，然后点击重试。" IDRETRY check_running_app
        Abort
        running_app_silent:
          SetErrorLevel 1
          Abort
      ${EndIf}
  ${EndIf}
FunctionEnd



Function DetectWebView
  StrCpy $WebViewPresent "0"
  SetRegView 32
  ReadRegStr $R0 HKLM "Software\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}" "pv"
  ${If} $R0 != ""
  ${AndIf} $R0 != "0.0.0.0"
    StrCpy $WebViewPresent "1"
  ${EndIf}
  ReadRegStr $R0 HKCU "Software\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}" "pv"
  ${If} $R0 != ""
  ${AndIf} $R0 != "0.0.0.0"
    StrCpy $WebViewPresent "1"
  ${EndIf}
  SetRegView 64
  ReadRegStr $R0 HKLM "Software\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}" "pv"
  ${If} $R0 != ""
  ${AndIf} $R0 != "0.0.0.0"
    StrCpy $WebViewPresent "1"
  ${EndIf}
  ReadRegStr $R0 HKCU "Software\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}" "pv"
  ${If} $R0 != ""
  ${AndIf} $R0 != "0.0.0.0"
    StrCpy $WebViewPresent "1"
  ${EndIf}
FunctionEnd

Section "OpenCowork" MainSection
  Call DetectWebView
  ${If} $WebViewPresent != "1"
    InitPluginsDir
    SetOutPath "$PLUGINSDIR"
    File /oname=MicrosoftEdgeWebview2Setup.exe "${WEBVIEW_BOOTSTRAPPER}"
    ExecWait '"$PLUGINSDIR\MicrosoftEdgeWebview2Setup.exe" /silent /install' $R0
    Call DetectWebView
    ${If} $WebViewPresent != "1"
      MessageBox MB_OK|MB_ICONSTOP "运行组件安装失败，请连接网络后重试。"
      SetErrorLevel 1
      Abort
    ${EndIf}
  ${EndIf}
  SetOutPath "$INSTDIR"
  File "${BINARY_DIR}\opencowork-shell.exe"
  File "${BINARY_DIR}\opencowork-desktop.exe"
  File "${PROJECT_ROOT}\third-party\windows-capture-LICENSE.txt"
  File "${PROJECT_ROOT}\third-party\portable-pty-LICENSE.txt"
  WriteUninstaller "$INSTDIR\Uninstall.exe"
  WriteINIStr "$INSTDIR\installation.ini" "Installation" "TestMode" "$TestMode"
  WriteINIStr "$INSTDIR\installation.ini" "Installation" "Version" "${APP_VERSION}"
  WriteINIStr "$INSTDIR\installation.ini" "Installation" "Commit" "${BUILD_COMMIT}"
  StrCpy $WorkspaceDir "$LOCALAPPDATA\OpenCowork\workspace"
  ${If} $TestMode == "1"
    StrCpy $WorkspaceDir "$INSTDIR\workspace"
  ${EndIf}
  SetOutPath "$WorkspaceDir"
  CreateDirectory "$SMPROGRAMS\$RegistrationName"
  CreateShortCut "$DESKTOP\$RegistrationName.lnk" "$INSTDIR\opencowork-desktop.exe"
  CreateShortCut "$SMPROGRAMS\$RegistrationName\OpenCowork.lnk" "$INSTDIR\opencowork-desktop.exe"
  CreateShortCut "$SMPROGRAMS\$RegistrationName\Uninstall.lnk" "$INSTDIR\Uninstall.exe"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\$RegistrationName" "DisplayName" "OpenCowork"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\$RegistrationName" "DisplayVersion" "${APP_VERSION}"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\$RegistrationName" "Publisher" "OpenCowork contributors"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\$RegistrationName" "DisplayIcon" "$INSTDIR\opencowork-desktop.exe"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\$RegistrationName" "InstallLocation" "$INSTDIR"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\$RegistrationName" "UninstallString" '$\"$INSTDIR\Uninstall.exe$\"'
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\$RegistrationName" "QuietUninstallString" '$\"$INSTDIR\Uninstall.exe$\" /S'
  WriteRegDWORD HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\$RegistrationName" "NoModify" 1
  WriteRegDWORD HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\$RegistrationName" "NoRepair" 1
SectionEnd

Section "Uninstall"
  SetShellVarContext current
  SetRegView 64
  ReadINIStr $TestMode "$INSTDIR\installation.ini" "Installation" "TestMode"
  StrCpy $RegistrationName "OpenCowork"
  ${If} $TestMode == "1"
    StrCpy $RegistrationName "OpenCowork-InstallerTest"
  ${EndIf}
  Delete "$DESKTOP\$RegistrationName.lnk"
  Delete "$SMPROGRAMS\$RegistrationName\OpenCowork.lnk"
  Delete "$SMPROGRAMS\$RegistrationName\Uninstall.lnk"
  RMDir "$SMPROGRAMS\$RegistrationName"
  DeleteRegKey HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\$RegistrationName"
  Delete "$INSTDIR\opencowork-shell.exe"
  Delete "$INSTDIR\opencowork-desktop.exe"
  Delete "$INSTDIR\windows-capture-LICENSE.txt"
  Delete "$INSTDIR\portable-pty-LICENSE.txt"
  Delete "$INSTDIR\installation.ini"
  Delete "$INSTDIR\Uninstall.exe"
  RMDir "$INSTDIR"
SectionEnd
