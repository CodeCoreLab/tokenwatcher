; ============================================================
; TokenWatcher NSIS 安装包脚本
;
; 设计要点：
; 1. RequestExecutionLevel user + 装到 $LOCALAPPDATA\Programs —— 全程不弹 UAC。
; 2. 安装前 taskkill 旧实例，否则运行中的 exe 被占用会静默写不进去。
; 3. TokenWatcher.ini 只在不存在时安装，覆盖安装不冲掉用户配置。
; 4. 卸载问一次「是否连数据一起删」，不默认删人数据。
; 5. 卸载时清 HKCU Run 项，避免残留自启指向已删路径。
;
; 编译：.tools\nsis-3.10\makensis.exe tokenwatcher.nsi
; ============================================================

Unicode true
!include "MUI2.nsh"
!include "LogicLib.nsh"

!define APP_NAME      "TokenWatcher"
!define APP_VERSION   "1.5.1"
!define APP_PUBLISHER "内部工具（引擎 luwill/token-watcher, MIT）"
!define UNINST_KEY    "Software\Microsoft\Windows\CurrentVersion\Uninstall\${APP_NAME}"
!define RUN_KEY       "Software\Microsoft\Windows\CurrentVersion\Run"

Name "${APP_NAME} ${APP_VERSION}"
OutFile "dist\TokenWatcher-${APP_VERSION}-win-x64-setup.exe"
InstallDir "$LOCALAPPDATA\Programs\${APP_NAME}"
InstallDirRegKey HKCU "${UNINST_KEY}" "InstallLocation"
RequestExecutionLevel user
SetCompressor /SOLID lzma
SetCompressorDictSize 64

VIProductVersion "${APP_VERSION}.0"
VIAddVersionKey "ProductName"     "${APP_NAME}"
VIAddVersionKey "FileDescription" "${APP_NAME} 安装程序"
VIAddVersionKey "FileVersion"     "${APP_VERSION}"
VIAddVersionKey "ProductVersion"  "${APP_VERSION}"
VIAddVersionKey "LegalCopyright"  "Engine: MIT (luwill/token-watcher)"

!define MUI_ICON   "tray\icon.ico"
!define MUI_UNICON "tray\icon.ico"
!define MUI_ABORTWARNING

!define MUI_WELCOMEPAGE_TITLE "安装 ${APP_NAME} ${APP_VERSION}"
!define MUI_WELCOMEPAGE_TEXT  "本地 AI 编码工具 token 用量面板。$\r$\n$\r$\n后台静默运行，托盘图标打开面板；不需要 Node、不需要联网。$\r$\n$\r$\n安装到当前用户目录，不会请求管理员权限。"

!define MUI_FINISHPAGE_RUN "$INSTDIR\TokenWatcherTray.exe"
!define MUI_FINISHPAGE_RUN_TEXT "立即启动（静默驻留托盘，不弹窗）"
!define MUI_FINISHPAGE_SHOWREADME "$INSTDIR\README.md"
!define MUI_FINISHPAGE_SHOWREADME_TEXT "查看使用说明"
!define MUI_FINISHPAGE_SHOWREADME_NOTCHECKED

!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH

!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES

!insertmacro MUI_LANGUAGE "SimpChinese"

; ------------------------------------------------------------ 安装

Section "TokenWatcher" SecMain
  SectionIn RO

  ; 旧实例必须先退，否则 exe 被锁无法覆盖
  nsExec::Exec 'taskkill /IM TokenWatcherTray.exe /F'
  Pop $0
  nsExec::Exec 'taskkill /IM TokenWatcher.exe /F'
  Pop $0
  Sleep 600

  SetOutPath "$INSTDIR"
  File "dist\TokenWatcher.exe"
  File "dist\TokenWatcherTray.exe"
  File "dist\README.md"

  ; 用户改过的配置不能被升级冲掉
  ${IfNot} ${FileExists} "$INSTDIR\TokenWatcher.ini"
    File "dist\TokenWatcher.ini"
  ${EndIf}

  WriteUninstaller "$INSTDIR\uninstall.exe"

  CreateDirectory "$SMPROGRAMS\${APP_NAME}"
  CreateShortCut "$SMPROGRAMS\${APP_NAME}\${APP_NAME}.lnk" "$INSTDIR\TokenWatcherTray.exe"
  CreateShortCut "$SMPROGRAMS\${APP_NAME}\打开配置.ini.lnk" "$INSTDIR\TokenWatcher.ini"
  CreateShortCut "$SMPROGRAMS\${APP_NAME}\卸载 ${APP_NAME}.lnk" "$INSTDIR\uninstall.exe"
  CreateShortCut "$DESKTOP\${APP_NAME}.lnk" "$INSTDIR\TokenWatcherTray.exe"

  WriteRegStr HKCU "${UNINST_KEY}" "DisplayName"     "${APP_NAME}"
  WriteRegStr HKCU "${UNINST_KEY}" "DisplayVersion"  "${APP_VERSION}"
  WriteRegStr HKCU "${UNINST_KEY}" "Publisher"       "${APP_PUBLISHER}"
  WriteRegStr HKCU "${UNINST_KEY}" "InstallLocation" "$INSTDIR"
  WriteRegStr HKCU "${UNINST_KEY}" "DisplayIcon"     "$INSTDIR\TokenWatcherTray.exe"
  WriteRegStr HKCU "${UNINST_KEY}" "UninstallString" '"$INSTDIR\uninstall.exe"'
  WriteRegStr HKCU "${UNINST_KEY}" "QuietUninstallString" '"$INSTDIR\uninstall.exe" /S'
  WriteRegDWORD HKCU "${UNINST_KEY}" "NoModify" 1
  WriteRegDWORD HKCU "${UNINST_KEY}" "NoRepair" 1
  WriteRegDWORD HKCU "${UNINST_KEY}" "EstimatedSize" 143360
SectionEnd

; ------------------------------------------------------------ 卸载

Section "Uninstall"
  nsExec::Exec 'taskkill /IM TokenWatcherTray.exe /F'
  Pop $0
  nsExec::Exec 'taskkill /IM TokenWatcher.exe /F'
  Pop $0
  Sleep 600

  Delete "$INSTDIR\TokenWatcher.exe"
  Delete "$INSTDIR\TokenWatcherTray.exe"
  Delete "$INSTDIR\TokenWatcher.ini"
  Delete "$INSTDIR\README.md"
  Delete "$INSTDIR\uninstall.exe"
  RMDir "$INSTDIR"

  Delete "$SMPROGRAMS\${APP_NAME}\*.lnk"
  RMDir "$SMPROGRAMS\${APP_NAME}"
  Delete "$DESKTOP\${APP_NAME}.lnk"

  ; 自启项必须清，否则开机指向已删路径
  DeleteRegValue HKCU "${RUN_KEY}" "${APP_NAME}"
  DeleteRegKey HKCU "${UNINST_KEY}"

  MessageBox MB_YESNO|MB_ICONQUESTION \
    "是否同时删除统计数据？$\r$\n$\r$\n$LOCALAPPDATA\TokenWatcher（运行时资源与日志）$\r$\n$PROFILE\.tokenmeter（用量统计数据库）$\r$\n$\r$\n选「否」则保留，将来重装可继续用。" \
    IDNO KeepData
    RMDir /r "$LOCALAPPDATA\TokenWatcher"
    RMDir /r "$PROFILE\.tokenmeter"
  KeepData:
SectionEnd
