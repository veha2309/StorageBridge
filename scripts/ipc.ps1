function Invoke-Bridge {
    param([Parameter(Mandatory)][string]$Method, [hashtable]$Params = @{}, [string]$Instance = '')
    $sid = [System.Security.Principal.WindowsIdentity]::GetCurrent().User.Value
    if ($Instance -and $Instance -notmatch '^[a-zA-Z0-9-]{1,32}$') { throw 'Invalid test instance name' }
    $pipeName = "StorageBridge-v1-$sid"
    if ($Instance) { $pipeName += "-test-$Instance" }
    $pipe = [System.IO.Pipes.NamedPipeClientStream]::new('.', $pipeName, [System.IO.Pipes.PipeDirection]::InOut)
    try {
        $pipe.Connect(3000)
        $writer = [System.IO.BinaryWriter]::new($pipe, [System.Text.Encoding]::UTF8, $true)
        $reader = [System.IO.BinaryReader]::new($pipe, [System.Text.Encoding]::UTF8, $true)
        $body = [System.Text.Encoding]::UTF8.GetBytes((@{ version = 1; method = $Method; params = $Params } | ConvertTo-Json -Depth 8 -Compress))
        $writer.Write([uint32]$body.Length)
        $writer.Write($body)
        $writer.Flush()
        $length = $reader.ReadUInt32()
        if ($length -gt 1048576) { throw 'Invalid IPC frame length' }
        $reply = [System.Text.Encoding]::UTF8.GetString($reader.ReadBytes($length)) | ConvertFrom-Json
        if (-not $reply.ok) { throw $reply.error }
        return $reply.result
    } finally { $pipe.Dispose() }
}
