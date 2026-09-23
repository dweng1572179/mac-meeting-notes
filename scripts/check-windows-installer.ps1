param([Parameter(Mandatory=$true)][string]$Installer)
$ErrorActionPreference = 'Stop'

# This script is only run on a disposable hosted Windows runner.
if ($env:CI -ne 'true') { throw 'Installer smoke test requires a disposable CI profile' }
$appDirectory = Join-Path $env:LOCALAPPDATA 'Meeting Notes'
if (Test-Path $appDirectory) { throw 'Refusing to replace an existing installation' }
$dataDirectory = Join-Path $env:APPDATA 'com.dweng.meetingnotes'
if (Test-Path $dataDirectory) { throw 'Refusing to modify existing application data' }
$executable = Join-Path $appDirectory 'meeting-notes.exe'
$preserved = @{}
$process = $null
try {
    foreach ($attempt in 1..2) {
        $setup = Start-Process -FilePath (Resolve-Path $Installer) -ArgumentList '/S' -Wait -PassThru
        if ($setup.ExitCode -ne 0) { throw "Installer failed: $($setup.ExitCode)" }
        if (!(Test-Path $executable)) { throw 'Installer did not create the application executable' }
        if ($attempt -eq 1) {
            New-Item -ItemType Directory -Path "$dataDirectory/sessions", "$dataDirectory/audio" | Out-Null
            $id = [guid]::NewGuid().ToString()
            $audioPath = Join-Path "$dataDirectory/audio" "$id.wav"
            # One synthetic PCM sample; no device or network capture is involved.
            $wave = [System.IO.BinaryWriter]::new([System.IO.File]::Create($audioPath))
            try {
                $wave.Write([Text.Encoding]::ASCII.GetBytes('RIFF')); $wave.Write([uint32]38)
                $wave.Write([Text.Encoding]::ASCII.GetBytes('WAVEfmt ')); $wave.Write([uint32]16)
                $wave.Write([uint16]1); $wave.Write([uint16]1); $wave.Write([uint32]16000)
                $wave.Write([uint32]32000); $wave.Write([uint16]2); $wave.Write([uint16]16)
                $wave.Write([Text.Encoding]::ASCII.GetBytes('data')); $wave.Write([uint32]2); $wave.Write([int16]1)
            } finally { $wave.Dispose() }
            @{
                id = $id; title = 'Synthetic installer preservation check'; startedAt = '2026-01-01T00:00:00Z'
                endedAt = '2026-01-01T00:00:01Z'; context = ''; attendees = @(); originalNotes = 'Keep this note.'
                notes = 'Keep this note.'; transcript = $null; enrichedNotes = $null; status = 'complete'; error = $null
                audioPath = $audioPath; audioFormat = 'wav'
            } | ConvertTo-Json | Set-Content "$dataDirectory/sessions/$id.json" -Encoding utf8NoBOM
            '{"language":"en","vocabulary":"Synthetic","model":"gpt-4o-mini-transcribe"}' |
                Set-Content "$dataDirectory/settings.json" -Encoding utf8NoBOM
            foreach ($path in @("$dataDirectory/sessions/$id.json", $audioPath, "$dataDirectory/settings.json")) {
                $preserved[$path] = (Get-FileHash $path).Hash
            }
        }
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
        foreach ($path in $preserved.Keys) {
            if ((Get-FileHash $path).Hash -ne $preserved[$path]) { throw "Install/launch changed saved data: $path" }
        }
    }
} finally {
    if ($process -and !$process.HasExited) { Stop-Process -Id $process.Id -Force }
    $uninstaller = Join-Path $appDirectory 'uninstall.exe'
    if (Test-Path $uninstaller) {
        $removed = Start-Process -FilePath $uninstaller -ArgumentList '/S' -Wait -PassThru
        if ($removed.ExitCode -ne 0) { throw "Uninstaller failed: $($removed.ExitCode)" }
    }
    if (Test-Path $executable) { throw 'Uninstaller left the application executable behind' }
    foreach ($path in $preserved.Keys) {
        if ((Get-FileHash $path).Hash -ne $preserved[$path]) { throw "Uninstaller changed saved data: $path" }
    }
    # This profile was empty before the test; remove only the synthetic data created above.
    if (Test-Path $dataDirectory) { Remove-Item $dataDirectory -Recurse -Force }
}
Write-Output 'Per-user install, reinstall, launches, normal closes, uninstall, and saved note/audio/settings preservation passed.'
