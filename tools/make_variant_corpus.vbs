Option Explicit

Const adUseClient = 3
Const adOpenStatic = 3
Const adLockBatchOptimistic = 4
Const adCmdFile = 256
Const adAffectAll = 3
Const adPersistADTG = 0
Const adPersistXML = 1
Const adFldIsNullable = 32
Const adInteger = 3
Const adVariant = 12
Const adTypeBinary = 1

Dim fso
Set fso = CreateObject("Scripting.FileSystemObject")

Dim root
root = ArgText(0, fso.BuildPath(fso.GetParentFolderName(WScript.ScriptFullName), "..\corpus\variant"))

EnsureFolder root
DeleteIfExists fso.BuildPath(root, "manifest.csv")
DeleteIfExists fso.BuildPath(root, "failures.csv")
WriteLine fso.BuildPath(root, "manifest.csv"), "case,scenario,result,xml,adtg,roundtrip_xml,error_number,error_description"
WriteLine fso.BuildPath(root, "failures.csv"), "case,scenario,error_number,error_description"

MakeVariantCorpus

WScript.Echo "Generated ADO variant corpus in " & fso.GetAbsolutePathName(root)

Sub MakeVariantCorpus()
    MakeScenario "variant_string", "string"
    MakeScenario "variant_byte", "byte"
    MakeScenario "variant_smallint", "smallint"
    MakeScenario "variant_integer", "integer"
    MakeScenario "variant_single", "single"
    MakeScenario "variant_double", "double"
    MakeScenario "variant_currency", "currency"
    MakeScenario "variant_boolean", "boolean"
    MakeScenario "variant_date", "date"
    MakeScenario "variant_empty", "empty"
    MakeScenario "variant_binary", "binary"
    MakeScenario "variant_null", "null"
    MakeScenario "variant_mixed", "mixed"
    WriteVariantErrorProbeFailure
    RunDecimalSupplement
End Sub

Sub MakeScenario(caseName, scenario)
    Dim xmlPath, adtgPath, rtPath
    xmlPath = ""
    adtgPath = ""
    rtPath = ""
    DeleteCaseFiles caseName
    On Error Resume Next
    BuildVariantCase caseName, scenario, xmlPath, adtgPath, rtPath
    If Err.Number <> 0 Then
        DeleteIfExists xmlPath
        DeleteIfExists adtgPath
        DeleteIfExists rtPath
        WriteLine fso.BuildPath(root, "manifest.csv"), Csv(Array(caseName, scenario, "fail", "", "", "", Err.Number, Err.Description))
        WriteLine fso.BuildPath(root, "failures.csv"), Csv(Array(caseName, scenario, Err.Number, Err.Description))
        Err.Clear
    Else
        WriteLine fso.BuildPath(root, "manifest.csv"), Csv(Array(caseName, scenario, "ok", xmlPath, adtgPath, rtPath, "", ""))
    End If
    On Error GoTo 0
End Sub

Sub DeleteCaseFiles(caseName)
    DeleteIfExists fso.BuildPath(root, caseName & ".xml")
    DeleteIfExists fso.BuildPath(root, caseName & ".adtg")
    DeleteIfExists fso.BuildPath(root, caseName & ".roundtrip.xml")
End Sub

Sub BuildVariantCase(caseName, scenario, ByRef xmlPath, ByRef adtgPath, ByRef rtPath)
    Dim rs
    Set rs = NewVariantRecordset()

    AddVariantRow rs, scenario, 1, 0
    AddVariantRow rs, scenario, 2, 1
    AddVariantRow rs, scenario, 3, 2
    rs.UpdateBatch adAffectAll

    rs.MoveFirst
    SetVariantValue rs, scenario, 3
    rs.Update

    rs.MoveNext
    rs.Delete

    AddVariantRow rs, scenario, 4, 4

    xmlPath = fso.BuildPath(root, caseName & ".xml")
    adtgPath = fso.BuildPath(root, caseName & ".adtg")
    rtPath = fso.BuildPath(root, caseName & ".roundtrip.xml")
    DeleteIfExists xmlPath
    DeleteIfExists adtgPath
    DeleteIfExists rtPath
    SaveBoth rs, xmlPath, adtgPath
    rs.Close
    RoundtripAdtgToXml adtgPath, rtPath
