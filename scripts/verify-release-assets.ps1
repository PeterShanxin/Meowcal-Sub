[CmdletBinding()]
param(
    # Directory holding the merged package artifacts from every architecture.
    [Parameter(Mandatory)][string]$Directory,

    # Where to write the SHA256 manifest. Omit to check the assets only, which
    # is what a preflight run that publishes nothing wants.
    [string]$ChecksumPath
)

$ErrorActionPreference = "Stop"

$files = @(Get-ChildItem -LiteralPath $Directory -Recurse -File)

# Named per architecture rather than counted, so a run that produced four x64
# files cannot pass as one that produced both architectures. The signature is
# checked here too: without it the updater manifest cannot be built, and that
# failure belongs before the release exists.
$required = @(
    @{ Kind = "MSI"; Pattern = "*.msi" },
    @{ Kind = "NSIS setup"; Pattern = "*-setup.exe" },
    @{ Kind = "updater signature"; Pattern = "*-setup.exe.sig" }
)
foreach ($architecture in @("x64", "arm64")) {
    foreach ($expected in $required) {
        $found = @($files | Where-Object {
            $_.Name -like "*$architecture*" -and $_.Name -like $expected.Pattern
        })
        if ($found.Count -ne 1) {
            throw "Expected exactly one $architecture $($expected.Kind), found $($found.Count)."
        }
    }
}

if (-not $ChecksumPath) {
    Write-Host "Release assets satisfy the x64 and arm64 contract." -ForegroundColor Green
    return
}

$installers = @($files |
    Where-Object { $_.Extension -eq ".msi" -or $_.Name -like "*-setup.exe" })
$checksumLines = $installers |
    Sort-Object Name |
    ForEach-Object {
        $hash = (Get-FileHash -Algorithm SHA256 -LiteralPath $_.FullName).Hash.ToLowerInvariant()
        # GitHub rewrites spaces in release asset names to periods. Keep the
        # checksum manifest aligned with the names users download.
        $downloadName = $_.Name -replace '[^A-Za-z0-9._-]', '.'
        "$hash  $downloadName"
    }
Set-Content -LiteralPath $ChecksumPath -Value $checksumLines -Encoding utf8NoBOM
Write-Host "Wrote $($checksumLines.Count) checksum line(s) to $ChecksumPath." -ForegroundColor Green
