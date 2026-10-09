[CmdletBinding()]
param(
    [string]$OutputRoot = '',
    [switch]$SkipBuild,
    [switch]$SkipRestore,
    [switch]$RequireClean,
    [switch]$Force
)

$ErrorActionPreference = 'Stop'
$repoRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
if ([string]::IsNullOrWhiteSpace($OutputRoot)) {
    $OutputRoot = Join-Path $repoRoot 'target\distribution'
}
$resolvedOutputRoot = [System.IO.Path]::GetFullPath($OutputRoot)
$workspacePrefix = $repoRoot.TrimEnd('\') + '\'
if (-not $resolvedOutputRoot.StartsWith($workspacePrefix, [StringComparison]::OrdinalIgnoreCase)) {
    throw 'OutputRoot must remain inside the repository workspace.'
}

$agentManifest = Join-Path $repoRoot 'apps\inputflow-agent\Cargo.toml'
$settingsProject = Join-Path $repoRoot 'apps\settings-winui\InputFlow.Settings\InputFlow.Settings.csproj'
$settingsAppManifest = Join-Path $repoRoot 'apps\settings-winui\InputFlow.Settings\app.manifest'
$settingsPackageManifest = Join-Path $repoRoot 'apps\settings-winui\InputFlow.Settings\Package.appxmanifest'
$protocolConstants = Join-Path $repoRoot 'apps\settings-winui\InputFlow.Protocol\ProtocolConstants.cs'
$projectLicense = Join-Path $repoRoot 'LICENSE'
$thirdPartyNotices = Join-Path $repoRoot 'THIRD-PARTY-NOTICES.txt'

foreach ($requiredInput in @(
    $agentManifest,
    $settingsProject,
    $settingsAppManifest,
    $settingsPackageManifest,
    $protocolConstants,
    $projectLicense,
    $thirdPartyNotices
)) {
    if (-not (Test-Path -LiteralPath $requiredInput -PathType Leaf)) {
        throw "Required release input is missing: $requiredInput"
    }
}

$agentVersionMatch = Select-String -LiteralPath $agentManifest -Pattern '^version\s*=\s*"([^"]+)"' | Select-Object -First 1
if ($null -eq $agentVersionMatch) { throw 'Unable to read the Agent version.' }
$agentVersion = $agentVersionMatch.Matches[0].Groups[1].Value
[xml]$settingsXml = Get-Content -LiteralPath $settingsProject -Raw
$settingsVersionNode = $settingsXml.SelectSingleNode('/Project/PropertyGroup/Version')
if ($null -eq $settingsVersionNode) { throw 'Unable to read the Settings version.' }
$settingsVersion = $settingsVersionNode.InnerText.Trim()
$windowsAppSdkNode = $settingsXml.SelectSingleNode('/Project/ItemGroup/PackageReference[@Include="Microsoft.WindowsAppSDK"]/@Version')
if ($null -eq $windowsAppSdkNode) { throw 'Unable to read the Windows App SDK version.' }
$windowsAppSdkVersion = $windowsAppSdkNode.Value.Trim()
$targetFrameworkNode = $settingsXml.SelectSingleNode('/Project/PropertyGroup/TargetFramework')
$targetPlatformMinVersionNode = $settingsXml.SelectSingleNode('/Project/PropertyGroup/TargetPlatformMinVersion')
if ($null -eq $targetFrameworkNode -or $null -eq $targetPlatformMinVersionNode) {
    throw 'Unable to read the Settings target framework or minimum platform version.'
}
$targetFramework = $targetFrameworkNode.InnerText.Trim()
$targetPlatformMinVersion = $targetPlatformMinVersionNode.InnerText.Trim()
if ($agentVersion -ne $settingsVersion) {
    throw "Version mismatch: Agent=$agentVersion Settings=$settingsVersion"
}
$version = $agentVersion
if ($version -notmatch '^\d+\.\d+\.\d+$') {
    throw "Release version must be a three-component SemVer without a prefix: $version"
}
$numericVersion = "$version.0"
$releaseNotes = Join-Path $repoRoot "docs\releases\V$version.md"
if (-not (Test-Path -LiteralPath $releaseNotes -PathType Leaf)) {
    throw "Versioned release notes are missing: $releaseNotes"
}

$protocolVersionMatch = Select-String -LiteralPath $protocolConstants -Pattern 'ProductVersion\s*=\s*"([^"]+)"' | Select-Object -First 1
if ($null -eq $protocolVersionMatch -or $protocolVersionMatch.Matches[0].Groups[1].Value -ne $version) {
    throw 'InputFlow.Protocol product version does not match the Agent and Settings version.'
}
$appManifestVersionMatch = Select-String -LiteralPath $settingsAppManifest -Pattern 'assemblyIdentity\s+version="([^"]+)"' | Select-Object -First 1
if ($null -eq $appManifestVersionMatch -or $appManifestVersionMatch.Matches[0].Groups[1].Value -ne $numericVersion) {
    throw "Settings app.manifest version must be $numericVersion."
}
$packageManifestVersionMatch = Select-String -LiteralPath $settingsPackageManifest -Pattern '^\s+Version="([^"]+)"\s*/>' | Select-Object -First 1
if ($null -eq $packageManifestVersionMatch -or $packageManifestVersionMatch.Matches[0].Groups[1].Value -ne $numericVersion) {
    throw "Settings Package.appxmanifest version must be $numericVersion."
}

$cargoMetadataJson = (& cargo metadata --format-version 1 --locked) | Out-String
if ($LASTEXITCODE -ne 0) { throw 'Unable to read locked Cargo metadata.' }
$cargoMetadata = $cargoMetadataJson | ConvertFrom-Json
$workspacePackages = @($cargoMetadata.packages | Where-Object { $cargoMetadata.workspace_members -contains $_.id })
$mismatchedPackages = @($workspacePackages | Where-Object { $_.version -ne $version })
if ($mismatchedPackages.Count -ne 0) {
    throw "Rust workspace version mismatch: $($mismatchedPackages.name -join ', ')"
}

$commit = (& git -C $repoRoot rev-parse HEAD).Trim()
if ($LASTEXITCODE -ne 0) { throw 'Unable to read the Git commit.' }
$dirty = -not [string]::IsNullOrWhiteSpace((& git -C $repoRoot status --porcelain | Out-String))
if ($RequireClean -and $dirty) {
    throw 'The RC package requires a clean Git worktree.'
}

$packageName = "InputFlow-$version-win-x64"
$packageDirectory = Join-Path $resolvedOutputRoot $packageName
$zipPath = Join-Path $resolvedOutputRoot "$packageName.zip"
$checksumPath = Join-Path $resolvedOutputRoot 'SHA256SUMS.txt'
$stagingRoot = Join-Path $resolvedOutputRoot ('.staging-' + [guid]::NewGuid().ToString('N'))
$stagingPackage = Join-Path $stagingRoot $packageName

function Assert-SafeOutputTarget([string]$Path) {
    $full = [System.IO.Path]::GetFullPath($Path)
    $rootPrefix = $resolvedOutputRoot.TrimEnd('\') + '\'
    if (-not $full.StartsWith($rootPrefix, [StringComparison]::OrdinalIgnoreCase)) {
        throw "Refusing to modify a path outside OutputRoot: $full"
    }
}

function Remove-ExistingOutput([string]$Path) {
    if (-not (Test-Path -LiteralPath $Path)) { return }
    if (-not $Force) { throw "Output already exists; use -Force to replace it: $Path" }
    Assert-SafeOutputTarget $Path
    Remove-Item -LiteralPath $Path -Recurse -Force
}

function Invoke-Checked([string]$Label, [scriptblock]$Command) {
    & $Command
    if ($LASTEXITCODE -ne 0) { throw "$Label failed with exit code $LASTEXITCODE." }
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

function Copy-LegalFiles([string]$SourceDirectory, [string]$DestinationDirectory) {
    $legalFiles = @(Get-ChildItem -LiteralPath $SourceDirectory -File -ErrorAction Stop | Where-Object {
        $_.Name -match '^(LICENSE|LICENCE|UNLICENSE|COPYRIGHT|NOTICE|THIRDPARTYNOTICES|THIRD-PARTY-NOTICES)([-.].*)?$'
    })
    if ($legalFiles.Count -eq 0) { return $false }
    New-Item -ItemType Directory -Path $DestinationDirectory -Force | Out-Null
    foreach ($legalFile in $legalFiles) {
        Copy-Item -LiteralPath $legalFile.FullName -Destination (Join-Path $DestinationDirectory $legalFile.Name)
    }
    return $true
}

function Find-EmbeddedPrivatePaths([string]$Root, [string[]]$PrivateRoots) {
    $needles = @($PrivateRoots |
        Where-Object { -not [string]::IsNullOrWhiteSpace($_) } |
        ForEach-Object { $_.TrimEnd('\', '/') } |
        Select-Object -Unique)
    $matches = @()
    foreach ($file in Get-ChildItem -LiteralPath $Root -File -Recurse) {
        $bytes = [System.IO.File]::ReadAllBytes($file.FullName)
        $utf8Text = [System.Text.Encoding]::UTF8.GetString($bytes)
        $utf16Text = [System.Text.Encoding]::Unicode.GetString($bytes)
        foreach ($needle in $needles) {
            $variants = @($needle, ($needle -replace '\\', '/')) | Select-Object -Unique
            foreach ($variant in $variants) {
                if ($utf8Text.IndexOf($variant, [StringComparison]::OrdinalIgnoreCase) -ge 0 -or
                    $utf16Text.IndexOf($variant, [StringComparison]::OrdinalIgnoreCase) -ge 0) {
                    $matches += $file.FullName
                    break
                }
            }
            if ($matches.Count -ne 0 -and $matches[-1] -eq $file.FullName) { break }
        }
    }
    return @($matches | Select-Object -Unique)
}

New-Item -ItemType Directory -Path $resolvedOutputRoot -Force | Out-Null
Remove-ExistingOutput $packageDirectory
Remove-ExistingOutput $zipPath
Remove-ExistingOutput $checksumPath

try {
    New-Item -ItemType Directory -Path $stagingPackage -Force | Out-Null

    if (-not $SkipBuild) {
        $buildArgs = @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', (Join-Path $repoRoot 'scripts\build-windows.ps1'))
        if ($SkipRestore) { $buildArgs += '-SkipRestore' }
        Invoke-Checked 'Complete Windows build' { & powershell.exe @buildArgs }
    }

    $publishArgs = @(
        'publish', $settingsProject,
        '-c', 'Release',
        '-r', 'win-x64',
        '-p:PublishProfile=win-x64',
        '-p:DebugSymbols=false',
        '-p:DebugType=None',
        ('-p:PublishDir=' + ($stagingPackage.TrimEnd('\') + '\'))
    )
    if ($SkipRestore) { $publishArgs += '--no-restore' }
    Invoke-Checked 'Settings self-contained publish' { & dotnet @publishArgs }

    $agentPath = Join-Path $repoRoot 'target\release\inputflow-agent.exe'
    if (-not (Test-Path -LiteralPath $agentPath -PathType Leaf)) {
        throw "Release Agent is missing: $agentPath"
    }
    Copy-Item -LiteralPath $agentPath -Destination (Join-Path $stagingPackage 'inputflow-agent.exe')

    foreach ($name in @(
        'Manage-InputFlow-Autostart.ps1',
        'Enable-InputFlow-Autostart.cmd',
        'Disable-InputFlow-Autostart.cmd',
        'Get-InputFlow-Autostart-Status.cmd'
    )) {
        Copy-Item -LiteralPath (Join-Path $repoRoot "scripts\distribution\$name") -Destination $stagingPackage
    }
    Copy-Item -LiteralPath (Join-Path $repoRoot 'docs\guides\USER_GUIDE.md') -Destination (Join-Path $stagingPackage 'README.md')
    Copy-Item -LiteralPath $releaseNotes -Destination (Join-Path $stagingPackage 'RELEASE_NOTES.md')
    Copy-Item -LiteralPath $projectLicense -Destination (Join-Path $stagingPackage 'LICENSE.txt')
    Copy-Item -LiteralPath $thirdPartyNotices -Destination (Join-Path $stagingPackage 'THIRD-PARTY-NOTICES.txt')

    $licensesRoot = Join-Path $stagingPackage 'Licenses'
    foreach ($package in @($cargoMetadata.packages | Where-Object { $null -ne $_.source })) {
        $packageSource = Split-Path -Parent $package.manifest_path
        $packageDirectoryName = ("$($package.name)-$($package.version)" -replace '[^A-Za-z0-9._-]', '_')
        $packageLicenseDirectory = Join-Path (Join-Path $licensesRoot 'Rust') $packageDirectoryName
        if (-not (Copy-LegalFiles $packageSource $packageLicenseDirectory)) {
            throw "Locked Rust dependency has no local legal file: $($package.name) $($package.version)"
        }
    }

    $dotnetCommand = Get-Command dotnet -ErrorAction Stop
    $dotnetRoot = Split-Path -Parent $dotnetCommand.Source
    if (-not (Copy-LegalFiles $dotnetRoot (Join-Path $licensesRoot 'DotNet'))) {
        throw "The .NET installation has no redistributable legal files: $dotnetRoot"
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
    $windowsAppSdkLegalCopied = $false
    foreach ($library in $assets.libraries.PSObject.Properties) {
        if ($library.Value.type -ne 'package') { continue }
        $packageSource = Join-Path $nugetRoot ($library.Value.path -replace '/', '\')
        if (-not (Test-Path -LiteralPath $packageSource -PathType Container)) {
            throw "Restored NuGet package directory is missing: $packageSource"
        }
        $packageDirectoryName = ($library.Name -replace '/', '-' -replace '[^A-Za-z0-9._-]', '_')
        $packageLicenseDirectory = Join-Path (Join-Path $licensesRoot 'NuGet') $packageDirectoryName
        $copied = Copy-LegalFiles $packageSource $packageLicenseDirectory
        if ($library.Name -match '^Microsoft\.WindowsAppSDK/' -and $copied) {
            $windowsAppSdkLegalCopied = $true
        }
    }
    if (-not $windowsAppSdkLegalCopied) {
        throw 'Microsoft.WindowsAppSDK license and notice files were not copied from the restored package.'
    }

    # Project-reference PDBs are emitted even when the root publish profile has
    # DebugSymbols=false. They are not part of the public binary package.
    Get-ChildItem -LiteralPath $stagingPackage -Filter '*.pdb' -File -Recurse |
        ForEach-Object { Remove-Item -LiteralPath $_.FullName -Force }

    $required = @(
        'inputflow-agent.exe',
        'InputFlow.Settings.exe',
        'InputFlow.Settings.runtimeconfig.json',
        'README.md',
        'RELEASE_NOTES.md',
        'LICENSE.txt',
        'THIRD-PARTY-NOTICES.txt',
        'coreclr.dll',
        'hostfxr.dll',
        'Microsoft.WindowsAppRuntime.dll',
        'Microsoft.WindowsAppRuntime.Bootstrap.dll'
    )
    foreach ($name in $required) {
        if (-not (Test-Path -LiteralPath (Join-Path $stagingPackage $name) -PathType Leaf)) {
            throw "Required package file is missing: $name"
        }
    }
    if ((Get-PeMachine (Join-Path $stagingPackage 'inputflow-agent.exe')) -ne 0x8664 -or
        (Get-PeMachine (Join-Path $stagingPackage 'InputFlow.Settings.exe')) -ne 0x8664) {
        throw 'Both product executables must be PE x64 (0x8664).'
    }
    $settingsFileVersion = (Get-Item -LiteralPath (Join-Path $stagingPackage 'InputFlow.Settings.exe')).VersionInfo.FileVersion
    if ($settingsFileVersion -ne $numericVersion) {
        throw "Published Settings file version must be $numericVersion, found $settingsFileVersion."
    }
    $forbidden = @(Get-ChildItem -LiteralPath $stagingPackage -File -Recurse | Where-Object {
        $_.Extension -ieq '.pdb' -or $_.Name -ieq 'config.json' -or $_.Name -ieq 'agent.log'
    })
    if ($forbidden.Count -ne 0) {
        throw "Forbidden private or debug files entered the package: $($forbidden.FullName -join ', ')"
    }
    $embeddedPrivatePaths = @(Find-EmbeddedPrivatePaths $stagingPackage @($repoRoot, $env:USERPROFILE))
    if ($embeddedPrivatePaths.Count -ne 0) {
        throw "Builder-private absolute paths entered the package: $($embeddedPrivatePaths -join ', ')"
    }

    $dirty = -not [string]::IsNullOrWhiteSpace((& git -C $repoRoot status --porcelain | Out-String))
    if ($RequireClean -and $dirty) {
        throw 'Tracked release inputs changed while the RC package was being built.'
    }
    $fileCountBeforeManifest = @(Get-ChildItem -LiteralPath $stagingPackage -File -Recurse).Count
    $manifest = [ordered]@{
        package = $packageName
        version = $version
        architecture = 'x64'
        git_commit = $commit
        worktree_dirty = $dirty
        created_at = [DateTimeOffset]::Now.ToString('o')
        release_tag = "v$version"
        release_channel = 'pre-release'
        project_license = 'MIT'
        legal_notices = 'THIRD-PARTY-NOTICES.txt and Licenses/'
        deployment = 'unpackaged-portable-directory'
        dotnet_self_contained = $true
        windows_app_sdk_self_contained = $true
        windows_app_sdk_version = $windowsAppSdkVersion
        external_runtime_requirement = 'Microsoft Visual C++ Redistributable 2015-2022 x64 (VCRUNTIME140.dll)'
        target_framework = $targetFramework
        target_platform_min_version = $targetPlatformMinVersion
        publish_trimmed = $false
        publish_single_file = $false
        build_paths_redacted = $true
        file_count_before_manifest = $fileCountBeforeManifest
    }
    [System.IO.File]::WriteAllText(
        (Join-Path $stagingPackage 'package-manifest.json'),
        ($manifest | ConvertTo-Json -Depth 5),
        [System.Text.UTF8Encoding]::new($false))

    Move-Item -LiteralPath $stagingPackage -Destination $packageDirectory
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    [System.IO.Compression.ZipFile]::CreateFromDirectory(
        $packageDirectory,
        $zipPath,
        [System.IO.Compression.CompressionLevel]::Optimal,
        $true)
    $zipHash = (Get-FileHash -LiteralPath $zipPath -Algorithm SHA256).Hash
    $checksumLine = "$zipHash  $([System.IO.Path]::GetFileName($zipPath))`r`n"
    [System.IO.File]::WriteAllText($checksumPath, $checksumLine, [System.Text.UTF8Encoding]::new($false))

    [ordered]@{
        package_directory = $packageDirectory
        zip_path = $zipPath
        checksum_path = $checksumPath
        zip_sha256 = $zipHash
        package_bytes = (Get-ChildItem -LiteralPath $packageDirectory -File -Recurse | Measure-Object Length -Sum).Sum
        zip_bytes = (Get-Item -LiteralPath $zipPath).Length
    } | ConvertTo-Json
}
finally {
    if (Test-Path -LiteralPath $stagingRoot) {
        Assert-SafeOutputTarget $stagingRoot
        Remove-Item -LiteralPath $stagingRoot -Recurse -Force
    }
}