End Sub

Function NewVariantRecordset()
    Dim rs
    Set rs = CreateObject("ADODB.Recordset")
    rs.CursorLocation = adUseClient
    rs.Fields.Append "ID", adInteger
    rs.Fields.Append "VALUE_FIELD", adVariant, , adFldIsNullable
    rs.Open
    Set NewVariantRecordset = rs
End Function

Sub AddVariantRow(rs, scenario, rowId, valueIndex)
    rs.AddNew
    rs.Fields("ID").Value = rowId
    SetVariantValue rs, scenario, valueIndex
    rs.Update
End Sub

Sub SetVariantValue(rs, scenario, valueIndex)
    Select Case scenario
        Case "string"
            rs.Fields("VALUE_FIELD").Value = ArrayAt(Array("plain", "Joe's <xml> & data", ChrW(&HD55C) & ChrW(&HAE00), "", "tail"), valueIndex)
        Case "byte"
            rs.Fields("VALUE_FIELD").Value = CByte(ArrayAt(Array(0, 1, 42, 254, 255), valueIndex))
        Case "smallint"
            rs.Fields("VALUE_FIELD").Value = CInt(ArrayAt(Array(-32768, -1, 0, 1, 32767), valueIndex))
        Case "integer"
            rs.Fields("VALUE_FIELD").Value = CLng(ArrayAt(Array(-2147483647, -1, 0, 1, 2147483647), valueIndex))
        Case "single"
            rs.Fields("VALUE_FIELD").Value = CSng(ArrayAt(Array("-12345.25", "-1.5", "0", "1.25", "12345.5"), valueIndex))
        Case "double"
            rs.Fields("VALUE_FIELD").Value = CDbl(ArrayAt(Array("-123456.125", "-1.5", "0", "1.25", "123456.5"), valueIndex))
        Case "currency"
            rs.Fields("VALUE_FIELD").Value = CCur(ArrayAt(Array("-1234.5678", "-1.0001", "0", "1.0001", "1234.5678"), valueIndex))
        Case "boolean"
            rs.Fields("VALUE_FIELD").Value = CBool(ArrayAt(Array(False, True, False, True, False), valueIndex))
        Case "date"
            rs.Fields("VALUE_FIELD").Value = DateValueAt(valueIndex)
        Case "empty"
            SetEmptyVariantValue rs
        Case "binary"
            rs.Fields("VALUE_FIELD").Value = Bytes(ByteValues(valueIndex, 8))
        Case "null"
            rs.Fields("VALUE_FIELD").Value = Null
        Case "mixed"
            SetMixedVariantValue rs, valueIndex
        Case Else
            rs.Fields("VALUE_FIELD").Value = CStr(valueIndex)
    End Select
End Sub

Sub SetEmptyVariantValue(rs)
    Dim emptyValue
    rs.Fields("VALUE_FIELD").Value = emptyValue
End Sub

Sub SetMixedVariantValue(rs, valueIndex)
    Select Case valueIndex Mod 5
        Case 0
            rs.Fields("VALUE_FIELD").Value = "mixed text"
        Case 1
            rs.Fields("VALUE_FIELD").Value = CLng(42)
        Case 2
            rs.Fields("VALUE_FIELD").Value = CDbl("3.25")
        Case 3
            rs.Fields("VALUE_FIELD").Value = DateSerial(2026, 6, 12) + TimeSerial(1, 2, 3)
        Case Else
            rs.Fields("VALUE_FIELD").Value = Null
    End Select
End Sub

Sub WriteVariantErrorProbeFailure()
    Dim caseName, scenario, errorNumber, errorDescription
    caseName = "variant_error"
    scenario = "error"
    errorNumber = "-2147352562"
    errorDescription = "ADODB Field.Value rejects VT_ERROR with DISP_E_BADPARAMCOUNT; reproduce with tools\probe_variant_error.cpp"
    DeleteCaseFiles caseName
    WriteLine fso.BuildPath(root, "manifest.csv"), Csv(Array(caseName, scenario, "fail", "", "", "", errorNumber, errorDescription))
    WriteLine fso.BuildPath(root, "failures.csv"), Csv(Array(caseName, scenario, errorNumber, errorDescription))
End Sub

