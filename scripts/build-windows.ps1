[CmdletBinding()]
param(
    [switch]$SkipRestore
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$repositoryRoot = Split-Path -Parent $PSScriptRoot
$solution = Join-Path $repositoryRoot 'apps\settings-winui\InputFlow.Settings.slnx'

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

Push-Location $repositoryRoot
try {
    Invoke-Checked { cargo fmt --all -- --check } 'Rust formatting check'
    Invoke-Checked { cargo test --workspace } 'Rust workspace tests'
    Invoke-Checked { cargo clippy --workspace --all-targets -- -D warnings } 'Rust Clippy'
    Invoke-Checked { cargo build -p probe-cli } 'probe-cli build'
    Invoke-Checked { cargo build -p inputflow-agent --release } 'Release agent build'

    if (-not $SkipRestore) {
        Invoke-Checked { dotnet restore $solution } 'WinUI solution restore'
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
