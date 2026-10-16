[CmdletBinding()]
param([Parameter(Mandatory = $true)][string]$AdbPath)

$ErrorActionPreference = 'Stop'
$expectedPath = [System.IO.Path]::GetFullPath($AdbPath)

# ADB's daemon outlives Mash and keeps its executable and DLLs mapped. Do not
# use kill-server: it targets the shared server, including other tools' ADB.
foreach ($adbProcess in @(Get-Process -Name adb -ErrorAction SilentlyContinue)) {
    try {
        $processPath = $adbProcess.Path
        if (-not $processPath -or -not [string]::Equals(
            [System.IO.Path]::GetFullPath($processPath),
            $expectedPath,
            [System.StringComparison]::OrdinalIgnoreCase)) {
            continue
        }
        $adbProcess.Kill()
        if (-not $adbProcess.WaitForExit(5000)) {
            throw "Timed out stopping bundled ADB (PID $($adbProcess.Id))."
        }
    } catch [System.InvalidOperationException] {
        # The selected process may exit between enumeration and termination.
        if (-not $adbProcess.HasExited) { throw }
    } finally {
        $adbProcess.Dispose()
    }
}
