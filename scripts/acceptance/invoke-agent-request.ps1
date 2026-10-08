[CmdletBinding()]
param(
    [Parameter(Mandatory)]
    [ValidateSet('pause', 'resume', 'get_status', 'get_stats', 'apply_config')]
    [string]$Method,
    [ValidateRange(0, 600000)]
    [int]$DelayMilliseconds = 0,
    [ValidateRange(0, 255)]
    [int]$WaitForVirtualKey = 0,
    [switch]$WaitForObservedEvent,
    [ValidateRange(1, 1000)]
    [int]$WaitPollMilliseconds = 5,
    [ValidateRange(1000, 300000)]
    [int]$WaitTimeoutMilliseconds = 60000,
    [string]$ConfigPath = '',
    [string]$OutputPath = ''
)

$ErrorActionPreference = 'Stop'

function Read-Exactly([System.IO.Stream]$Stream, [int]$Count) {
    $buffer = [byte[]]::new($Count)
    $offset = 0
    while ($offset -lt $Count) {
        $read = $Stream.Read($buffer, $offset, $Count - $offset)
        if ($read -eq 0) { throw 'Agent pipe closed while reading a frame.' }
        $offset += $read
    }
    return $buffer
}

function Invoke-Frame([System.IO.Pipes.NamedPipeClientStream]$Pipe, [object]$Request) {
    $json = $Request | ConvertTo-Json -Compress -Depth 20
    $payload = [System.Text.Encoding]::UTF8.GetBytes($json)
    $header = [System.BitConverter]::GetBytes([uint32]$payload.Length)
    $Pipe.Write($header, 0, $header.Length)
    $Pipe.Write($payload, 0, $payload.Length)
    $Pipe.Flush()
    $responseHeader = Read-Exactly $Pipe 4
    $length = [System.BitConverter]::ToUInt32($responseHeader, 0)
    if ($length -eq 0 -or $length -gt 1048576) { throw "Invalid frame length $length." }
    $responsePayload = Read-Exactly $Pipe ([int]$length)
    return ([System.Text.Encoding]::UTF8.GetString($responsePayload) | ConvertFrom-Json)
}

$sessionId = [System.Diagnostics.Process]::GetCurrentProcess().SessionId
$pipeName = "InputFlow.Agent.v1.$sessionId"
$pipe = [System.IO.Pipes.NamedPipeClientStream]::new(
    '.',
    $pipeName,
    [System.IO.Pipes.PipeDirection]::InOut,
    [System.IO.Pipes.PipeOptions]::Asynchronous)
$pipe.Connect(3000)

