!include "LogicLib.nsh"
!define MASH_INSTALLER_HOOK_DIR "${__FILEDIR__}"

; This runs in the NEW installer, so it also fixes updates from older Mash
; versions which left their ADB daemon running after the app exited.
!macro NSIS_HOOK_PREINSTALL
  ; Tauri's default app check follows PREINSTALL. Do it first as well so an
  ; older app cannot restart ADB between cleanup and resource replacement.
  !insertmacro CheckIfAppIsRunning "${MAINBINARYNAME}.exe" "${PRODUCTNAME}"
  Push $0
  Push $1
  InitPluginsDir
  File "/oname=$PLUGINSDIR\mash-stop-adb.ps1" "${MASH_INSTALLER_HOOK_DIR}\stop-adb.ps1"
  nsExec::ExecToStack /TIMEOUT=15000 '"$SYSDIR\WindowsPowerShell\v1.0\powershell.exe" -NoProfile -NonInteractive -ExecutionPolicy Bypass -WindowStyle Hidden -File "$PLUGINSDIR\mash-stop-adb.ps1" -AdbPath "$INSTDIR\resources\adb\adb.exe"'
  Pop $0
  Pop $1
  ${If} $0 != 0
    DetailPrint "$1"
    MessageBox MB_OK|MB_ICONSTOP "无法停止 Mash 的 ADB 进程，请关闭设备连接程序后重试。$\r$\n$1" /SD IDOK
    SetErrorLevel 1
    Abort
  ${EndIf}
  Pop $1
  Pop $0
!macroend
