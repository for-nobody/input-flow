[CmdletBinding()]
param(
    [switch]$Force,
    [switch]$UseProductIcon
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$repoRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$release = Import-PowerShellDataFile -LiteralPath (Join-Path $repoRoot 'installer\release.psd1')
$Version = [string]$release.Version
if ($Version -notmatch '^\d+\.\d+\.\d+$') {
    throw "installer\release.psd1 Version must be a three-component number: $Version"
}
$PublishFiles = @($release.PublishFiles)
$RustLicenses = @($release.RustLicenses)
$NuGetLicenses = @($release.NuGetLicenses)
$payloadRoot = Join-Path $repoRoot "InputFlow-$Version-install"
$publishRoot = Join-Path $repoRoot 'target\installer-publish'
$setupPath = Join-Path $repoRoot "inno_setup_output\InputFlow-$Version-win-x64-setup.exe"
$issPath = Join-Path $repoRoot 'Innoscripts.iss'
$agentManifest = Join-Path $repoRoot 'apps\inputflow-agent\Cargo.toml'
$settingsProject = Join-Path $repoRoot 'apps\settings-winui\InputFlow.Settings\InputFlow.Settings.csproj'
$projectLicense = Join-Path $repoRoot 'LICENSE'
$iconPath = Join-Path $repoRoot 'installer\InputFlow.ico'

$ExpectedRelativePaths = @($PublishFiles + @('inputflow-agent.exe', 'LICENSE.txt'))
foreach ($license in $RustLicenses) {
    $ExpectedRelativePaths += "Licenses\Rust\$($license.Name)-$($license.Version)\$($license.File)"
}
foreach ($license in $NuGetLicenses) {
    $directory = '{0}-{1}' -f $license.Id.ToLowerInvariant(), $license.Version
    foreach ($file in $license.Files) {
        $ExpectedRelativePaths += "Licenses\NuGet\$directory\$file"
    }
}

function Invoke-Checked {
    param(
        [Parameter(Mandatory)]
        [scriptblock]$Command,
        [Parameter(Mandatory)]
        [string]$Description
    )

    Write-Host "==> $Description"
    & $Command
    if ($LASTEXITCODE -ne 0) {
        throw "$Description failed with exit code $LASTEXITCODE."
    }
}

function Initialize-MsvcEnvironment {
    $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
    if (-not (Test-Path -LiteralPath $vswhere -PathType Leaf)) {
        throw "vswhere.exe was not found: $vswhere"
    }

    $installationJson = (& $vswhere -all -products '*' -format json) | Out-String
    $installations = $installationJson | ConvertFrom-Json
    $selected = $null
    foreach ($installation in $installations) {
        $toolsRoot = Join-Path $installation.installationPath 'VC\Tools\MSVC'
        if (-not (Test-Path -LiteralPath $toolsRoot -PathType Container)) { continue }
        $toolset = Get-ChildItem -LiteralPath $toolsRoot -Directory |
            Sort-Object Name -Descending |
            Where-Object {
                Test-Path -LiteralPath (Join-Path $_.FullName 'lib\x64\msvcrt.lib') -PathType Leaf
            } |
            Select-Object -First 1
        if ($null -ne $toolset) {
            $selected = $installation
            break
        }
    }
    if ($null -eq $selected) {
        throw 'No Visual Studio installation with the x64 MSVC runtime import libraries was found.'
    }

    $vsDevCmd = Join-Path $selected.installationPath 'Common7\Tools\VsDevCmd.bat'
    if (-not (Test-Path -LiteralPath $vsDevCmd -PathType Leaf)) {
        throw "VsDevCmd.bat was not found: $vsDevCmd"
    }
    $environmentCommand = "call `"$vsDevCmd`" -no_logo -arch=x64 -host_arch=x64 >nul && set"
    $environmentLines = & $env:ComSpec /d /c $environmentCommand
    if ($LASTEXITCODE -ne 0) {
        throw "VsDevCmd.bat failed with exit code $LASTEXITCODE."
    }
    foreach ($line in $environmentLines) {
        if ($line -match '^([^=]+)=(.*)$') {
            [Environment]::SetEnvironmentVariable($Matches[1], $Matches[2], 'Process')
        }
    }
    if ([string]::IsNullOrWhiteSpace($env:LIB) -or $env:LIB -notmatch [regex]::Escape($selected.installationPath)) {
        throw 'The selected MSVC environment did not publish its library search path.'
    }
    Write-Host "==> MSVC environment: $($selected.installationPath)"
}

function Add-CargoBin {
    $cargoBin = Join-Path $env:USERPROFILE '.cargo\bin'
    $cargoExe = Join-Path $cargoBin 'cargo.exe'
    if (-not (Test-Path -LiteralPath $cargoExe -PathType Leaf)) {
        throw "cargo.exe was not found: $cargoExe"
    }
    $alreadyPresent = $env:Path -split ';' | Where-Object {
        $_.TrimEnd('\') -eq $cargoBin.TrimEnd('\')
    }
    if (-not $alreadyPresent) {
        $env:Path = "$cargoBin;$env:Path"
    }
}

function Assert-RepositoryPath([string]$Path) {
    $full = [System.IO.Path]::GetFullPath($Path)
    $prefix = $repoRoot.TrimEnd('\') + '\'
    if (-not $full.StartsWith($prefix, [StringComparison]::OrdinalIgnoreCase) -and
        -not $full.Equals($repoRoot, [StringComparison]::OrdinalIgnoreCase)) {
        throw "Refusing to modify a path outside the repository: $full"
    }
}

function Get-PeMachine([string]$Path) {
    $stream = [System.IO.File]::OpenRead($Path)
    try {
        $reader = [System.IO.BinaryReader]::new($stream)
        if ($reader.ReadUInt16() -ne 0x5A4D) { throw "Not a PE file: $Path" }
        $stream.Position = 0x3C
        $peOffset = $reader.ReadUInt32()
        $stream.Position = $peOffset
        if ($reader.ReadUInt32() -ne 0x00004550) { throw "Invalid PE signature: $Path" }
        return $reader.ReadUInt16()
    }
    finally { $stream.Dispose() }
}

function Assert-FileSet([string[]]$Expected, [string[]]$Actual, [string]$Label) {
    $expectedSet = [System.Collections.Generic.HashSet[string]]::new(
        [string[]]$Expected, [StringComparer]::Ordinal)
    $actualSet = [System.Collections.Generic.HashSet[string]]::new(
        [string[]]$Actual, [StringComparer]::Ordinal)
    $missing = @($Expected | Where-Object { -not $actualSet.Contains($_) })
    $extra = @($Actual | Where-Object { -not $expectedSet.Contains($_) })
    if ($missing.Count -ne 0 -or $extra.Count -ne 0) {
        $detail = @("$Label does not match the installer file list.")
        if ($missing.Count -ne 0) { $detail += "Missing: $($missing -join ', ')" }
        if ($extra.Count -ne 0) { $detail += "Unexpected: $($extra -join ', ')" }
        throw ($detail -join [Environment]::NewLine)
    }
}

function Find-Iscc {
    $candidates = @(
        (Join-Path $env:ProgramFiles 'Inno Setup 7\ISCC.exe'),
        (Join-Path ${env:ProgramFiles(x86)} 'Inno Setup 7\ISCC.exe'),
        (Join-Path $env:ProgramFiles 'Inno Setup 6\ISCC.exe'),
        (Join-Path ${env:ProgramFiles(x86)} 'Inno Setup 6\ISCC.exe')
    )
    foreach ($candidate in $candidates) {
        if (Test-Path -LiteralPath $candidate -PathType Leaf) { return $candidate }
    }
    $command = Get-Command ISCC.exe -ErrorAction SilentlyContinue
    if ($null -ne $command) { return $command.Source }
    throw 'ISCC.exe was not found. Install Inno Setup 7 and rerun.'
}

foreach ($requiredInput in @($agentManifest, $settingsProject, $projectLicense, $iconPath, $issPath)) {
    if (-not (Test-Path -LiteralPath $requiredInput -PathType Leaf)) {
        throw "Required installer input is missing: $requiredInput"
    }
}

$agentVersionMatch = Select-String -LiteralPath $agentManifest -Pattern '^version\s*=\s*"([^"]+)"' | Select-Object -First 1
if ($null -eq $agentVersionMatch) { throw 'Unable to read the Agent version.' }
$agentVersion = $agentVersionMatch.Matches[0].Groups[1].Value
[xml]$settingsXml = Get-Content -LiteralPath $settingsProject -Raw
$settingsVersionNode = $settingsXml.SelectSingleNode('/Project/PropertyGroup/Version')
if ($null -eq $settingsVersionNode) { throw 'Unable to read the Settings version.' }
$settingsVersion = $settingsVersionNode.InnerText.Trim()
$issText = Get-Content -LiteralPath $issPath -Raw
if ($issText.IndexOf('#define PayloadDir "InputFlow-" + MyAppVersion + "-install"', [StringComparison]::Ordinal) -lt 0) {
    throw 'Innoscripts.iss must derive the payload directory from MyAppVersion.'
}
if ($agentVersion -ne $Version -or $settingsVersion -ne $Version) {
    throw "Project version must match installer\release.psd1 ($Version). Agent=$agentVersion Settings=$settingsVersion"
}

foreach ($existingOutput in @($payloadRoot, $setupPath)) {
    if (Test-Path -LiteralPath $existingOutput) {
        if (-not $Force) { throw "Output already exists; use -Force to replace it: $existingOutput" }
        Assert-RepositoryPath $existingOutput
    }
}

Initialize-MsvcEnvironment
Add-CargoBin

$separator = [string][char]0x1f
$originalEncodedFlags = $env:CARGO_ENCODED_RUSTFLAGS
$encodedFlags = @()
if (-not [string]::IsNullOrWhiteSpace($originalEncodedFlags)) {
    $encodedFlags += $originalEncodedFlags -split [regex]::Escape($separator)
}
$encodedFlags += "--remap-path-prefix=$repoRoot=<workspace>"
if (-not [string]::IsNullOrWhiteSpace($env:USERPROFILE)) {
    $encodedFlags += "--remap-path-prefix=$($env:USERPROFILE)=<user-profile>"
}

$originalProductIcon = $env:INPUTFLOW_PRODUCT_ICON
Push-Location $repoRoot
try {
    try {
        $env:CARGO_ENCODED_RUSTFLAGS = $encodedFlags -join $separator
        if ($UseProductIcon) {
            $env:INPUTFLOW_PRODUCT_ICON = '1'
            Write-Host '==> Product icon: installer\InputFlow.ico'
        }
        else {
            Remove-Item Env:INPUTFLOW_PRODUCT_ICON -ErrorAction SilentlyContinue
            Write-Host '==> Product icon: system default'
        }
        Invoke-Checked { cargo build -p inputflow-agent --release --locked } 'Release agent build'
    }
    finally {
        if ($null -eq $originalEncodedFlags) {
            Remove-Item Env:CARGO_ENCODED_RUSTFLAGS -ErrorAction SilentlyContinue
        }
        else {
            $env:CARGO_ENCODED_RUSTFLAGS = $originalEncodedFlags
        }
        if ($null -eq $originalProductIcon) {
            Remove-Item Env:INPUTFLOW_PRODUCT_ICON -ErrorAction SilentlyContinue
        }
        else {
            $env:INPUTFLOW_PRODUCT_ICON = $originalProductIcon
        }
    }

    $publishStaging = Join-Path $repoRoot ('target\installer-publish-staging-' + [guid]::NewGuid().ToString('N'))
    try {
        Invoke-Checked {
            dotnet publish $settingsProject `
                -c Release -r win-x64 `
                -p:SelfContained=false `
                -p:PublishSelfContained=false `
                -p:WindowsAppSDKSelfContained=false `
                -p:PublishSingleFile=false `
                -p:PublishTrimmed=false `
                -p:PublishReadyToRun=false `
                -p:DebugSymbols=false `
                -p:DebugType=None `
                -o $publishStaging
        } 'Framework-dependent Settings publish'
        if (Test-Path -LiteralPath $publishRoot) {
            Assert-RepositoryPath $publishRoot
            Remove-Item -LiteralPath $publishRoot -Recurse -Force
        }
        Move-Item -LiteralPath $publishStaging -Destination $publishRoot
    }
    finally {
        if (Test-Path -LiteralPath $publishStaging) {
            Remove-Item -LiteralPath $publishStaging -Recurse -Force
        }
    }
}
finally {
    Pop-Location
}

$cargoTarget = if ([string]::IsNullOrWhiteSpace($env:CARGO_TARGET_DIR)) {
    Join-Path $repoRoot 'target'
} else {
    $env:CARGO_TARGET_DIR
}
$agentPath = Join-Path $cargoTarget 'release\inputflow-agent.exe'
if (-not (Test-Path -LiteralPath $agentPath -PathType Leaf)) {
    throw "Release Agent is missing: $agentPath"
}

$publishedNames = @(Get-ChildItem -LiteralPath $publishRoot -Force | ForEach-Object {
    if ($_.PSIsContainer) { throw "Settings publish contains a directory: $($_.Name)" }
    if ($_.Extension -eq '.pdb') { return }
    $_.Name
})
Assert-FileSet $PublishFiles $publishedNames 'Settings publish output'

if (Test-Path -LiteralPath $payloadRoot) {
    Assert-RepositoryPath $payloadRoot
    Remove-Item -LiteralPath $payloadRoot -Recurse -Force
}
New-Item -ItemType Directory -Path $payloadRoot -Force | Out-Null
foreach ($name in $PublishFiles) {
    Copy-Item -LiteralPath (Join-Path $publishRoot $name) -Destination (Join-Path $payloadRoot $name)
}
Copy-Item -LiteralPath $agentPath -Destination (Join-Path $payloadRoot 'inputflow-agent.exe')
Copy-Item -LiteralPath $projectLicense -Destination (Join-Path $payloadRoot 'LICENSE.txt')

$cargoMetadataJson = (& cargo metadata --format-version 1 --locked --manifest-path (Join-Path $repoRoot 'Cargo.toml')) | Out-String
if ($LASTEXITCODE -ne 0) { throw 'Unable to read locked Cargo metadata.' }
$cargoMetadata = $cargoMetadataJson | ConvertFrom-Json
foreach ($license in $RustLicenses) {
    $package = $cargoMetadata.packages | Where-Object { $_.name -eq $license.Name -and $_.version -eq $license.Version } | Select-Object -First 1
    if ($null -eq $package) { throw "Locked Rust crate is missing: $($license.Name) $($license.Version)" }
    $source = Join-Path (Split-Path -Parent $package.manifest_path) $license.File
    if (-not (Test-Path -LiteralPath $source -PathType Leaf)) {
        throw "MIT license is missing: $source"
    }
    $relative = "Licenses\Rust\$($license.Name)-$($license.Version)\$($license.File)"
    $destination = Join-Path $payloadRoot $relative
    New-Item -ItemType Directory -Path (Split-Path -Parent $destination) -Force | Out-Null
    Copy-Item -LiteralPath $source -Destination $destination
}

$assetsPath = Join-Path (Split-Path -Parent $settingsProject) 'obj\project.assets.json'
if (-not (Test-Path -LiteralPath $assetsPath -PathType Leaf)) {
    throw "NuGet assets are missing after publish: $assetsPath"
}
$assets = Get-Content -LiteralPath $assetsPath -Raw | ConvertFrom-Json
$nugetRoot = if ([string]::IsNullOrWhiteSpace($env:NUGET_PACKAGES)) {
    Join-Path $env:USERPROFILE '.nuget\packages'
} else {
    $env:NUGET_PACKAGES
}
foreach ($license in $NuGetLicenses) {
    $library = $assets.libraries.PSObject.Properties |
        Where-Object { $_.Name -eq "$($license.Id)/$($license.Version)" } |
        Select-Object -First 1
    if ($null -eq $library -or $library.Value.type -ne 'package') {
        throw "Restored NuGet package is missing: $($license.Id) $($license.Version)"
    }
    $packageSource = Join-Path $nugetRoot ($library.Value.path -replace '/', '\')
    $directory = '{0}-{1}' -f $license.Id.ToLowerInvariant(), $license.Version
    foreach ($file in $license.Files) {
        $source = Join-Path $packageSource $file
        if (-not (Test-Path -LiteralPath $source -PathType Leaf)) {
            throw "NuGet legal file is missing: $source"
        }
        $destination = Join-Path $payloadRoot "Licenses\NuGet\$directory\$file"
        New-Item -ItemType Directory -Path (Split-Path -Parent $destination) -Force | Out-Null
        Copy-Item -LiteralPath $source -Destination $destination
    }
}

$actualRelativePaths = @(Get-ChildItem -LiteralPath $payloadRoot -File -Recurse | ForEach-Object {
    $_.FullName.Substring($payloadRoot.Length + 1)
})
Assert-FileSet $ExpectedRelativePaths $actualRelativePaths 'Installer payload'

foreach ($forbidden in @(
    'coreclr.dll',
    'hostfxr.dll',
    'Microsoft.WindowsAppRuntime.dll',
    'THIRD-PARTY-NOTICES.txt'
)) {
    $found = Get-ChildItem -LiteralPath $payloadRoot -Filter $forbidden -File -Recurse
    if ($null -ne $found) { throw "Installer payload must not contain $forbidden." }
}

$runtimeConfig = Get-Content -LiteralPath (Join-Path $payloadRoot 'InputFlow.Settings.runtimeconfig.json') -Raw | ConvertFrom-Json
$framework = $runtimeConfig.runtimeOptions.framework
if ($null -eq $framework -or $framework.name -ne 'Microsoft.NETCore.App' -or $framework.version -notmatch '^10\.0\.') {
    throw 'Settings runtimeconfig must request Microsoft.NETCore.App 10.0.x.'
}

foreach ($executable in @('inputflow-agent.exe', 'InputFlow.Settings.exe')) {
    $machine = Get-PeMachine (Join-Path $payloadRoot $executable)
    if ($machine -ne 0x8664) { throw "$executable is not an x64 PE file." }
}

$privateRoots = @($repoRoot, $env:USERPROFILE) | Where-Object { -not [string]::IsNullOrWhiteSpace($_) }
foreach ($file in Get-ChildItem -LiteralPath $payloadRoot -File -Recurse) {
    $bytes = [System.IO.File]::ReadAllBytes($file.FullName)
    $utf8Text = [System.Text.Encoding]::UTF8.GetString($bytes)
    $utf16Text = [System.Text.Encoding]::Unicode.GetString($bytes)
    foreach ($root in $privateRoots) {
        $needle = $root.TrimEnd('\')
        foreach ($variant in @($needle, ($needle -replace '\\', '/'))) {
            if ($utf8Text.IndexOf($variant, [StringComparison]::OrdinalIgnoreCase) -ge 0 -or
                $utf16Text.IndexOf($variant, [StringComparison]::OrdinalIgnoreCase) -ge 0) {
                throw "Installer payload contains a private path in $($file.FullName)."
            }
        }
    }
}

if (Test-Path -LiteralPath $setupPath) {
    Assert-RepositoryPath $setupPath
    Remove-Item -LiteralPath $setupPath -Force
}
$iscc = Find-Iscc
Push-Location $repoRoot
try {
    $isccArgs = @("/DMyAppVersion=$Version")
    if ($UseProductIcon) { $isccArgs += '/DUseProductIcon' }
    Invoke-Checked { & $iscc @isccArgs $issPath } 'Inno Setup compile'
}
finally {
    Pop-Location
}
if (-not (Test-Path -LiteralPath $setupPath -PathType Leaf)) {
    throw "Inno Setup did not write the expected setup: $setupPath"
}
Write-Host "==> Installer: $setupPath"
Write-Host "==> Payload files: $($ExpectedRelativePaths.Count)"
