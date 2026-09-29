[CmdletBinding()]
param([Parameter(Mandatory)][string]$PackageDirectory, [Parameter(Mandatory)][string]$OutputDirectory)
$ErrorActionPreference = 'Stop'
if ($env:GITHUB_ACTIONS -ne 'true' -or $env:RUNNER_ENVIRONMENT -ne 'github-hosted') {
    throw 'WACK validation is restricted to disposable GitHub-hosted runners.'
}
$output = [IO.Path]::GetFullPath($OutputDirectory)
New-Item -ItemType Directory -Path $output -Force | Out-Null
$kit = Join-Path ${env:ProgramFiles(x86)} 'Windows Kits\10\App Certification Kit'
$appcert = Join-Path $kit 'appcert.exe'
$os = Get-CimInstance Win32_OperatingSystem
$result = [ordered]@{
    os = $os.Caption
    build = $os.BuildNumber
    productType = $os.ProductType
    sessionId = (Get-Process -Id $PID).SessionId
    appcert = $appcert
    available = Test-Path $appcert
    fullClientAcceptance = $false
    status = 'not-run'
}
function Save-Result { $result | ConvertTo-Json -Depth 5 | Set-Content "$output/result.json" -Encoding UTF8 }
Save-Result
if (-not $result.available -or $result.sessionId -eq 0) {
    $result.reason = if (-not $result.available) { 'WACK executable is not installed.' } else { 'WACK requires a nonzero active user session.' }
    Save-Result
    Write-Warning $result.reason
    return
}
$thumbprint = $null
try {
    $result.toolVersion = (Get-Item $appcert).VersionInfo.FileVersion
    $fixtures = Join-Path $env:RUNNER_TEMP 'store-wack-fixtures'
    & (Join-Path $PSScriptRoot 'prepare-store-lifecycle.ps1') -PackageDirectory $PackageDirectory -OutputDirectory $fixtures
    $certificate = Import-Certificate -FilePath "$fixtures/local-test.cer" -CertStoreLocation Cert:\LocalMachine\TrustedPeople
    $thumbprint = $certificate.Thumbprint
    $architecture = [Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString()
    if (-not (Get-AppxPackage Microsoft.VCLibs.140.00.UWPDesktop | Where-Object {
        $_.Architecture -eq $architecture -and [version]$_.Version -ge [version]'14.0.33728.0'
    })) { Add-AppxPackage -Path "$fixtures/vclibs.appx" }
    $result.packageHash = (Get-FileHash "$fixtures/initial.msix").Hash
    $reset = Start-Process -FilePath $appcert -ArgumentList 'reset' -PassThru -WindowStyle Hidden
    if (-not $reset.WaitForExit(60000)) {
        Stop-Process -Id $reset.Id
        throw 'WACK reset exceeded one minute.'
    }
    if ($reset.ExitCode -ne 0) { throw "WACK reset failed with exit code $($reset.ExitCode)." }
    $process = Start-Process -FilePath $appcert -ArgumentList @('test', '-appxpackagepath', "`"$fixtures/initial.msix`"", '-reportoutputpath', "`"$output/report.xml`"") -PassThru -WindowStyle Hidden -RedirectStandardOutput "$output/stdout.txt" -RedirectStandardError "$output/stderr.txt"
    if (-not $process.WaitForExit(1200000)) {
        Stop-Process -Id $process.Id
        throw 'WACK validation exceeded 20 minutes.'
    }
    $result.exitCode = $process.ExitCode
    $result.reportCreated = Test-Path "$output/report.xml"
    if ($process.ExitCode -ne 0 -or -not $result.reportCreated) { throw 'WACK did not complete successfully; inspect the retained report and logs.' }
    $result.status = 'report-ready-for-review'
    $result.reason = 'Review XML test outcomes and skipped tests. A Windows Server run does not establish Windows 11 client acceptance.'
} catch {
    $result.status = 'failed'
    $result.error = $_.Exception.Message
    throw
} finally {
    Save-Result
    if ($thumbprint) { Remove-Item -LiteralPath "Cert:\LocalMachine\TrustedPeople\$thumbprint" }
}
