param(
    [string]$Root = $env:TEMP,
    [string]$CsvPath = ''
)

$ErrorActionPreference = 'Stop'

function ConvertTo-AsciiText([string]$Value) {
    if ($null -eq $Value) {
        return ''
    }
    return $Value -replace '[^\x20-\x7E]', '?'
}

function ConvertTo-CsvField([object]$Value) {
    $text = ConvertTo-AsciiText ([string]$Value)
    '"' + ($text -replace '"', '""') + '"'
}

function Add-CsvRow([object[]]$Values) {
    if ([string]::IsNullOrEmpty($CsvPath)) {
        return
    }
    $line = ($Values | ForEach-Object { ConvertTo-CsvField $_ }) -join ','
    Add-Content -LiteralPath $CsvPath -Value $line -Encoding ASCII
}

$types = @(
    @{ name = 'Single'; code = 4; values = @(
        @{ name = 'nan'; value = [single]::NaN },
        @{ name = 'positive_infinity'; value = [single]::PositiveInfinity },
        @{ name = 'negative_infinity'; value = [single]::NegativeInfinity }
    ) },
    @{ name = 'Double'; code = 5; values = @(
        @{ name = 'nan'; value = [double]::NaN },
        @{ name = 'positive_infinity'; value = [double]::PositiveInfinity },
        @{ name = 'negative_infinity'; value = [double]::NegativeInfinity }
    ) }
)

foreach ($type in $types) {
    foreach ($case in $type.values) {
        $caseName = "float_special_$($type.name)_$($case.name)"
        $xmlPath = Join-Path $Root "$caseName.xml"
        $adtgPath = Join-Path $Root "$caseName.adtg"
        Remove-Item -LiteralPath $xmlPath, $adtgPath -Force -ErrorAction SilentlyContinue

        $result = [ordered]@{
            type_name = $type.name
            type_code = $type.code
            value_name = $case.name
            result = 'ok'
            stage = ''
            error_hresult = ''
            error_message = ''
            xml_value = ''
            reopen_value = ''
        }

        $rs = New-Object -ComObject ADODB.Recordset
        $rs.CursorLocation = 3
        $stage = 'create_recordset'

        try {
            $stage = 'append_schema'
            $rs.Fields.Append('ID', 3)
            $rs.Fields.Append('VALUE_FIELD', $type.code, 0, 32)
            $stage = 'open_recordset'
            $rs.Open()
            $stage = 'add_row'
            $rs.AddNew()
            $rs.Fields.Item('ID').Value = 1
            $stage = 'assign_value'
            $rs.Fields.Item('VALUE_FIELD').Value = $case.value
            $stage = 'update_row'
            $rs.Update()
            $stage = 'save_xml'
            $rs.Save($xmlPath, 1)
            $stage = 'save_adtg'
            $clone = $rs.Clone()
            $clone.Save($adtgPath, 0)
            $clone.Close()

            $xmlText = [System.IO.File]::ReadAllText($xmlPath)
            if ($xmlText -match 'VALUE_FIELD="([^"]*)"') {
                $result.xml_value = $Matches[1]
            }

            $check = New-Object -ComObject ADODB.Recordset
            $check.CursorLocation = 3
            $stage = 'reopen_xml'
            $check.Open($xmlPath, 'Provider=MSPersist', 3, 4, 256)
            $result.reopen_value = [string]$check.Fields.Item('VALUE_FIELD').Value
            $check.Close()
        } catch {
            $result.result = 'fail'
            $result.stage = $stage
            $result.error_hresult = $_.Exception.HResult
            $result.error_message = $_.Exception.Message
        }

        [pscustomobject]$result
        Add-CsvRow @(
            $result.type_name,
            $result.type_code,
            $result.value_name,
            $result.result,
            $result.stage,
            $result.error_hresult,
            $result.error_message
        )

        try { $rs.Close() } catch {}
        Remove-Item -LiteralPath $xmlPath, $adtgPath -Force -ErrorAction SilentlyContinue
    }
}
