[CmdletBinding()]
param(
    [Parameter(Mandatory)]
    [ValidateSet('pause', 'resume', 'get_status', 'get_stats')]
    [string]$Method,
    [ValidateRange(0, 600000)]
    [int]$DelayMilliseconds = 0,
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

    if ($DelayMilliseconds -gt 0) {
        Start-Sleep -Milliseconds $DelayMilliseconds
    }

    $invokedAt = (Get-Date).ToString('o')
    $response = Invoke-Frame $pipe ([ordered]@{
        protocol_version = 1
        request_id = "acceptance-$Method"
        method = $Method
        params = [ordered]@{}
    })
    $result = [ordered]@{
        connected_at = $handshake.result.server_name
        method = $Method
        delay_ms = $DelayMilliseconds
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
