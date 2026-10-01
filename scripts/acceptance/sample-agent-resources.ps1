[CmdletBinding()]
param(
    [Parameter(Mandatory)]
    [int]$AgentProcessId,
    [int]$SettingsProcessId = 0,
    [ValidateRange(1, 86400)]
    [int]$DurationSeconds = 300,
    [ValidateRange(100, 60000)]
    [int]$IntervalMilliseconds = 1000,
    [Parameter(Mandatory)]
    [string]$OutputPath
)

$ErrorActionPreference = 'Stop'
$resolvedOutput = [System.IO.Path]::GetFullPath($OutputPath)
New-Item -ItemType Directory -Path (Split-Path -Parent $resolvedOutput) -Force | Out-Null
$writer = [System.IO.StreamWriter]::new($resolvedOutput, $false, [System.Text.UTF8Encoding]::new($false))
$writer.AutoFlush = $true
$writer.WriteLine('timestamp,elapsed_ms,stats_rtt_ms,observed_events,callback_samples,callback_p50_us,callback_p95_us,callback_p99_us,callback_max_us,hold_samples,hold_p50_us,hold_p95_us,hold_p99_us,hold_max_us,output_sent,output_failed,output_dropped,agent_working_set,agent_private_bytes,agent_threads,agent_handles,agent_cpu_seconds,settings_working_set,settings_private_bytes,settings_threads,settings_handles,settings_cpu_seconds,error')

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

function Cell($Value) {
    if ($null -eq $Value) { return '' }
    return ('"' + ([string]$Value).Replace('"', '""') + '"')
}

$sessionId = [System.Diagnostics.Process]::GetCurrentProcess().SessionId
$pipeName = "InputFlow.Agent.v1.$sessionId"
$pipe = [System.IO.Pipes.NamedPipeClientStream]::new('.', $pipeName, [System.IO.Pipes.PipeDirection]::InOut, [System.IO.Pipes.PipeOptions]::Asynchronous)
$pipe.Connect(3000)
$requestSequence = 0

function New-RequestId {
    $script:requestSequence += 1
    return "acceptance-$script:requestSequence"
}

$handshake = Invoke-Frame $pipe ([ordered]@{
    protocol_version = 1
    request_id = New-RequestId
    method = 'handshake'
    params = [ordered]@{ client_name = 'InputFlow.AcceptanceSampler'; client_version = '0.1.0' }
})
if ($handshake.type -ne 'success') { throw "Handshake failed: $($handshake | ConvertTo-Json -Compress -Depth 10)" }

$started = [System.Diagnostics.Stopwatch]::StartNew()
try {
    while ($started.Elapsed.TotalSeconds -lt $DurationSeconds) {
        $iteration = [System.Diagnostics.Stopwatch]::StartNew()
        $errorText = ''
        $stats = $null
        $statsRtt = [System.Diagnostics.Stopwatch]::StartNew()
        try {
            $response = Invoke-Frame $pipe ([ordered]@{
                protocol_version = 1
                request_id = New-RequestId
                method = 'get_stats'
                params = [ordered]@{}
            })
            if ($response.type -ne 'success') { throw ($response | ConvertTo-Json -Compress -Depth 10) }
            $stats = $response.result
        }
        catch {
            $errorText = $_.Exception.Message
        }
        $statsRtt.Stop()

        $agent = Get-Process -Id $AgentProcessId -ErrorAction Stop
        $settings = if ($SettingsProcessId -gt 0) { Get-Process -Id $SettingsProcessId -ErrorAction SilentlyContinue } else { $null }
        $callback = $stats.callback_latency_us
        $hold = $stats.hold_delay_us
        $cells = @(
            (Get-Date).ToString('o'),
            $started.ElapsedMilliseconds,
            [math]::Round($statsRtt.Elapsed.TotalMilliseconds, 3),
            $stats.observed_events,
            $callback.samples,
            $callback.p50,
            $callback.p95,
            $callback.p99,
            $callback.max,
            $hold.samples,
            $hold.p50,
            $hold.p95,
            $hold.p99,
            $hold.max,
            $stats.output_batches_sent,
            $stats.output_batches_failed,
            $stats.output_batches_dropped,
            $agent.WorkingSet64,
            $agent.PrivateMemorySize64,
            $agent.Threads.Count,
            $agent.HandleCount,
            [math]::Round($agent.TotalProcessorTime.TotalSeconds, 6),
            $(if ($settings) { $settings.WorkingSet64 } else { $null }),
            $(if ($settings) { $settings.PrivateMemorySize64 } else { $null }),
            $(if ($settings) { $settings.Threads.Count } else { $null }),
            $(if ($settings) { $settings.HandleCount } else { $null }),
            $(if ($settings) { [math]::Round($settings.TotalProcessorTime.TotalSeconds, 6) } else { $null }),
            $errorText
        )
        $writer.WriteLine(($cells | ForEach-Object { Cell $_ }) -join ',')

        $remaining = $IntervalMilliseconds - [int]$iteration.Elapsed.TotalMilliseconds
        if ($remaining -gt 0) { Start-Sleep -Milliseconds $remaining }
    }
}
finally {
    $pipe.Dispose()
    $writer.Dispose()
}
