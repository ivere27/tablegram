Option Explicit

Const adUseClient = 3
Const adOpenStatic = 3
Const adLockBatchOptimistic = 4
Const adCmdFile = 256
Const adAffectAll = 3
Const adPersistADTG = 0
Const adPersistXML = 1
Const adFldIsNullable = 32
Const adTypeBinary = 1
Const adTypeText = 2

Const adArray = 8192
Const adEmpty = 0
Const adTinyInt = 16
Const adUnsignedTinyInt = 17
Const adSmallInt = 2
Const adUnsignedSmallInt = 18
Const adInteger = 3
Const adUnsignedInt = 19
Const adBigInt = 20
Const adUnsignedBigInt = 21
Const adSingle = 4
Const adDouble = 5
Const adCurrency = 6
Const adBoolean = 11
Const adDate = 7
Const adDBDate = 133
Const adDBTime = 134
Const adDBTimeStamp = 135
Const adFileTime = 64
Const adGUID = 72
Const adBSTR = 8
Const adChar = 129
Const adWChar = 130
Const adVarChar = 200
Const adVarWChar = 202
Const adLongVarChar = 201
Const adLongVarWChar = 203
Const adBinary = 128
Const adVarBinary = 204
Const adLongVarBinary = 205
Const adNumeric = 131
Const adUserDefined = 132
Const adDecimal = 14
Const adVarNumeric = 139
Const adError = 10
Const adVariant = 12
Const adIDispatch = 9
Const adIUnknown = 13
Const adChapter = 136
Const adPropVariant = 138

Dim fso
Set fso = CreateObject("Scripting.FileSystemObject")

Dim root
root = ArgText(0, fso.BuildPath(fso.GetParentFolderName(WScript.ScriptFullName), "..\corpus\exhaustive"))

EnsureFolder root
DeleteIfExists fso.BuildPath(root, "coverage.csv")
DeleteIfExists fso.BuildPath(root, "unsupported.csv")
WriteLine fso.BuildPath(root, "coverage.csv"), "type_name,type_code,scenario,result,xml,adtg,roundtrip_xml,error_number,error_description"
WriteLine fso.BuildPath(root, "unsupported.csv"), "type_name,type_code,result,error_number,error_description"

MakeSupportedCorpus
MakeUnsupportedMatrix

WScript.Echo "Generated exhaustive flat ADO corpus in " & fso.GetAbsolutePathName(root)

Sub MakeSupportedCorpus()
    Dim i, def
    For i = 0 To SupportedTypeCount() - 1
        def = SupportedTypeAt(i)
        MakeScenario def, "boundaries"
        MakeScenario def, "states"
        MakeScenario def, "null_states"
    Next
    MakeNumericPrecisionScaleCorpus
End Sub

Sub MakeScenario(def, scenario)
    On Error Resume Next
    Select Case scenario
        Case "boundaries"
            MakeBoundaryCase def
        Case "states"
            MakeStateCase def
        Case "null_states"
            MakeNullStateCase def
    End Select
    If Err.Number <> 0 Then
        WriteCoverage def, scenario, "fail", "", "", "", Err.Number, Err.Description
        Err.Clear
    End If
    On Error GoTo 0
End Sub

Sub MakeBoundaryCase(def)
    Dim rs, rowNo, xmlPath, adtgPath, rtPath
    Set rs = NewTypedRecordset(def)
    For rowNo = 0 To 4
        rs.AddNew
        rs.Fields("ID").Value = rowNo + 1
        SetFieldVariant rs, "VALUE_FIELD", def(0), rowNo
        rs.Update
    Next
    rs.AddNew
    rs.Fields("ID").Value = 6
    rs.Fields("VALUE_FIELD").Value = Null
    rs.Update
    rs.UpdateBatch adAffectAll

    SaveScenario rs, def, "boundaries", xmlPath, adtgPath, rtPath
    rs.Close
    WriteCoverage def, "boundaries", "ok", xmlPath, adtgPath, rtPath, "", ""
End Sub

Sub MakeStateCase(def)
    Dim rs, xmlPath, adtgPath, rtPath
    Set rs = NewTypedRecordset(def)

    AddValueRow rs, def, 1, 0
    AddValueRow rs, def, 2, 1
    AddValueRow rs, def, 3, 2
    rs.UpdateBatch adAffectAll

    rs.MoveFirst
    SetFieldVariant rs, "VALUE_FIELD", def(0), 3
    rs.Update

    rs.MoveNext
    rs.Delete

    AddValueRow rs, def, 4, 4

    SaveScenario rs, def, "states", xmlPath, adtgPath, rtPath
    rs.Close
    WriteCoverage def, "states", "ok", xmlPath, adtgPath, rtPath, "", ""
