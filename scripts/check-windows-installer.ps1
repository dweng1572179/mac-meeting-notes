param([Parameter(Mandatory=$true)][string]$Installer)
$ErrorActionPreference = 'Stop'

# This script is only run on a disposable hosted Windows runner.
if ($env:CI -ne 'true') { throw 'Installer smoke test requires a disposable CI profile' }
$appDirectory = Join-Path $env:LOCALAPPDATA 'Meeting Notes'
if (Test-Path $appDirectory) { throw 'Refusing to replace an existing installation' }
$process = $null
try {
    $setup = Start-Process -FilePath (Resolve-Path $Installer) -ArgumentList '/S' -Wait -PassThru
    if ($setup.ExitCode -ne 0) { throw "Installer failed: $($setup.ExitCode)" }
    $executable = Join-Path $appDirectory 'meeting-notes.exe'
    if (!(Test-Path $executable)) { throw 'Installer did not create the application executable' }
    $process = Start-Process -FilePath $executable -PassThru
    $deadline = (Get-Date).AddSeconds(45)
    do {
        Start-Sleep -Milliseconds 500
        $process.Refresh()
        if ($process.HasExited) { throw "Application exited during startup: $($process.ExitCode)" }
    } while ($process.MainWindowHandle -eq 0 -and (Get-Date) -lt $deadline)
    if ($process.MainWindowHandle -eq 0) { throw 'Application did not create a window' }
    if (!$process.CloseMainWindow()) { throw 'Application did not accept a normal close request' }
    if (!$process.WaitForExit(15000)) { throw 'Application did not close cleanly' }
    Write-Output 'Per-user installer, first application launch, and normal close passed.'
} finally {
    if ($process -and !$process.HasExited) { Stop-Process -Id $process.Id -Force }
    $uninstaller = Join-Path $appDirectory 'uninstall.exe'
    if (Test-Path $uninstaller) {
        $removed = Start-Process -FilePath $uninstaller -ArgumentList '/S' -Wait -PassThru
        if ($removed.ExitCode -ne 0) { throw "Uninstaller failed: $($removed.ExitCode)" }
    }
}
