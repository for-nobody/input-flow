[CmdletBinding()]
param(
    [Parameter(Mandatory)]
    [string]$FromTitle,
    [Parameter(Mandatory)]
    [string]$ToTitle,
    [Parameter(Mandatory)]
    [string]$OutputPath,
    [Parameter(Mandatory)]
    [string]$TracePath,
    [ValidateRange(1000, 300000)]
    [int]$TimeoutMilliseconds = 60000,
    [ValidateRange(5, 1000)]
    [int]$PollMilliseconds = 10
)

$ErrorActionPreference = 'Stop'

Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
using System.Text;

public static class InputFlowForegroundWindow
{
    [DllImport("user32.dll")]
    private static extern IntPtr GetForegroundWindow();

    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    private static extern int GetWindowText(IntPtr window, StringBuilder text, int capacity);

    public static string Title()
    {
        IntPtr window = GetForegroundWindow();
        if (window == IntPtr.Zero) return string.Empty;
        var text = new StringBuilder(1024);
        GetWindowText(window, text, text.Capacity);
        return text.ToString();
    }
}
'@

$resolvedOutput = [System.IO.Path]::GetFullPath($OutputPath)
$resolvedTrace = [System.IO.Path]::GetFullPath($TracePath)
$requestScript = Join-Path $PSScriptRoot 'invoke-agent-request.ps1'
$deadline = [DateTimeOffset]::Now.AddMilliseconds($TimeoutMilliseconds)
$armed = $false
$trace = [System.Collections.Generic.List[string]]::new()
$trace.Add("started_at=$([DateTimeOffset]::Now.ToString('O'))")
$trace.Add("from_title=$FromTitle")
$trace.Add("to_title=$ToTitle")

while ([DateTimeOffset]::Now -lt $deadline) {
    $title = [InputFlowForegroundWindow]::Title()
    if (-not $armed -and $title -eq $FromTitle) {
        $armed = $true
        $trace.Add("armed_at=$([DateTimeOffset]::Now.ToString('O'))")
    }
    elseif ($armed -and $title -eq $ToTitle) {
        $trace.Add("transition_at=$([DateTimeOffset]::Now.ToString('O'))")
        $trace.Add("pre_request_title=$([InputFlowForegroundWindow]::Title())")
        $trace | Set-Content -LiteralPath $resolvedTrace -Encoding UTF8
        $response = & powershell.exe -NoProfile -ExecutionPolicy Bypass -File $requestScript `
            -Method pause -OutputPath $resolvedOutput
        $exitCode = $LASTEXITCODE
        $trace.Add("completed_at=$([DateTimeOffset]::Now.ToString('O'))")
        $trace.Add("post_response_title=$([InputFlowForegroundWindow]::Title())")
        $trace | Set-Content -LiteralPath $resolvedTrace -Encoding UTF8
        $response | Write-Output
        exit $exitCode
    }
    Start-Sleep -Milliseconds $PollMilliseconds
}

$trace.Add("timeout_at=$([DateTimeOffset]::Now.ToString('O'))")
$trace | Set-Content -LiteralPath $resolvedTrace -Encoding UTF8
throw "Foreground transition '$FromTitle' -> '$ToTitle' was not observed within ${TimeoutMilliseconds}ms."
