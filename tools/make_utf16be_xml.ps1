param(
    [Parameter(Mandatory = $true)]
    [string]$Root,

    [string]$SourceName = 'utf16_xml_stream.xml',
    [string]$OutputName = 'utf16be_xml_stream.xml'
)

$ErrorActionPreference = 'Stop'

$sourcePath = Join-Path $Root $SourceName
$outputPath = Join-Path $Root $OutputName

if (-not (Test-Path -LiteralPath $sourcePath)) {
    throw "source UTF-16 XML fixture not found: $sourcePath"
}

$bytes = [System.IO.File]::ReadAllBytes($sourcePath)
if ($bytes.Length -lt 4 -or $bytes[0] -ne 0xFF -or $bytes[1] -ne 0xFE) {
    throw "source XML is not UTF-16LE with BOM: $sourcePath"
}
if (($bytes.Length % 2) -ne 0) {
    throw "source UTF-16 XML has an odd byte length: $sourcePath"
}

$out = New-Object byte[] $bytes.Length
$out[0] = 0xFE
$out[1] = 0xFF
for ($i = 2; $i -lt $bytes.Length; $i += 2) {
    $out[$i] = $bytes[$i + 1]
    $out[$i + 1] = $bytes[$i]
}

[System.IO.File]::WriteAllBytes($outputPath, $out)

$check = New-Object -ComObject ADODB.Recordset
$check.CursorLocation = 3
$check.Open($outputPath, 'Provider=MSPersist', 3, 4, 256)
$check.Close()
