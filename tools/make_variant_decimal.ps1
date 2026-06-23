param(
    [string]$Root = (Join-Path $PSScriptRoot '..\corpus\variant'),
    [string]$Scenario = 'decimal'
)

$ErrorActionPreference = 'Stop'

$adUseClient = 3
$adOpenStatic = 3
$adLockBatchOptimistic = 4
$adCmdFile = 256
$adAffectAll = 3
$adPersistADTG = 0
$adPersistXML = 1
$adFldIsNullable = 32
$adInteger = 3
$adVariant = 12

function CsvLine([object[]]$Values) {
    ($Values | ForEach-Object {
        '"' + ([string]$_).Replace('"', '""') + '"'
    }) -join ','
}

function Remove-CaseFiles([string]$CaseName) {
    foreach ($extension in @('xml', 'adtg', 'roundtrip.xml')) {
        Remove-Item -LiteralPath (Join-Path $Root "$CaseName.$extension") -Force -ErrorAction SilentlyContinue
    }
}

function Reset-ManifestRow([string]$Path, [string]$CaseName, [string]$Header) {
    if (Test-Path -LiteralPath $Path) {
        $lines = Get-Content -LiteralPath $Path | Where-Object { $_ -notlike "`"$CaseName`",*" }
    } else {
        $lines = @($Header)
    }
    if ($lines.Count -eq 0) {
        $lines = @($Header)
    }
    Set-Content -LiteralPath $Path -Value $lines -Encoding Default
}

function Set-VariantValue($Recordset, [string]$Scenario, [int]$ValueIndex) {
    switch ($Scenario) {
        'decimal' {
            $values = @(
                [decimal]'-1234.5678',
                [decimal]'-1.0001',
                [decimal]'0',
                [decimal]'1.0001',
                [decimal]'1234.5678'
            )
        }
        'sbyte' {
            $values = @(
                [sbyte]-128,
                [sbyte]-5,
                [sbyte]0,
                [sbyte]1,
                [sbyte]127
            )
        }
        'int64' {
            $values = @(
                [int64]::MinValue,
                [int64]-1,
                [int64]0,
                [int64]1,
                [int64]::MaxValue
            )
        }
        'uint16' {
            $values = @(
                [uint16]0,
                [uint16]1,
                [uint16]42,
                [uint16]65534,
                [uint16]65535
            )
        }
        'uint32' {
            $values = @(
                [uint32]0,
                [uint32]1,
                [uint32]42,
                [uint32]4294967294,
                [uint32]4294967295
            )
        }
        'uint64' {
            $values = @(
                [uint64]0,
                [uint64]1,
                [uint64]42,
                [uint64]18446744073709551614,
                [uint64]::MaxValue
            )
        }
        default {
            throw "unsupported variant supplement scenario: $Scenario"
        }
    }
    $value = $values[$ValueIndex % $values.Count]
    $Recordset.Fields.Item('VALUE_FIELD').Value = $value
}

function Add-VariantRow($Recordset, [string]$Scenario, [int]$RowId, [int]$ValueIndex) {
    $Recordset.AddNew()
    $Recordset.Fields.Item('ID').Value = $RowId
    Set-VariantValue $Recordset $Scenario $ValueIndex
    $Recordset.Update()
}

function Roundtrip-AdtgToXml([string]$AdtgPath, [string]$XmlPath) {
    $rs = New-Object -ComObject ADODB.Recordset
    $rs.CursorLocation = $adUseClient
    $rs.Open($AdtgPath, 'Provider=MSPersist', $adOpenStatic, $adLockBatchOptimistic, $adCmdFile)
    $rs.Save($XmlPath, $adPersistXML)
    $rs.Close()
}

function Write-VariantSupplementCase([string]$CaseName, [string]$Scenario) {
    $manifest = Join-Path $Root 'manifest.csv'
    $failures = Join-Path $Root 'failures.csv'
    $manifestHeader = 'case,scenario,result,xml,adtg,roundtrip_xml,error_number,error_description'
    $failuresHeader = 'case,scenario,error_number,error_description'
    Reset-ManifestRow $manifest $CaseName $manifestHeader
    Reset-ManifestRow $failures $CaseName $failuresHeader
    Remove-CaseFiles $CaseName

    $xmlPath = Join-Path $Root "$CaseName.xml"
    $adtgPath = Join-Path $Root "$CaseName.adtg"
    $roundtripPath = Join-Path $Root "$CaseName.roundtrip.xml"

    $rs = New-Object -ComObject ADODB.Recordset
    $rs.CursorLocation = $adUseClient
    $rs.Fields.Append('ID', $adInteger) | Out-Null
    $rs.Fields.Append('VALUE_FIELD', $adVariant, 0, $adFldIsNullable) | Out-Null
    $rs.Open()

    Add-VariantRow $rs $Scenario 1 0
    Add-VariantRow $rs $Scenario 2 1
    Add-VariantRow $rs $Scenario 3 2
    $rs.UpdateBatch($adAffectAll)

    $rs.MoveFirst()
    Set-VariantValue $rs $Scenario 3
    $rs.Update()

    $rs.MoveNext()
    $rs.Delete()

    Add-VariantRow $rs $Scenario 4 4

    $clone = $rs.Clone()
    $rs.Save($xmlPath, $adPersistXML)
    $clone.Save($adtgPath, $adPersistADTG)
    $clone.Close()
    $rs.Close()

    Roundtrip-AdtgToXml $adtgPath $roundtripPath
    Add-Content -LiteralPath $manifest -Encoding Default -Value (CsvLine @($CaseName, $Scenario, 'ok', $xmlPath, $adtgPath, $roundtripPath, '', ''))
}

New-Item -ItemType Directory -Force -Path $Root | Out-Null

$case = switch ($Scenario) {
    'decimal' { @{ Name = 'variant_decimal'; Scenario = 'decimal' } }
    'sbyte' { @{ Name = 'variant_sbyte'; Scenario = 'sbyte' } }
    'int64' { @{ Name = 'variant_int64'; Scenario = 'int64' } }
    'uint16' { @{ Name = 'variant_uint16'; Scenario = 'uint16' } }
    'uint32' { @{ Name = 'variant_uint32'; Scenario = 'uint32' } }
    'uint64' { @{ Name = 'variant_uint64'; Scenario = 'uint64' } }
    default { throw "unsupported variant supplement scenario: $Scenario" }
}

try {
    Write-VariantSupplementCase $case.Name $case.Scenario
} catch {
    Remove-CaseFiles $case.Name
    $manifest = Join-Path $Root 'manifest.csv'
    $failures = Join-Path $Root 'failures.csv'
    $number = $_.Exception.HResult
    $description = $_.Exception.Message
    Add-Content -LiteralPath $manifest -Encoding Default -Value (CsvLine @($case.Name, $case.Scenario, 'fail', '', '', '', $number, $description))
    Add-Content -LiteralPath $failures -Encoding Default -Value (CsvLine @($case.Name, $case.Scenario, $number, $description))
}
