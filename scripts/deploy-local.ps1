$ErrorActionPreference = 'Stop'
$projectRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$deliveryRoot = [IO.Path]::GetFullPath((Join-Path $projectRoot '交付'))
$sourceExe = Join-Path $projectRoot 'src-tauri/target/release/ruc-web-helper.exe'
$targetExe = Join-Path $deliveryRoot '人大校园网助手.exe'
if (!(Test-Path -LiteralPath $sourceExe -PathType Leaf)) {
    throw '请先运行 npm run desktop:build。'
}
New-Item -ItemType Directory -Path $deliveryRoot -Force | Out-Null

# Only stop copies of this application belonging to this project.
$allowedNames = @('人大校园网助手.exe', 'ruc-web-helper.exe', 'ruc-campus-assistant.exe')
$processes = @(Get-CimInstance Win32_Process | Where-Object {
    $_.Name -in $allowedNames -and $_.ExecutablePath -and
    ([IO.Path]::GetFullPath($_.ExecutablePath)).StartsWith(
        $projectRoot + [IO.Path]::DirectorySeparatorChar,
        [StringComparison]::OrdinalIgnoreCase)
})
foreach ($process in $processes) {
    Stop-Process -Id $process.ProcessId -ErrorAction Stop
    Wait-Process -Id $process.ProcessId -Timeout 15 -ErrorAction SilentlyContinue
}

# Replace the executable in place. Never replace credentials, settings or caches.
for ($copyAttempt = 0; $copyAttempt -lt 10; $copyAttempt++) {
    try { Copy-Item -LiteralPath $sourceExe -Destination $targetExe -Force; break }
    catch { if ($copyAttempt -eq 9) { throw }; Start-Sleep -Milliseconds 250 }
}
$sourceHash = (Get-FileHash -LiteralPath $sourceExe -Algorithm SHA256).Hash
$targetHash = (Get-FileHash -LiteralPath $targetExe -Algorithm SHA256).Hash
if ($sourceHash -ne $targetHash) { throw '程序复制校验失败，未启动。' }
# Preserve enabled startup registration when it still refers to a retired copy.
$runPath = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run'
$startupCommand = Get-ItemPropertyValue -LiteralPath $runPath -Name '人大校园网助手' -ErrorAction SilentlyContinue
if ($startupCommand -and $startupCommand -match '^"?(.+?\.exe)"?(?:\s+--autostart)?$') {
    $registeredExe = [IO.Path]::GetFullPath($Matches[1])
    if ($registeredExe.StartsWith($deliveryRoot + [IO.Path]::DirectorySeparatorChar,
        [StringComparison]::OrdinalIgnoreCase) -and
        [IO.Path]::GetFileName($registeredExe) -in $allowedNames) {
        Set-ItemProperty -LiteralPath $runPath -Name '人大校园网助手' -Value ('"' + $targetExe + '" --autostart')
    }
}

# The user requested opening the interactive app after each replacement.
$started = Start-Process -FilePath $targetExe -WorkingDirectory $deliveryRoot -WindowStyle Normal -PassThru
Start-Sleep -Seconds 2
$started.Refresh()
if ($started.HasExited) { throw '程序启动后已退出，请检查是否有项目外的旧版仍在运行。' }
Write-Output ('本地程序已更新并启动：' + $targetExe)
