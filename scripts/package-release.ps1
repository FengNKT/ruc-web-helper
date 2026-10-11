$ErrorActionPreference = 'Stop'
$projectRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
Push-Location -LiteralPath $projectRoot
try {
    node scripts/version.cjs --check
    if ($LASTEXITCODE -ne 0) { throw 'Version validation failed.' }
    $version = (Get-Content -LiteralPath 'package.json' -Raw | ConvertFrom-Json).version
    $executable = Join-Path $projectRoot 'src-tauri/target/x86_64-pc-windows-msvc/release/ruc-web-helper.exe'
    if (!(Test-Path -LiteralPath $executable -PathType Leaf)) { throw 'Build the Windows x64 release executable first.' }
    $output = [IO.Path]::GetFullPath((Join-Path $projectRoot "release/$version"))
    if (!$output.StartsWith($projectRoot + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) { throw 'Output must remain inside the project.' }
    if (Test-Path -LiteralPath $output) { throw "Output already exists: $output. Use a clean checkout to package a release." }
    $staging = Join-Path $output 'portable'
    New-Item -ItemType Directory -Path $staging -Force | Out-Null
    $assetBase = "ruc-web-helper_${version}_windows-x86_64"
    Copy-Item -LiteralPath $executable -Destination (Join-Path $staging '人大校园网助手.exe')
    Copy-Item -LiteralPath (Join-Path $projectRoot 'LICENSE') -Destination $staging
    Copy-Item -LiteralPath (Join-Path $projectRoot 'docs/usage.md') -Destination (Join-Path $staging '使用说明.md')
    Copy-Item -LiteralPath (Join-Path $projectRoot 'docs/statistics.md') -Destination $staging
    $exeHash = (Get-FileHash -LiteralPath $executable -Algorithm SHA256).Hash.ToLowerInvariant()
    "$exeHash  人大校园网助手.exe" | Set-Content -LiteralPath (Join-Path $staging 'SHA256.txt') -Encoding utf8NoBOM
    Copy-Item -LiteralPath $executable -Destination (Join-Path $output "$assetBase.exe")
    [IO.Compression.ZipFile]::CreateFromDirectory($staging, (Join-Path $output "${assetBase}_portable.zip"))
    $assets = @("$assetBase.exe", "${assetBase}_portable.zip")
    $checksums = foreach ($asset in $assets) {
        $hash = (Get-FileHash -LiteralPath (Join-Path $output $asset) -Algorithm SHA256).Hash.ToLowerInvariant()
        "$hash  $asset"
    }
    [IO.File]::WriteAllText((Join-Path $output 'SHA256SUMS.txt'), (($checksums -join "`n") + "`n"), [Text.UTF8Encoding]::new($false))
    Write-Output "Prepared release assets in $output"
} finally {
    Pop-Location
}
