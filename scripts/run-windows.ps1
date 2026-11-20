[CmdletBinding()]
param(
    [ValidateSet('Debug', 'Release')]
    [string]$SettingsConfiguration = 'Release',
    [string]$ConfigPath
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$repo = Split-Path -Parent $PSScriptRoot
$agent = Join-Path $repo 'target\release\inputflow-agent.exe'
$project = Join-Path $repo 'apps\settings-winui\InputFlow.Settings\InputFlow.Settings.csproj'

if (-not (Test-Path -LiteralPath $agent -PathType Leaf)) {
    throw "Release Agent is missing at '$agent'. Run scripts\build-windows.ps1 first."
}

$sessionId = [System.Diagnostics.Process]::GetCurrentProcess().SessionId
$running = @(Get-Process -Name 'inputflow-agent' -ErrorAction SilentlyContinue |
    Where-Object { $_.SessionId -eq $sessionId })
if ($running.Count -gt 0) {
    throw "An InputFlow Agent is already running in this session (PID $($running.Id -join ', ')). Exit it from its tray menu before starting a fresh development Agent."
}

Push-Location $repo
try {
    $target = & dotnet msbuild $project -nologo "-p:Configuration=$SettingsConfiguration" '-getProperty:TargetPath'
    if ($LASTEXITCODE -ne 0) { throw 'Could not resolve the Settings build output.' }
} finally {
    Pop-Location
}
$targetPath = [string]($target | Select-Object -Last 1)
if (-not [IO.Path]::IsPathRooted($targetPath)) {
    throw 'MSBuild did not return an absolute Settings output path.'
}
$settings = [IO.Path]::ChangeExtension($targetPath, '.exe')
if (-not (Test-Path -LiteralPath $settings -PathType Leaf)) {
    throw "$SettingsConfiguration Settings is missing at '$settings'. Run scripts\build-windows.ps1 first."
}

$arguments = @('--settings', ('"' + $settings + '"'))
if ($ConfigPath) {
    $absoluteConfig = $ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($ConfigPath)
    $arguments += @('--config', ('"' + $absoluteConfig + '"'))
}
$process = Start-Process -FilePath $agent -ArgumentList $arguments -WorkingDirectory (Split-Path -Parent $agent) `
    -WindowStyle Hidden -PassThru
Write-Output "Started InputFlow Agent (PID $($process.Id)). Open Settings from its tray menu."
