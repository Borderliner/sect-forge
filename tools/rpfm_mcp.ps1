# Minimal MCP-over-HTTP client for rpfm_server (Streamable HTTP transport).
# Usage:  . .\rpfm_mcp.ps1 ; Start-Rpfm ; Mcp-Tool set_game @{ game = 'three_kingdoms' }

$script:McpUrl = 'http://127.0.0.1:45127/mcp'
$script:McpSession = $null

function Start-Rpfm {
    if (-not (Get-Process rpfm_server -ErrorAction SilentlyContinue)) {
        Start-Process -FilePath 'C:\Users\Touraj\Apps\RPFM\rpfm_server.exe' -WindowStyle Hidden
    }
    for ($i = 0; $i -lt 40; $i++) {
        if (Test-NetConnection -ComputerName 127.0.0.1 -Port 45127 -InformationLevel Quiet -WarningAction SilentlyContinue) { return $true }
        Start-Sleep -Milliseconds 250
    }
    throw 'rpfm_server did not open port 45127'
}

function Invoke-McpRaw {
    param([hashtable]$Body, [switch]$NoSession)
    $headers = @{ 'Accept' = 'application/json, text/event-stream' }
    if ($script:McpSession -and -not $NoSession) { $headers['Mcp-Session-Id'] = $script:McpSession }
    $json = $Body | ConvertTo-Json -Depth 30 -Compress
    $resp = Invoke-WebRequest -Uri $script:McpUrl -Method Post -Headers $headers -ContentType 'application/json' -Body $json -UseBasicParsing -TimeoutSec 120
    $sid = $resp.Headers['Mcp-Session-Id']
    if ($sid -and -not $script:McpSession) { $script:McpSession = @($sid)[0] }
    $text = $resp.Content
    # Streamable HTTP answers as SSE. Data frames may be preceded by id:/retry: lines,
    # so pick the last data: line that actually holds JSON.
    $jsonLines = @($text -split "`r?`n" | Where-Object { $_ -like 'data: *' -and $_.Substring(6).TrimStart().StartsWith('{') })
    if ($jsonLines.Count -gt 0) { $text = $jsonLines[-1].Substring(6).Trim() }
    if ([string]::IsNullOrWhiteSpace($text)) { return $null }
    return $text | ConvertFrom-Json
}

function Connect-Mcp {
    if (-not $script:McpSession) {
        $init = @{
            jsonrpc = '2.0'; id = 1; method = 'initialize'
            params = @{ protocolVersion = '2025-06-18'; capabilities = @{}; clientInfo = @{ name = 'opencode'; version = '1.0' } }
        }
        $null = Invoke-McpRaw -Body $init -NoSession
        $notify = @{ jsonrpc = '2.0'; method = 'notifications/initialized'; params = @{} }
        try { $null = Invoke-McpRaw -Body $notify } catch { }
    }
}

function Mcp-Tool {
    param([string]$Name, [hashtable]$Arguments = @{})
    Connect-Mcp
    $body = @{ jsonrpc = '2.0'; id = (Get-Random); method = 'tools/call'; params = @{ name = $Name; arguments = $Arguments } }
    $r = Invoke-McpRaw -Body $body
    if ($r.error) { throw ("MCP error: " + ($r.error | ConvertTo-Json -Depth 10 -Compress)) }
    return $r.result
}

function Mcp-ListTools {
    Connect-Mcp
    $body = @{ jsonrpc = '2.0'; id = (Get-Random); method = 'tools/list'; params = @{} }
    (Invoke-McpRaw -Body $body).result.tools
}

