$ErrorActionPreference = 'Stop'
$source = Split-Path -Parent $MyInvocation.MyCommand.Path
$target = Join-Path $env:LOCALAPPDATA 'CalmMD'
New-Item -ItemType Directory -Force -Path $target | Out-Null
Copy-Item -Path (Join-Path $source 'calmmd-gpui.exe') -Destination $target -Force
Copy-Item -Path (Join-Path $source 'resources') -Destination $target -Recurse -Force

$startMenu = Join-Path $env:APPDATA 'Microsoft\Windows\Start Menu\Programs'
$shortcut = Join-Path $startMenu 'CalmMD.lnk'
$shell = New-Object -ComObject WScript.Shell
$link = $shell.CreateShortcut($shortcut)
$link.TargetPath = Join-Path $target 'calmmd-gpui.exe'
$link.WorkingDirectory = $target
$link.IconLocation = Join-Path $target 'resources\icons\icon.ico'
$link.Save()
Write-Host "CalmMD installed to $target"