End Sub

Sub MakeNullStateCase(def)
    Dim rs, xmlPath, adtgPath, rtPath
    Set rs = NewTypedRecordset(def)

    AddValueRow rs, def, 1, 0
    AddNullRow rs, 2
    AddNullRow rs, 3
    rs.UpdateBatch adAffectAll

    rs.MoveFirst
    rs.Fields("VALUE_FIELD").Value = Null
    rs.Update

    rs.MoveNext
    rs.Delete

    AddNullRow rs, 4

    SaveScenario rs, def, "null_states", xmlPath, adtgPath, rtPath
    rs.Close
    WriteCoverage def, "null_states", "ok", xmlPath, adtgPath, rtPath, "", ""
End Sub

Sub MakeNumericPrecisionScaleCorpus()
    MakePrecisionScaleScenario Array("Numeric", adNumeric, 0), 1, 0, Array("-9", "0", "9")
    MakePrecisionScaleScenario Array("Numeric", adNumeric, 0), 5, 0, Array("-99999", "-1", "0", "1", "99999")
    MakePrecisionScaleScenario Array("Numeric", adNumeric, 0), 9, 2, Array("-12345.67", "-1.23", "0", "1.23", "12345.67")
    MakePrecisionScaleScenario Array("Numeric", adNumeric, 0), 18, 6, Array("-999999999999.999999", "-1.000001", "0", "1.000001", "999999999999.999999")
    MakePrecisionScaleScenario Array("Numeric", adNumeric, 0), 28, 0, Array("-9999999999999999999999999999", "-1", "0", "1", "9999999999999999999999999999")
    MakePrecisionScaleScenario Array("Numeric", adNumeric, 0), 28, 10, Array("-999999999999999999.9999999999", "-1.0000000001", "0", "1.0000000001", "999999999999999999.9999999999")
    MakePrecisionScaleScenario Array("Numeric", adNumeric, 0), 28, 28, Array("-0.9999999999999999999999999999", "-0.0000000000000000000000000001", "0", "0.0000000000000000000000000001", "0.9999999999999999999999999999")
    MakePrecisionScaleScenario Array("Decimal", adDecimal, 0), 1, 0, Array("-9", "0", "9")
    MakePrecisionScaleScenario Array("Decimal", adDecimal, 0), 5, 0, Array("-99999", "-1", "0", "1", "99999")
    MakePrecisionScaleScenario Array("Decimal", adDecimal, 0), 9, 2, Array("-12345.67", "-1.23", "0", "1.23", "12345.67")
    MakePrecisionScaleScenario Array("Decimal", adDecimal, 0), 18, 6, Array("-999999999999.999999", "-1.000001", "0", "1.000001", "999999999999.999999")
    MakePrecisionScaleScenario Array("Decimal", adDecimal, 0), 28, 0, Array("-9999999999999999999999999999", "-1", "0", "1", "9999999999999999999999999999")
    MakePrecisionScaleScenario Array("Decimal", adDecimal, 0), 28, 10, Array("-999999999999999999.9999999999", "-1.0000000001", "0", "1.0000000001", "999999999999999999.9999999999")
    MakePrecisionScaleScenario Array("Decimal", adDecimal, 0), 28, 28, Array("-0.9999999999999999999999999999", "-0.0000000000000000000000000001", "0", "0.0000000000000000000000000001", "0.9999999999999999999999999999")
End Sub

Sub MakePrecisionScaleScenario(def, precision, scale, values)
    Dim scenario
    scenario = "precision_scale_" & CStr(precision) & "_" & CStr(scale)

    On Error Resume Next
    MakePrecisionScaleCase def, scenario, precision, scale, values
    If Err.Number <> 0 Then
        WriteCoverage def, scenario, "fail", "", "", "", Err.Number, Err.Description
        Err.Clear
    End If
    On Error GoTo 0
End Sub