Function DateValueAt(valueIndex)
    Select Case valueIndex Mod 5
        Case 0: DateValueAt = DateSerial(1899, 12, 30) + TimeSerial(0, 0, 0)
        Case 1: DateValueAt = DateSerial(1999, 12, 31) + TimeSerial(23, 59, 58)
        Case 2: DateValueAt = DateSerial(2000, 1, 1) + TimeSerial(0, 0, 1)
        Case 3: DateValueAt = DateSerial(2026, 6, 12) + TimeSerial(14, 30, 15)
        Case Else: DateValueAt = DateSerial(2038, 1, 19) + TimeSerial(3, 14, 7)
    End Select
End Function

Function ByteValues(valueIndex, count)
    Dim values(), i
    ReDim values(count - 1)
    For i = 0 To count - 1
        values(i) = (valueIndex * 37 + i * 19) Mod 256
    Next
    ByteValues = values
End Function

Function Bytes(values)
    Dim doc, node, hexText, i
    hexText = ""
    For i = 0 To UBound(values)
        hexText = hexText & HexByte(CInt(values(i)))
    Next

    Set doc = CreateObject("MSXML2.DOMDocument.6.0")
    Set node = doc.createElement("bytes")
    node.dataType = "bin.hex"
    node.Text = hexText
    Bytes = node.nodeTypedValue
End Function

Function HexByte(value)
    HexByte = Right("0" & Hex(value And 255), 2)
End Function

Sub SaveBoth(rs, xmlPath, adtgPath)
    Dim clone
    Set clone = rs.Clone
    rs.Save xmlPath, adPersistXML
    clone.Save adtgPath, adPersistADTG
    clone.Close
End Sub

Sub RoundtripAdtgToXml(adtgPath, xmlPath)
    Dim rs
    Set rs = CreateObject("ADODB.Recordset")
    rs.CursorLocation = adUseClient
    rs.Open adtgPath, "Provider=MSPersist", adOpenStatic, adLockBatchOptimistic, adCmdFile
    rs.Save xmlPath, adPersistXML
    rs.Close
End Sub

Function ArrayAt(values, index)
    ArrayAt = values(index Mod (UBound(values) + 1))
End Function

Function ArgText(index, defaultValue)
    If WScript.Arguments.Count > index Then
        ArgText = WScript.Arguments(index)
    Else
        ArgText = defaultValue
    End If
End Function

Sub RunDecimalSupplement()
    Dim shell, scriptPath, psPath, command, exitCode, scenarios, i, scenario
    scriptPath = fso.BuildPath(fso.GetParentFolderName(WScript.ScriptFullName), "make_variant_decimal.ps1")
    psPath = fso.BuildPath(fso.GetSpecialFolder(0), "SysWOW64\WindowsPowerShell\v1.0\powershell.exe")
    If Not fso.FileExists(scriptPath) Or Not fso.FileExists(psPath) Then Exit Sub

    Set shell = CreateObject("WScript.Shell")
    scenarios = Array("sbyte", "decimal", "int64", "uint16", "uint32", "uint64")
    For i = 0 To UBound(scenarios)
        scenario = scenarios(i)
        command = """" & psPath & """ -NoProfile -ExecutionPolicy Bypass -File """ & scriptPath & """ """ & root & """ """ & scenario & """"
        exitCode = shell.Run(command, 0, True)
        If exitCode <> 0 Then Err.Raise exitCode, "make_variant_decimal.ps1", "variant " & scenario & " supplement failed"
    Next
End Sub

Function Csv(values)
    Dim out, i
    out = ""
    For i = 0 To UBound(values)
        If i > 0 Then out = out & ","
        out = out & """" & Replace(CStr(values(i)), """", """""") & """"
    Next
    Csv = out
End Function

Sub WriteLine(path, text)
    Dim stream
    Set stream = fso.OpenTextFile(path, 8, True)
    stream.WriteLine text
    stream.Close
End Sub

Sub DeleteIfExists(path)
    If fso.FileExists(path) Then
        fso.DeleteFile path, True
    End If
End Sub

Sub EnsureFolder(path)
    Dim parent
    If fso.FolderExists(path) Then Exit Sub
    parent = fso.GetParentFolderName(path)
    If Len(parent) > 0 And Not fso.FolderExists(parent) Then EnsureFolder parent
    fso.CreateFolder path
End Sub
