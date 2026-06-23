param(
    [string[]]$Charsets = @('unicode', 'unicodeFFFE', 'utf-16', 'utf-16BE', 'utf-8')
)

$ErrorActionPreference = 'Stop'

foreach ($charset in $Charsets) {
    $safeName = $charset -replace '[^A-Za-z0-9]', '_'
    $path = Join-Path $env:TEMP "ado_charset_$safeName.xml"
    Remove-Item -LiteralPath $path -Force -ErrorAction SilentlyContinue

    $rs = New-Object -ComObject ADODB.Recordset
    $rs.CursorLocation = 3
    $rs.Fields.Append('ID', 3)
    $rs.Fields.Append('TXT', 202, 120, 32)
    $rs.Open()
    $rs.AddNew()
    $rs.Fields.Item('ID').Value = 1
    $rs.Fields.Item('TXT').Value = [string]([char]0xD55C) + [string]([char]0xAE00) + ' ' + [string]([char]0x20AC)
    $rs.Update()

    $stream = New-Object -ComObject ADODB.Stream
    $stream.Type = 2
    $stream.Charset = $charset
    $stream.Open()

    $result = [ordered]@{
        charset = $charset
        save = 'ok'
        file = 'skip'
        reopen = 'skip'
        first_bytes = ''
        error_hresult = ''
        error_message = ''
    }

    try {
        $rs.Save($stream, 1)
    } catch {
        $result.save = 'fail'
        $result.error_hresult = $_.Exception.HResult
        $result.error_message = $_.Exception.Message
    }

    if ($result.save -eq 'ok') {
        try {
            $stream.SaveToFile($path, 2)
            $result.file = 'ok'
        } catch {
            $result.file = 'fail'
            $result.error_hresult = $_.Exception.HResult
            $result.error_message = $_.Exception.Message
        }
    }

    if ($result.file -eq 'ok') {
        $bytes = [System.IO.File]::ReadAllBytes($path)
        $result.first_bytes = (($bytes | Select-Object -First ([Math]::Min(8, $bytes.Length))) | ForEach-Object { $_.ToString('X2') }) -join ' '

        $check = New-Object -ComObject ADODB.Recordset
        $check.CursorLocation = 3
        try {
            $check.Open($path, 'Provider=MSPersist', 3, 4, 256)
            $result.reopen = 'ok'
            $check.Close()
        } catch {
            $result.reopen = 'fail'
            $result.error_hresult = $_.Exception.HResult
            $result.error_message = $_.Exception.Message
        }
    }

    [pscustomobject]$result

    try { $stream.Close() } catch {}
    try { $rs.Close() } catch {}
    Remove-Item -LiteralPath $path -Force -ErrorAction SilentlyContinue
}