Sub MakePrecisionScaleCase(def, scenario, precision, scale, values)
    Dim rs, rowNo, xmlPath, adtgPath, rtPath

    Set rs = CreateObject("ADODB.Recordset")
    rs.CursorLocation = adUseClient
    rs.Fields.Append "ID", adInteger
    rs.Fields.Append "VALUE_FIELD", def(1), , adFldIsNullable
    rs.Fields("VALUE_FIELD").Precision = precision
    rs.Fields("VALUE_FIELD").NumericScale = scale
    rs.Open

    For rowNo = 0 To UBound(values)
        rs.AddNew
        rs.Fields("ID").Value = rowNo + 1
        rs.Fields("VALUE_FIELD").Value = CStr(values(rowNo))
        rs.Update
    Next
    rs.AddNew
    rs.Fields("ID").Value = UBound(values) + 2
    rs.Fields("VALUE_FIELD").Value = Null
    rs.Update
    rs.UpdateBatch adAffectAll

    SaveScenario rs, def, scenario, xmlPath, adtgPath, rtPath
    rs.Close
    WriteCoverage def, scenario, "ok", xmlPath, adtgPath, rtPath, "", ""
End Sub

Function NewTypedRecordset(def)
    Dim rs
    Set rs = CreateObject("ADODB.Recordset")
    rs.CursorLocation = adUseClient
    rs.Fields.Append "ID", adInteger
    AppendTypedField rs, "VALUE_FIELD", def
    rs.Open
    Set NewTypedRecordset = rs
End Function

Sub AddValueRow(rs, def, rowId, valueIndex)
    rs.AddNew
    rs.Fields("ID").Value = rowId
    SetFieldVariant rs, "VALUE_FIELD", def(0), valueIndex
    rs.Update
End Sub

Sub AddNullRow(rs, rowId)
    rs.AddNew
    rs.Fields("ID").Value = rowId
    rs.Fields("VALUE_FIELD").Value = Null
    rs.Update
End Sub

Sub AppendTypedField(rs, fieldName, def)
    Dim typeCode, size
    typeCode = def(1)
    size = def(2)

    If size > 0 Then
        rs.Fields.Append fieldName, typeCode, size, adFldIsNullable
    Else
        rs.Fields.Append fieldName, typeCode, , adFldIsNullable
    End If

    If def(0) = "Numeric" Or def(0) = "Decimal" Or def(0) = "VarNumeric" Then
        rs.Fields(fieldName).Precision = 18
        rs.Fields(fieldName).NumericScale = 4
    End If
End Sub

Sub SaveScenario(rs, def, scenario, ByRef xmlPath, ByRef adtgPath, ByRef rtPath)
    Dim name
    name = "flat_" & SafeName(def(0)) & "_" & scenario
    xmlPath = fso.BuildPath(root, name & ".xml")
    adtgPath = fso.BuildPath(root, name & ".adtg")
    rtPath = fso.BuildPath(root, name & ".roundtrip.xml")
    DeleteIfExists xmlPath
    DeleteIfExists adtgPath
    DeleteIfExists rtPath
    SaveBoth rs, xmlPath, adtgPath
    RoundtripAdtgToXml adtgPath, rtPath
End Sub

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

Sub MakeUnsupportedMatrix()
    Dim i, def
    For i = 0 To UnsupportedTypeCount() - 1
        def = UnsupportedTypeAt(i)
        On Error Resume Next
        ProbeUnsupportedType def
        If Err.Number <> 0 Then
            WriteUnsupported def, "fail", Err.Number, Err.Description
            Err.Clear
        Else
            WriteUnsupported def, "probe_ok_not_exhaustive", "", ""
        End If
        On Error GoTo 0
    Next
End Sub

Sub ProbeUnsupportedType(def)
    Dim rs, tempXml
    tempXml = fso.BuildPath(root, "unsupported_probe_" & SafeName(def(0)) & ".xml")
    DeleteIfExists tempXml

    Set rs = CreateObject("ADODB.Recordset")
    rs.CursorLocation = adUseClient
    AppendTypedField rs, "VALUE_FIELD", def
    rs.Open
    rs.AddNew
    SetFieldVariant rs, "VALUE_FIELD", def(0), 0
    rs.Update
    rs.Save tempXml, adPersistXML
    rs.Close
    DeleteIfExists tempXml
End Sub

