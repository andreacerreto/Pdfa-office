Unicode true
!ifdef NATIVE_X64_INSTALLER
  Target amd64-unicode
!endif
!include "MUI2.nsh"
!include "x64.nsh"
!include "WinVer.nsh"

!ifndef APP_VERSION
  !define APP_VERSION "0.1.0"
!endif
!ifndef PACKAGE_DIR
  !define PACKAGE_DIR "..\build\package"
!endif
!ifndef OUTPUT_DIR
  !define OUTPUT_DIR "..\dist"
!endif
!ifndef UNINSTALL_INCLUDE
  !error "Generate an exact-file uninstall manifest and pass -DUNINSTALL_INCLUDE=path."
!endif
!include "${UNINSTALL_INCLUDE}"

Name "PDF/A Office"
OutFile "${OUTPUT_DIR}/PDF-A-Office-${APP_VERSION}-Windows-x64-Setup.exe"
InstallDir "$LOCALAPPDATA\Programs\PDF-A Office"
InstallDirRegKey HKCU "Software\PDF-A Office" "InstallDir"
RequestExecutionLevel user
ManifestSupportedOS all
SetCompressor /SOLID lzma
SetCompressorDictSize 32
ShowInstDetails show
ShowUninstDetails show
BrandingText "PDF/A Office - Conversione e verifica locali"
VIProductVersion "${APP_VERSION}.0"
VIAddVersionKey /LANG=1040 "ProductName" "PDF/A Office"
VIAddVersionKey /LANG=1040 "FileDescription" "Installazione PDF/A Office per Windows x64"
VIAddVersionKey /LANG=1040 "FileVersion" "${APP_VERSION}"
VIAddVersionKey /LANG=1040 "ProductVersion" "${APP_VERSION}"
VIAddVersionKey /LANG=1040 "LegalCopyright" "PDF/A Office contributors - AGPL-3.0-or-later"

!define MUI_ICON "${PACKAGE_DIR}/icon.ico"
!define MUI_UNICON "${PACKAGE_DIR}/icon.ico"
!define MUI_ABORTWARNING
!define MUI_WELCOMEPAGE_TITLE "Benvenuto in PDF/A Office"
!define MUI_WELCOMEPAGE_TEXT "Converti PDF in PDF/A-1b, PDF/A-2b e PDF/A-3b e verifica i risultati con veraPDF.$\r$\n$\r$\nTutti i componenti sono inclusi. Dopo l'installazione non serve una connessione Internet.$\r$\n$\r$\nL'applicazione verrà installata per il tuo utente."
!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_LICENSE "${PACKAGE_DIR}/LICENSE"
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!define MUI_FINISHPAGE_RUN "$INSTDIR\pdfa-office.exe"
!define MUI_FINISHPAGE_RUN_TEXT "Apri PDF/A Office"
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "Italian"

Function .onInit
  ${IfNot} ${RunningX64}
    MessageBox MB_OK|MB_ICONSTOP "Questa versione richiede Windows a 64 bit."
    Abort
  ${EndIf}
  ${IfNot} ${AtLeastWin10}
    MessageBox MB_OK|MB_ICONSTOP "Questa versione richiede Windows 10 o Windows 11."
    Abort
  ${EndIf}
  SetShellVarContext current
  SetRegView 64
FunctionEnd

Section "PDF/A Office" MainSection
  ; Only install into an empty directory or one owned by this application.
  IfFileExists "$INSTDIR\pdfa-office.install" install_ok
  IfFileExists "$INSTDIR\*.*" 0 install_ok
  MessageBox MB_OK|MB_ICONSTOP "La cartella contiene già altri file. Scegli una cartella vuota dedicata a PDF/A Office."
  Abort
  install_ok:
  SetOutPath "$INSTDIR"
  File /r "${PACKAGE_DIR}/*"
  WriteUninstaller "$INSTDIR\Disinstalla.exe"
  WriteRegStr HKCU "Software\PDF-A Office" "InstallDir" "$INSTDIR"
  CreateDirectory "$SMPROGRAMS\PDF-A Office"
  CreateShortcut "$SMPROGRAMS\PDF-A Office\PDF-A Office.lnk" "$INSTDIR\pdfa-office.exe" "" "$INSTDIR\icon.ico"
  CreateShortcut "$SMPROGRAMS\PDF-A Office\Disinstalla.lnk" "$INSTDIR\Disinstalla.exe"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\PDF-A Office" "DisplayName" "PDF/A Office"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\PDF-A Office" "DisplayVersion" "${APP_VERSION}"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\PDF-A Office" "DisplayIcon" "$INSTDIR\icon.ico"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\PDF-A Office" "InstallLocation" "$INSTDIR"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\PDF-A Office" "UninstallString" '"$INSTDIR\Disinstalla.exe"'
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\PDF-A Office" "QuietUninstallString" '"$INSTDIR\Disinstalla.exe" /S'
  WriteRegDWORD HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\PDF-A Office" "NoModify" 1
  WriteRegDWORD HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\PDF-A Office" "NoRepair" 1
SectionEnd

Function un.onInit
  SetShellVarContext current
  SetRegView 64
  IfFileExists "$INSTDIR\pdfa-office.install" uninstall_ok
  MessageBox MB_OK|MB_ICONSTOP "Cartella di installazione non riconosciuta. Nessun file è stato rimosso."
  Abort
  uninstall_ok:
FunctionEnd

Section "Uninstall"
  ; Delete only the exact files in this build's package. Unknown user files
  ; survive even when saved inside resources, docs, licenses or sources.
  !insertmacro RemoveInstalledFiles
  Delete "$INSTDIR\Disinstalla.exe"
  RMDir "$INSTDIR"
  Delete "$SMPROGRAMS\PDF-A Office\PDF-A Office.lnk"
  Delete "$SMPROGRAMS\PDF-A Office\Disinstalla.lnk"
  RMDir "$SMPROGRAMS\PDF-A Office"
  DeleteRegKey HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\PDF-A Office"
  DeleteRegKey HKCU "Software\PDF-A Office"
SectionEnd
