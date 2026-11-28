[CmdletBinding()]
param(
    [ValidateRange(3, 200)][int]$Runs = 28,
    [ValidateRange(1, 2000)][int]$RetainedRecords = 1200
)

$ErrorActionPreference = 'Stop'
$repo = Split-Path (Split-Path $PSScriptRoot -Parent) -Parent
$parent = Join-Path $repo '.local'
$root = Join-Path $parent "meowcal-log-bench-$([guid]::NewGuid().ToString('N'))"
$null = New-Item -ItemType Directory -Path "$root/src"
try {
    $path = Join-Path $repo 'src-tauri/src/diagnostic_artifacts.rs'
    $source = Get-Content -LiteralPath $path -Raw
    $start = $source.IndexOf('fn encode_frame(')
    $end = $source.LastIndexOf('#[cfg(test)]')
    if ($start -lt 0 -or $end -le $start) { throw 'Cannot locate the diagnostics encoder and writer.' }
    $lock = Get-Content -LiteralPath (Join-Path $repo 'src-tauri/Cargo.lock') -Raw
    $image = [regex]::Match($lock, '(?m)^name = "image"\r?\nversion = "(?<v>[0-9.]+)"')
    if (!$image.Success) { throw 'Cannot find the image version in Cargo.lock.' }
    $version = $image.Groups['v'].Value
    @"
[package]
name = "diagnostic-logging-bench"
version = "0.0.0"
edition = "2024"
[dependencies]
image = { version = "=$version", default-features = false, features = ["png"] }
"@ | Set-Content -LiteralPath "$root/Cargo.toml" -Encoding utf8
    $harness = Get-Content -LiteralPath "$PSScriptRoot/measure.rs" -Raw
    [IO.File]::WriteAllText("$root/src/main.rs", $harness + "`n" + $source.Substring($start, $end-$start))
    Write-Output "diagnostic_artifacts.rs SHA256: $((Get-FileHash -LiteralPath $path).Hash)"
    Write-Output "host arch: $([Runtime.InteropServices.RuntimeInformation]::OSArchitecture); OS: $([Environment]::OSVersion.Version); image: $version"
    Push-Location $root
    try {
        cargo run --quiet --offline --release -- "$root/evidence" "$Runs" "$RetainedRecords"
        if ($LASTEXITCODE -ne 0) { throw "Benchmark failed ($LASTEXITCODE)." }
    } finally { Pop-Location }
} finally {
    $separator = [IO.Path]::DirectorySeparatorChar
    if ((Resolve-Path -LiteralPath $root).Path -ieq $root -and $root.StartsWith(("$parent$separator"), [StringComparison]::OrdinalIgnoreCase)) {
        Remove-Item -LiteralPath $root -Recurse -Force
    }
}
