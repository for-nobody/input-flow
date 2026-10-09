[CmdletBinding()]
param(
    [string]$OutputRoot = '',
    [switch]$SkipBuild,
    [switch]$SkipRestore,
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
$agentVersionMatch = Select-String -LiteralPath $agentManifest -Pattern '^version\s*=\s*"([^"]+)"' | Select-Object -First 1
if ($null -eq $agentVersionMatch) { throw 'Unable to read the Agent version.' }
$agentVersion = $agentVersionMatch.Matches[0].Groups[1].Value
[xml]$settingsXml = Get-Content -LiteralPath $settingsProject -Raw
$settingsVersionNode = $settingsXml.SelectSingleNode('/Project/PropertyGroup/Version')
if ($null -eq $settingsVersionNode) { throw 'Unable to read the Settings version.' }
$settingsVersion = $settingsVersionNode.InnerText.Trim()
if ($agentVersion -ne $settingsVersion) {
    throw "Version mismatch: Agent=$agentVersion Settings=$settingsVersion"
}
$version = $agentVersion
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

    # Project-reference PDBs are emitted even when the root publish profile has
    # DebugSymbols=false. They are not part of the public binary package.
    Get-ChildItem -LiteralPath $stagingPackage -Filter '*.pdb' -File -Recurse |
        ForEach-Object { Remove-Item -LiteralPath $_.FullName -Force }

    $required = @(
        'inputflow-agent.exe',
        'InputFlow.Settings.exe',
        'InputFlow.Settings.runtimeconfig.json',
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
    $forbidden = @(Get-ChildItem -LiteralPath $stagingPackage -File -Recurse | Where-Object {
        $_.Extension -ieq '.pdb' -or $_.Name -ieq 'config.json' -or $_.Name -ieq 'agent.log'
    })
    if ($forbidden.Count -ne 0) {
        throw "Forbidden private or debug files entered the package: $($forbidden.FullName -join ', ')"
    }

    $commit = (& git -C $repoRoot rev-parse HEAD).Trim()
    if ($LASTEXITCODE -ne 0) { throw 'Unable to read the Git commit.' }
    $dirty = -not [string]::IsNullOrWhiteSpace((& git -C $repoRoot status --porcelain | Out-String))
    $fileCountBeforeManifest = @(Get-ChildItem -LiteralPath $stagingPackage -File -Recurse).Count
    $manifest = [ordered]@{
        package = $packageName
        version = $version
        architecture = 'x64'
        git_commit = $commit
        worktree_dirty = $dirty
        created_at = [DateTimeOffset]::Now.ToString('o')
        deployment = 'unpackaged-portable-directory'
        dotnet_self_contained = $true
        windows_app_sdk_self_contained = $true
        windows_app_sdk_version = '2.5.1'
        external_runtime_requirement = 'Microsoft Visual C++ Redistributable 2015-2022 x64 (VCRUNTIME140.dll)'
        target_framework = 'net10.0-windows10.0.26100.0'
        target_platform_min_version = '10.0.17763.0'
        publish_trimmed = $false
        publish_single_file = $false
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
