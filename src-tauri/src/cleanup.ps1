param([int]$ParentProcessId, [string]$ReadyFile)
$ErrorActionPreference = 'Stop'
$dataDirectory = Join-Path ([Environment]::GetFolderPath('LocalApplicationData')) 'cn.ruc.webhelper'
$dataDirectory = [IO.Path]::GetFullPath($dataDirectory)
$resetMarker = $dataDirectory + '.reset'

function Remove-DataTree([string]$directory) {
    $attributes = [IO.File]::GetAttributes($directory)
    if (($attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { throw '数据目录不能是链接或联接点' }
    foreach ($entry in [IO.Directory]::EnumerateFileSystemEntries($directory)) {
        $attributes = [IO.File]::GetAttributes($entry)
        if (($attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
            if (($attributes -band [IO.FileAttributes]::Directory) -ne 0) { [IO.Directory]::Delete($entry) }
            else { [IO.File]::Delete($entry) }
        } elseif (($attributes -band [IO.FileAttributes]::Directory) -ne 0) { Remove-DataTree $entry }
        else { [IO.File]::SetAttributes($entry, [IO.FileAttributes]::Normal); [IO.File]::Delete($entry) }
    }
    [IO.Directory]::Delete($directory)
}

try {
    $markerStream = [IO.File]::Open($resetMarker, [IO.FileMode]::OpenOrCreate, [IO.FileAccess]::ReadWrite, [IO.FileShare]::None)
    function Write-ResetState([string]$state) {
        $bytes = [Text.Encoding]::ASCII.GetBytes($state)
        $markerStream.SetLength(0); $markerStream.Position = 0
        $markerStream.Write($bytes, 0, $bytes.Length); $markerStream.Flush($true)
    }
    Write-ResetState 'pending'
    [IO.File]::WriteAllText($ReadyFile, 'ready')
    $parentProcess = Get-Process -Id $ParentProcessId -ErrorAction SilentlyContinue
    if ($parentProcess) { $parentProcess | Wait-Process -Timeout 60 -ErrorAction Stop }
    for ($attempt = 0; $attempt -lt 120; $attempt++) {
        try {
            if ([IO.Directory]::Exists($dataDirectory)) { Remove-DataTree $dataDirectory }
            Write-ResetState 'complete'
            break
        } catch {
            if ($attempt -eq 119) { throw }
            Start-Sleep -Milliseconds 250
        }
    }
} catch {
    Add-Type -AssemblyName PresentationFramework
    [System.Windows.MessageBox]::Show('本地数据清除未完成，可能仍有文件被占用。请退出其他助手后重新打开软件，程序会继续清理。', '人大校园网助手') | Out-Null
} finally {
    if ($markerStream) { $markerStream.Dispose() }
    Remove-Item -LiteralPath $ReadyFile -Force -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath $PSCommandPath -Force -ErrorAction SilentlyContinue
}