try {
    $handshake = Invoke-Frame $pipe ([ordered]@{
        protocol_version = 1
        request_id = 'acceptance-handshake'
        method = 'handshake'
        params = [ordered]@{
            client_name = 'InputFlow.AcceptanceInvoker'
            client_version = '0.1.0'
        }
    })
    if ($handshake.type -ne 'success') {
        throw "Handshake failed: $($handshake | ConvertTo-Json -Compress -Depth 10)"
    }

    $requestParams = [ordered]@{}
    if ($Method -eq 'apply_config') {
        if ([string]::IsNullOrWhiteSpace($ConfigPath)) {
            throw 'ConfigPath is required for apply_config.'
        }
        $resolvedConfig = [System.IO.Path]::GetFullPath($ConfigPath)
        $requestParams.config = [System.IO.File]::ReadAllText($resolvedConfig) | ConvertFrom-Json
    }

    if ($WaitForObservedEvent -and $WaitForVirtualKey -ne 0) {
        throw 'WaitForObservedEvent and WaitForVirtualKey cannot be used together.'
    }

    $observedBaseline = $null
    $observedDetected = $null
    $keyDetectedAt = $null
    if ($WaitForObservedEvent) {
        $baselineResponse = Invoke-Frame $pipe ([ordered]@{
            protocol_version = 1
            request_id = 'acceptance-observed-baseline'
            method = 'get_stats'
            params = [ordered]@{}
        })
        if ($baselineResponse.type -ne 'success') {
            throw "Stats baseline failed: $($baselineResponse | ConvertTo-Json -Compress -Depth 10)"
        }
        $observedBaseline = [uint64]$baselineResponse.result.observed_events
        Write-Output "OBSERVED_EVENT_ARMED baseline=$observedBaseline"
        $deadline = [DateTimeOffset]::Now.AddMilliseconds($WaitTimeoutMilliseconds)
        $poll = 0
        while ($null -eq $observedDetected) {
            if ([DateTimeOffset]::Now -ge $deadline) {
                throw "No hook-observed event arrived within $WaitTimeoutMilliseconds ms."
            }
            $poll += 1
            $statsResponse = Invoke-Frame $pipe ([ordered]@{
                protocol_version = 1
                request_id = "acceptance-observed-$poll"
                method = 'get_stats'
                params = [ordered]@{}
            })
            if ($statsResponse.type -ne 'success') {
                throw "Stats poll failed: $($statsResponse | ConvertTo-Json -Compress -Depth 10)"
            }
            $currentObserved = [uint64]$statsResponse.result.observed_events
            if ($currentObserved -gt $observedBaseline) {
                $observedDetected = $currentObserved
                $keyDetectedAt = [DateTimeOffset]::Now.ToString('o')
            }
            else {
                Start-Sleep -Milliseconds $WaitPollMilliseconds
            }
        }
    }
    if ($WaitForVirtualKey -ne 0) {
        Add-Type -TypeDefinition @'
using System.Runtime.InteropServices;

public static class InputFlowAcceptanceKeyState
{
    [DllImport("user32.dll")]
    public static extern short GetAsyncKeyState(int virtualKey);
}
'@
        $deadline = [DateTimeOffset]::Now.AddMilliseconds($WaitTimeoutMilliseconds)
        while (([InputFlowAcceptanceKeyState]::GetAsyncKeyState($WaitForVirtualKey) -band 0x8000) -ne 0) {
            if ([DateTimeOffset]::Now -ge $deadline) {
                throw "Virtual key 0x$($WaitForVirtualKey.ToString('X2')) did not return to the up state."
            }
            Start-Sleep -Milliseconds 2
        }
        while (([InputFlowAcceptanceKeyState]::GetAsyncKeyState($WaitForVirtualKey) -band 0x8000) -eq 0) {
            if ([DateTimeOffset]::Now -ge $deadline) {
                throw "Virtual key 0x$($WaitForVirtualKey.ToString('X2')) was not pressed within $WaitTimeoutMilliseconds ms."
            }
            Start-Sleep -Milliseconds 2
        }
        $keyDetectedAt = [DateTimeOffset]::Now.ToString('o')
    }

    if ($DelayMilliseconds -gt 0) {
        Start-Sleep -Milliseconds $DelayMilliseconds
    }

    $invokedAt = (Get-Date).ToString('o')
    $response = Invoke-Frame $pipe ([ordered]@{
        protocol_version = 1
        request_id = "acceptance-$Method"
        method = $Method
        params = $requestParams
    })
    $result = [ordered]@{
        connected_at = $handshake.result.server_name
        method = $Method
        delay_ms = $DelayMilliseconds
        waited_for_virtual_key = $WaitForVirtualKey
        waited_for_observed_event = [bool]$WaitForObservedEvent
        observed_baseline = $observedBaseline
        observed_detected = $observedDetected
        key_detected_at = $keyDetectedAt
        invoked_at = $invokedAt
        response = $response
    }
    $json = $result | ConvertTo-Json -Depth 20
    if (-not [string]::IsNullOrWhiteSpace($OutputPath)) {
        $resolvedOutput = [System.IO.Path]::GetFullPath($OutputPath)
        New-Item -ItemType Directory -Path (Split-Path -Parent $resolvedOutput) -Force | Out-Null
        [System.IO.File]::WriteAllText($resolvedOutput, $json, [System.Text.UTF8Encoding]::new($false))
    }
    $json
}
finally {
    $pipe.Dispose()
}