Sub SetFieldVariant(rs, fieldName, kind, valueIndex)
    Select Case kind
        Case "TinyInt"
            rs.Fields(fieldName).Value = CInt(ArrayAt(Array(-128, -1, 0, 1, 127), valueIndex))
        Case "UnsignedTinyInt"
            rs.Fields(fieldName).Value = CByte(ArrayAt(Array(0, 1, 127, 254, 255), valueIndex))
        Case "SmallInt"
            rs.Fields(fieldName).Value = CInt(ArrayAt(Array(-32768, -1, 0, 1, 32767), valueIndex))
        Case "UnsignedSmallInt"
            rs.Fields(fieldName).Value = CLng(ArrayAt(Array(0, 1, 32767, 32768, 65535), valueIndex))
        Case "Integer"
            rs.Fields(fieldName).Value = CLng(ArrayAt(Array(-2147483647, -1, 0, 1, 2147483647), valueIndex))
        Case "UnsignedInt"
            rs.Fields(fieldName).Value = ArrayAt(Array("0", "1", "2147483647", "2147483648", "4294967295"), valueIndex)
        Case "BigInt"
            rs.Fields(fieldName).Value = ArrayAt(Array("-9223372036854775808", "-2147483649", "0", "2147483648", "9223372036854775807"), valueIndex)
        Case "UnsignedBigInt"
            rs.Fields(fieldName).Value = ArrayAt(Array("0", "1", "9223372036854775807", "9223372036854775808", "18446744073709551615"), valueIndex)
        Case "Single"
            rs.Fields(fieldName).Value = CSng(ArrayAt(Array("-12345.25", "-1.5", "0", "1.25", "12345.5"), valueIndex))
        Case "Double"
            rs.Fields(fieldName).Value = CDbl(ArrayAt(Array("-123456789.125", "-1.5", "0", "1.25", "123456789.5"), valueIndex))
        Case "Currency"
            rs.Fields(fieldName).Value = CCur(ArrayAt(Array("-922337203685477.5808", "-1.0001", "0", "1.0001", "922337203685477.5807"), valueIndex))
        Case "Numeric", "Decimal", "VarNumeric"
            rs.Fields(fieldName).Value = CCur(ArrayAt(Array("-922337.5808", "-1.2345", "0", "1.2345", "922337.5807"), valueIndex))
        Case "Boolean"
            rs.Fields(fieldName).Value = CBool(ArrayAt(Array(False, True, False, True, False), valueIndex))
        Case "Date", "DBDate", "DBTime", "DBTimeStamp", "FileTime"
            rs.Fields(fieldName).Value = DateValueAt(valueIndex)
        Case "GUID"
            rs.Fields(fieldName).Value = ArrayAt(Array( _
                "{00000000-0000-0000-0000-000000000000}", _
                "{11111111-2222-3333-4444-555555555555}", _
                "{AAAAAAAA-BBBB-CCCC-DDDD-EEEEEEEEEEEE}", _
                "{01234567-89AB-CDEF-0123-456789ABCDEF}", _
                "{FFFFFFFF-FFFF-FFFF-FFFF-FFFFFFFFFFFF}"), valueIndex)
        Case "BSTR", "VarChar", "VarWChar"
            rs.Fields(fieldName).Value = ArrayAt(Array("", "plain ascii", "Joe's <xml> & data", ChrW(&HD55C) & ChrW(&HAE00), "tail-value"), valueIndex)
        Case "Char", "WChar"
            rs.Fields(fieldName).Value = FixedText(ArrayAt(Array("", "A", "fixed", "twelve-char", "last"), valueIndex), 12)
        Case "LongVarChar", "LongVarWChar"
            rs.Fields(fieldName).Value = LongText(valueIndex)
        Case "Binary"
            rs.Fields(fieldName).Value = Bytes(ByteValues(valueIndex, 8))
        Case "VarBinary"
            rs.Fields(fieldName).Value = Bytes(ByteValues(valueIndex, 12))
        Case "LongVarBinary"
            rs.Fields(fieldName).Value = Bytes(ByteValues(valueIndex, 96))
        Case "Error"
            rs.Fields(fieldName).Value = CLng(ArrayAt(Array(0, 1, 5, 1000, 2147483647), valueIndex))
        Case Else
            rs.Fields(fieldName).Value = ArrayAt(Array("v0", "v1", "v2", "v3", "v4"), valueIndex)
    End Select
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

Function FixedText(value, size)
    Dim text
    text = CStr(value)
    If Len(text) < size Then text = text & Space(size - Len(text))
    FixedText = Left(text, size)
End Function

Function LongText(valueIndex)
    Dim out, i
    out = ""
    For i = 1 To 80
        out = out & "long-" & CStr(valueIndex) & "-" & CStr(i) & " " & ChrW(&HD55C) & ChrW(&HAE00) & " "
    Next
    LongText = out
End Function

Function SupportedTypeCount()
    SupportedTypeCount = 30
End Function

