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

function New-ProbeValue([int]$CodePoint) {
    'A' + [string]([char]$CodePoint) + 'Z'
}

$fieldTypes = @(
    @{ name = 'VarWChar'; code = 202; size = 40 },
    @{ name = 'LongVarWChar'; code = 203; size = 4000 },
    @{ name = 'VarChar'; code = 200; size = 40 },
    @{ name = 'LongVarChar'; code = 201; size = 4000 }
)

$controls = @(
    @{ name = 'nul'; code = 0 },
    @{ name = 'soh'; code = 1 },
    @{ name = 'backspace'; code = 8 },
    @{ name = 'vertical_tab'; code = 11 },
    @{ name = 'form_feed'; code = 12 },
    @{ name = 'shift_out'; code = 14 },
    @{ name = 'unit_separator'; code = 31 }
)

New-Item -ItemType Directory -Force -Path $Root | Out-Null

foreach ($fieldType in $fieldTypes) {
    foreach ($control in $controls) {
        $caseName = "text_control_$($fieldType.name)_$($control.name)"
        $xmlPath = Join-Path $Root "$caseName.xml"
        $adtgPath = Join-Path $Root "$caseName.adtg"
        Remove-Item -LiteralPath $xmlPath, $adtgPath -Force -ErrorAction SilentlyContinue

        $result = [ordered]@{
            case_name = $caseName
            field_type = $fieldType.name
            type_code = $fieldType.code
            control_name = $control.name
            code_point = $control.code
            result = 'ok'
            stage = ''
            error_hresult = ''
            error_message = ''
            xml_exists = ''
            adtg_exists = ''
            reopened_length = ''
        }

        $rs = New-Object -ComObject ADODB.Recordset
        $rs.CursorLocation = 3
        $stage = 'append_schema'
        try {
            $rs.Fields.Append('ID', 3)
            $rs.Fields.Append('VALUE_FIELD', $fieldType.code, $fieldType.size, 32)
            $stage = 'open_recordset'
            $rs.Open()
            $stage = 'assign_value'
            $rs.AddNew()
            $rs.Fields.Item('ID').Value = 1
            $rs.Fields.Item('VALUE_FIELD').Value = New-ProbeValue $control.code
            $stage = 'update_row'
            $rs.Update()
            $stage = 'save_xml'
            $rs.Save($xmlPath, 1)
            $stage = 'save_adtg'
            $clone = $rs.Clone()
            $clone.Save($adtgPath, 0)
            $clone.Close()
            $stage = 'reopen_xml'
            $check = New-Object -ComObject ADODB.Recordset
            $check.CursorLocation = 3
            $check.Open($xmlPath, 'Provider=MSPersist', 3, 4, 256)
            $result.reopened_length = ([string]$check.Fields.Item('VALUE_FIELD').Value).Length
            $check.Close()
        } catch {
            $result.result = 'fail'
            $result.stage = $stage
            $result.error_hresult = $_.Exception.HResult
            $result.error_message = $_.Exception.Message
        }

        $result.xml_exists = [string](Test-Path -LiteralPath $xmlPath)
        $result.adtg_exists = [string](Test-Path -LiteralPath $adtgPath)
        [pscustomobject]$result
        Add-CsvRow @(
            $result.case_name,
            $result.field_type,
            $result.type_code,
            $result.control_name,
            $result.code_point,
            $result.result,
            $result.stage,
            $result.error_hresult,
            $result.error_message,
            $result.xml_exists,
            $result.adtg_exists,
            $result.reopened_length
        )

        try { $rs.Close() } catch {}
        Remove-Item -LiteralPath $xmlPath, $adtgPath -Force -ErrorAction SilentlyContinue
    }
}
