; PTS Workbench's installer: for this Windows user only, so no admin rights are needed.
;
; Puts the launcher, the app, the sign-in window and the PDF engine in %LOCALAPPDATA%\Programs\PTS Workbench, makes
; the desktop and Start menu shortcuts (the same names as the Electron app's, so they're replaced), adds it to
; Settings > Apps (to uninstall), and starts it. Run with /S it shows nothing: that's how the Electron app moves over
; (handoff/) and how an update that needs a new launcher installs (crates/app/src/updates.rs).
; The app's own folder (%APPDATA%\PTS Workbench: settings, hours, sign-ins) is never touched, uninstalling included.
;
; Built by the GitHub build:  makensis /DVERSION=0.5.0-beta.3 /DNUMVER=0.5.0 /DSRC=<program files> /DICON=<icon.ico>
;                                      /DOUT=<installer.exe> installer\pts-workbench.nsi

Unicode true
ManifestDPIAware true

!ifndef VERSION
  !define VERSION "0.0.0"
!endif
!ifndef NUMVER
  !define NUMVER "0.0.0"
!endif
!define NAME "PTS Workbench"
!define UNINSTALLER "Uninstall PTS Workbench.exe"
; (its own key: the Electron app's is under its own id)
!define UNINST_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\PTS Workbench (new)"

Name "${NAME}"
OutFile "${OUT}"
RequestExecutionLevel user
InstallDir "$LOCALAPPDATA\Programs\PTS Workbench"
SetCompressor /SOLID lzma
Icon "${ICON}"
UninstallIcon "${ICON}"
BrandingText "${NAME} ${VERSION}"
ShowInstDetails nevershow
ShowUninstDetails nevershow
AutoCloseWindow true

VIProductVersion "${NUMVER}.0"
VIAddVersionKey "ProductName" "${NAME}"
VIAddVersionKey "FileDescription" "${NAME} installer"
VIAddVersionKey "ProductVersion" "${VERSION}"
VIAddVersionKey "FileVersion" "${VERSION}"
VIAddVersionKey "CompanyName" "Ashton Miller"
VIAddVersionKey "LegalCopyright" "Ashton Miller"

!include "LogicLib.nsh"
!include "FileFunc.nsh"

Page instfiles
UninstPage uninstConfirm
UninstPage instfiles

; A copy that's open is closed first (its files can't be replaced while it runs). Only this user's.
!macro CloseRunning
  nsExec::Exec 'taskkill /F /IM pts-workbench.exe'
  Pop $0
  nsExec::Exec 'taskkill /F /IM pts-workbench-app.exe'
  Pop $0
  nsExec::Exec 'taskkill /F /IM pts-signin.exe'
  Pop $0
  nsExec::Exec 'taskkill /F /IM pts-vci32.exe'
  Pop $0
  Sleep 800
!macroend

Section "PTS Workbench"
  !insertmacro CloseRunning
  SetOutPath "$INSTDIR"
  SetOverwrite on
  File "${SRC}\pts-workbench.exe"
  File "${SRC}\pts-workbench-app.exe"
  File "${SRC}\pts-signin.exe"
  File "${SRC}\pdfium.dll"
  File "${SRC}\pts-vci32.exe"
  WriteUninstaller "$INSTDIR\${UNINSTALLER}"

  ; (the same names as the Electron app's shortcuts: those now open this)
  CreateShortcut "$DESKTOP\${NAME}.lnk" "$INSTDIR\pts-workbench.exe" "" "$INSTDIR\pts-workbench.exe" 0
  CreateShortcut "$SMPROGRAMS\${NAME}.lnk" "$INSTDIR\pts-workbench.exe" "" "$INSTDIR\pts-workbench.exe" 0

  WriteRegStr HKCU "${UNINST_KEY}" "DisplayName" "${NAME}"
  WriteRegStr HKCU "${UNINST_KEY}" "DisplayVersion" "${VERSION}"
  WriteRegStr HKCU "${UNINST_KEY}" "Publisher" "Ashton Miller"
  WriteRegStr HKCU "${UNINST_KEY}" "DisplayIcon" "$INSTDIR\pts-workbench.exe"
  WriteRegStr HKCU "${UNINST_KEY}" "InstallLocation" "$INSTDIR"
  WriteRegStr HKCU "${UNINST_KEY}" "UninstallString" '"$INSTDIR\${UNINSTALLER}"'
  WriteRegStr HKCU "${UNINST_KEY}" "QuietUninstallString" '"$INSTDIR\${UNINSTALLER}" /S'
  WriteRegDWORD HKCU "${UNINST_KEY}" "NoModify" 1
  WriteRegDWORD HKCU "${UNINST_KEY}" "NoRepair" 1
  ${GetSize} "$INSTDIR" "/S=0K" $0 $1 $2
  IntFmt $0 "0x%08X" $0
  WriteRegDWORD HKCU "${UNINST_KEY}" "EstimatedSize" "$0"
SectionEnd

; Installed: the app starts (silent installs too: the Electron app's move, an update)
Function .onInstSuccess
  Exec '"$INSTDIR\pts-workbench.exe"'
FunctionEnd

Section "Uninstall"
  !insertmacro CloseRunning
  Delete "$INSTDIR\pts-workbench.exe"
  Delete "$INSTDIR\pts-workbench-app.exe"
  Delete "$INSTDIR\pts-signin.exe"
  Delete "$INSTDIR\pdfium.dll"
  Delete "$INSTDIR\pts-vci32.exe"
  Delete "$INSTDIR\${UNINSTALLER}"
  RMDir "$INSTDIR"
  Delete "$DESKTOP\${NAME}.lnk"
  Delete "$SMPROGRAMS\${NAME}.lnk"
  DeleteRegKey HKCU "${UNINST_KEY}"
SectionEnd