Function SupportedTypeAt(index)
    Select Case index
        Case 0: SupportedTypeAt = Array("TinyInt", adTinyInt, 0)
        Case 1: SupportedTypeAt = Array("UnsignedTinyInt", adUnsignedTinyInt, 0)
        Case 2: SupportedTypeAt = Array("SmallInt", adSmallInt, 0)
        Case 3: SupportedTypeAt = Array("UnsignedSmallInt", adUnsignedSmallInt, 0)
        Case 4: SupportedTypeAt = Array("Integer", adInteger, 0)
        Case 5: SupportedTypeAt = Array("UnsignedInt", adUnsignedInt, 0)
        Case 6: SupportedTypeAt = Array("BigInt", adBigInt, 0)
        Case 7: SupportedTypeAt = Array("UnsignedBigInt", adUnsignedBigInt, 0)
        Case 8: SupportedTypeAt = Array("Single", adSingle, 0)
        Case 9: SupportedTypeAt = Array("Double", adDouble, 0)
        Case 10: SupportedTypeAt = Array("Currency", adCurrency, 0)
        Case 11: SupportedTypeAt = Array("Boolean", adBoolean, 0)
        Case 12: SupportedTypeAt = Array("Date", adDate, 0)
        Case 13: SupportedTypeAt = Array("DBDate", adDBDate, 0)
        Case 14: SupportedTypeAt = Array("DBTime", adDBTime, 0)
        Case 15: SupportedTypeAt = Array("DBTimeStamp", adDBTimeStamp, 0)
        Case 16: SupportedTypeAt = Array("FileTime", adFileTime, 0)
        Case 17: SupportedTypeAt = Array("GUID", adGUID, 0)
        Case 18: SupportedTypeAt = Array("BSTR", adBSTR, 120)
        Case 19: SupportedTypeAt = Array("Char", adChar, 12)
        Case 20: SupportedTypeAt = Array("WChar", adWChar, 12)
        Case 21: SupportedTypeAt = Array("VarChar", adVarChar, 120)
        Case 22: SupportedTypeAt = Array("VarWChar", adVarWChar, 120)
        Case 23: SupportedTypeAt = Array("LongVarChar", adLongVarChar, 4000)
        Case 24: SupportedTypeAt = Array("LongVarWChar", adLongVarWChar, 4000)
        Case 25: SupportedTypeAt = Array("Binary", adBinary, 8)
        Case 26: SupportedTypeAt = Array("VarBinary", adVarBinary, 128)
        Case 27: SupportedTypeAt = Array("LongVarBinary", adLongVarBinary, 4000)
        Case 28: SupportedTypeAt = Array("Numeric", adNumeric, 0)
        Case Else: SupportedTypeAt = Array("Decimal", adDecimal, 0)
    End Select
End Function

Function UnsupportedTypeCount()
    UnsupportedTypeCount = 10
End Function

Function UnsupportedTypeAt(index)
    Select Case index
        Case 0: UnsupportedTypeAt = Array("Empty", adEmpty, 0)
        Case 1: UnsupportedTypeAt = Array("VarNumeric", adVarNumeric, 0)
        Case 2: UnsupportedTypeAt = Array("Error", adError, 0)
        Case 3: UnsupportedTypeAt = Array("Variant", adVariant, 0)
        Case 4: UnsupportedTypeAt = Array("IDispatch", adIDispatch, 0)
        Case 5: UnsupportedTypeAt = Array("IUnknown", adIUnknown, 0)
        Case 6: UnsupportedTypeAt = Array("Chapter", adChapter, 0)
        Case 7: UnsupportedTypeAt = Array("PropVariant", adPropVariant, 0)
        Case 8: UnsupportedTypeAt = Array("UserDefined", adUserDefined, 0)
        Case Else: UnsupportedTypeAt = Array("ArrayInteger", adArray + adInteger, 0)
    End Select
End Function

Function ArrayAt(values, index)
    ArrayAt = values(index Mod (UBound(values) + 1))
End Function

Function SafeName(text)
    SafeName = Replace(Replace(Replace(text, " ", "_"), ".", "_"), "-", "_")
End Function

Function ArgText(index, defaultValue)
    If WScript.Arguments.Count > index Then
        ArgText = WScript.Arguments(index)
    Else
        ArgText = defaultValue
    End If
End Function

Sub WriteCoverage(def, scenario, result, xmlPath, adtgPath, rtPath, errorNumber, errorDescription)
    WriteLine fso.BuildPath(root, "coverage.csv"), Csv(Array(def(0), def(1), scenario, result, xmlPath, adtgPath, rtPath, errorNumber, errorDescription))
End Sub

Sub WriteUnsupported(def, result, errorNumber, errorDescription)
    WriteLine fso.BuildPath(root, "unsupported.csv"), Csv(Array(def(0), def(1), result, errorNumber, errorDescription))
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
