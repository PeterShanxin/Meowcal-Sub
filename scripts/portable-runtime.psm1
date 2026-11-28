$ErrorActionPreference = 'Stop'

function Get-AppLocalCrt([ValidateSet('x64', 'arm64')][string]$Architecture) {
    # Use licensed Visual Studio's supported app-local CRT, never the host System32 copy.
    # https://learn.microsoft.com/cpp/windows/redistributing-visual-cpp-files
    $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
    if (!(Test-Path -LiteralPath $vswhere -PathType Leaf)) {
        throw 'Visual Studio Build Tools with the cross-architecture Visual C++ redistributable components are required.'
    }
    $installations = @(& $vswhere -all -products '*' -property installationPath)
    if ($LASTEXITCODE -ne 0) { throw 'Could not locate Visual Studio Build Tools.' }
    $candidates = foreach ($installation in $installations) {
        $root = Join-Path $installation 'VC\Redist\MSVC'
        foreach ($version in (Get-ChildItem -LiteralPath $root -Directory -ErrorAction SilentlyContinue)) {
            if ($version.Name -notmatch '^\d+\.\d+\.\d+(?:\.\d+)?$') { continue }
            $archRoot = Join-Path $version.FullName $Architecture
            foreach ($crt in (Get-ChildItem -LiteralPath $archRoot -Directory -ErrorAction SilentlyContinue)) {
                if ($crt.Name -match '^Microsoft\.VC\d+\.CRT$' -and
                    (Test-Path -LiteralPath (Join-Path $crt.FullName 'vcruntime140.dll') -PathType Leaf) -and
                    (Test-Path -LiteralPath (Join-Path $crt.FullName 'msvcp140.dll') -PathType Leaf)) {
                    [pscustomobject]@{ Version = [version]$version.Name; Directory = $crt.FullName }
                }
            }
        }
    }
    $selected = $candidates | Sort-Object Version -Descending | Select-Object -First 1
    if (!$selected) { throw "No Microsoft app-local Visual C++ runtime installed for $Architecture." }
    Write-Host "Bundling Microsoft app-local VC runtime ($Architecture $($selected.Version))."
    Get-ChildItem -LiteralPath $selected.Directory -File -Filter '*.dll'
}

function Copy-PortableCrt {
    param(
        [Parameter(Mandatory)][ValidateSet('x64', 'arm64')][string]$Architecture,
        [Parameter(Mandatory)][string]$Directory
    )
    $core = Join-Path $Directory 'resources\core'
    $runtimeFiles = @(Get-AppLocalCrt -Architecture $Architecture)
    foreach ($runtime in $runtimeFiles) {
        # Windows first checks the executable's directory. The app and pinned
        # Core have separate directories, so both need a local copy.
        Copy-Item -LiteralPath $runtime.FullName -Destination $Directory
        Copy-Item -LiteralPath $runtime.FullName -Destination $core
    }
}

Export-ModuleMember -Function Copy-PortableCrt, Get-AppLocalCrt
