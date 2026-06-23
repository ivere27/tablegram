param(
    [string]$Root = $env:TEMP,
    [string]$CsvPath = '',
    [string]$ManifestPath = ''
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

function Add-CsvLine([string]$Path, [object[]]$Values) {
    if ([string]::IsNullOrEmpty($Path)) {
        return
    }
    $line = ($Values | ForEach-Object { ConvertTo-CsvField $_ }) -join ','
    Add-Content -LiteralPath $Path -Value $line -Encoding ASCII
}

function Add-CsvRow([object[]]$Values) {
    Add-CsvLine $CsvPath $Values
}

function New-ProbeRecordset {
    $rs = New-Object -ComObject ADODB.Recordset
    $rs.CursorLocation = 3
    $rs.Fields.Append('ID', 3)
    $rs.Fields.Append('TXT', 202, 80, 32)
    $rs.Fields.Append('NUM', 3, 0, 32)
    $rs.Open()

    foreach ($id in 1, 2, 3) {
        $rs.AddNew()
        $rs.Fields.Item('ID').Value = $id
        $rs.Fields.Item('TXT').Value = "row$id"
        $rs.Fields.Item('NUM').Value = $id * 10
        $rs.Update()
    }
    $rs.UpdateBatch(3)

    $rs.MoveFirst()
    $rs.Fields.Item('TXT').Value = 'updated'
    $rs.Update()

    $rs.MoveNext()
    $rs.Delete()

    $rs.AddNew()
    $rs.Fields.Item('ID').Value = 4
    $rs.Fields.Item('TXT').Value = 'inserted'
    $rs.Fields.Item('NUM').Value = 40
    $rs.Update()

    return $rs
}

function Get-ViewSummary([string]$Path, [int]$FilterValue) {
    $rs = New-Object -ComObject ADODB.Recordset
    $rs.CursorLocation = 3
    $rs.Open($Path, 'Provider=MSPersist', 3, 4, 256)
    $rs.Filter = $FilterValue

    $items = @()
    if (-not $rs.EOF) {
        $rs.MoveFirst()
        while (-not $rs.EOF) {
            $id = '<deleted>'
            try {
                $id = [string]$rs.Fields.Item('ID').Value
            } catch {
            }
            $items += ('{0}:{1}' -f $id, $rs.Status)
            $rs.MoveNext()
        }
    }

    $rs.Close()
    return ($items -join '|')
}

$filters = @(
    @{ name = 'none'; value = 0 },
    @{ name = 'pending'; value = 1 },
    @{ name = 'affected'; value = 2 },
    @{ name = 'fetched'; value = 3 },
    @{ name = 'conflicting'; value = 5 },
    @{ name = 'criteria_id_1'; value = 'ID = 1' },
    @{ name = 'criteria_num_ge_30'; value = 'NUM >= 30' },
    @{ name = 'criteria_txt_inserted'; value = "TXT = 'inserted'" }
)

New-Item -ItemType Directory -Force -Path $Root | Out-Null

foreach ($filter in $filters) {
    $caseName = "filter_save_$($filter.name)"
    $xmlPath = Join-Path $Root "$caseName.xml"
    $adtgPath = Join-Path $Root "$caseName.adtg"
    $roundtripPath = Join-Path $Root "$caseName.roundtrip.xml"
    Remove-Item -LiteralPath $xmlPath, $adtgPath, $roundtripPath -Force -ErrorAction SilentlyContinue

    $result = [ordered]@{
        case_name = $caseName
        filter_name = $filter.name
        filter_value = $filter.value
        result = 'ok'
        stage = ''
        error_hresult = ''
        error_message = ''
        default_view = ''
        pending_view = ''
        affected_view = ''
        conflicting_view = ''
    }

    $rs = New-ProbeRecordset
    $stage = 'set_filter'
    try {
        $rs.Filter = $filter.value
        $stage = 'save_xml'
        $rs.Save($xmlPath, 1)
        $stage = 'save_adtg'
        $clone = $rs.Clone()
        $clone.Save($adtgPath, 0)
        $clone.Close()

        $stage = 'roundtrip_adtg'
        $roundtrip = New-Object -ComObject ADODB.Recordset
        $roundtrip.CursorLocation = 3
        $roundtrip.Open($adtgPath, 'Provider=MSPersist', 3, 4, 256)
        $roundtrip.Save($roundtripPath, 1)
        $roundtrip.Close()

        $result.default_view = Get-ViewSummary $xmlPath 0
        $result.pending_view = Get-ViewSummary $xmlPath 1
        $result.affected_view = Get-ViewSummary $xmlPath 2
        $result.conflicting_view = Get-ViewSummary $xmlPath 5
    } catch {
        $result.result = 'fail'
        $result.stage = $stage
        $result.error_hresult = $_.Exception.HResult
        $result.error_message = $_.Exception.Message
    }

    [pscustomobject]$result
    Add-CsvRow @(
        $result.case_name,
        $result.filter_name,
        $result.filter_value,
        $result.result,
        $result.stage,
        $result.error_hresult,
        $result.error_message,
        $result.default_view,
        $result.pending_view,
        $result.affected_view,
        $result.conflicting_view
    )
    if ($result.result -eq 'ok') {
        Add-CsvLine $ManifestPath @(
            $result.case_name,
            "filter_$($filter.name)_save",
            3,
            3,
            $xmlPath,
            $adtgPath,
            $roundtripPath
        )
    }

    try { $rs.Close() } catch {}
}
