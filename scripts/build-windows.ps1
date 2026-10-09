[CmdletBinding()]
param(
    [switch]$SkipRestore
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$repositoryRoot = Split-Path -Parent $PSScriptRoot
$solution = Join-Path $repositoryRoot 'apps\settings-winui\InputFlow.Settings.slnx'

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
        throw "$Description failed with exit code $LASTEXITCODE"
    }
}

function Invoke-ReleaseAgentBuild {
    $separator = [string][char]0x1f
    $originalEncodedFlags = $env:CARGO_ENCODED_RUSTFLAGS
    $encodedFlags = @()
    if (-not [string]::IsNullOrWhiteSpace($originalEncodedFlags)) {
        $encodedFlags += $originalEncodedFlags -split [regex]::Escape($separator)
    }

    # Rust keeps source locations used by panic diagnostics in optimized binaries.
    # Keep those diagnostics useful without publishing the builder's workspace or
    # user-profile path in the portable package.
    $encodedFlags += "--remap-path-prefix=$repositoryRoot=<workspace>"
    if (-not [string]::IsNullOrWhiteSpace($env:USERPROFILE)) {
        $encodedFlags += "--remap-path-prefix=$($env:USERPROFILE)=<user-profile>"
    }

    try {
        $env:CARGO_ENCODED_RUSTFLAGS = $encodedFlags -join $separator
        cargo build -p inputflow-agent --release
    }
    finally {
        if ($null -eq $originalEncodedFlags) {
            Remove-Item Env:CARGO_ENCODED_RUSTFLAGS -ErrorAction SilentlyContinue
        }
        else {
            $env:CARGO_ENCODED_RUSTFLAGS = $originalEncodedFlags
        }
    }
}

Initialize-MsvcEnvironment

Push-Location $repositoryRoot
try {
    Invoke-Checked { cargo fmt --all -- --check } 'Rust formatting check'
    Invoke-Checked { cargo test --workspace } 'Rust workspace tests'
    Invoke-Checked { cargo clippy --workspace --all-targets -- -D warnings } 'Rust Clippy'
    Invoke-Checked { cargo build -p probe-cli } 'probe-cli build'
    Invoke-Checked { Invoke-ReleaseAgentBuild } 'Release agent build with private-path remapping'

    if (-not $SkipRestore) {
        Invoke-Checked { dotnet restore $solution -p:Configuration=Debug } 'WinUI Debug solution restore'
        # Release enables trimming and ReadyToRun in the application project,
        # which require additional locked runtime/tool packs on a clean machine.
        Invoke-Checked { dotnet restore $solution -p:Configuration=Release } 'WinUI Release solution restore'
    }
    Invoke-Checked { dotnet build $solution -c Debug --no-restore } 'WinUI Debug build'
    Invoke-Checked { dotnet build $solution -c Release --no-restore } 'WinUI Release build'
    Invoke-Checked {
        dotnet run --project '.\apps\settings-winui\InputFlow.Protocol.ContractTests\InputFlow.Protocol.ContractTests.csproj' -c Debug --no-build
    } 'C# protocol contract tests'
    Invoke-Checked {
        dotnet run --project '.\apps\settings-winui\InputFlow.Settings.Core.Tests\InputFlow.Settings.Core.Tests.csproj' -c Debug --no-build
    } 'C# settings core tests'
}
finally {
    Pop-Location
}
