Option Explicit

Const adUseClient = 3
Const adOpenStatic = 3
Const adLockBatchOptimistic = 4
Const adCmdFile = 256
Const adTypeText = 2
Const adTypeBinary = 1

Const adFilterNone = 0
Const adFilterPendingRecords = 1
Const adFilterAffectedRecords = 2
Const adFilterFetchedRecords = 3
Const adFilterConflictingRecords = 5

If WScript.Arguments.Count < 2 Then
    WScript.Echo "usage: cscript //nologo dump_recordset_json.vbs input output.json"
    WScript.Quit 2
End If

Dim inputPath, outputPath
inputPath = WScript.Arguments(0)
outputPath = WScript.Arguments(1)

Dim json
json = "{""source"":" & JsonString(inputPath) & _
    ",""fields"":" & DumpFields(inputPath) & _
    ",""views"":[" & _
        DumpView(inputPath, "none", adFilterNone) & "," & _
        DumpView(inputPath, "pending", adFilterPendingRecords) & "," & _
        DumpView(inputPath, "affected", adFilterAffectedRecords) & "," & _
        DumpView(inputPath, "fetched", adFilterFetchedRecords) & "," & _
        DumpView(inputPath, "conflicting", adFilterConflictingRecords) & _
    "]}"

WriteUtf8 outputPath, json

Function DumpFields(path)
    Dim rs
    Set rs = OpenRecordset(path)
    DumpFields = RecordsetFieldsJson(rs)
    rs.Close
End Function

Function RecordsetFieldsJson(rs)
    Dim i, out
    out = "["
    For i = 0 To rs.Fields.Count - 1
        If i > 0 Then out = out & ","
        out = out & FieldJson(rs.Fields(i), i)
    Next
    RecordsetFieldsJson = out & "]"
End Function

Function DumpView(path, viewName, filterValue)
    Dim rs, out, rowNo
    On Error Resume Next
    Set rs = OpenRecordset(path)
    rs.Filter = filterValue
    If Err.Number <> 0 Then
        DumpView = "{""name"":" & JsonString(viewName) & _
            ",""filter"":" & CStr(filterValue) & _
            ",""error"":" & ErrorJson(Err.Number, Err.Description) & _
            ",""rows"":[]}"
        Err.Clear
        On Error GoTo 0
        Exit Function
    End If
    On Error GoTo 0

    out = "{""name"":" & JsonString(viewName) & _
        ",""filter"":" & CStr(filterValue) & _
        ",""record_count"":" & CStr(rs.RecordCount) & _
        ",""rows"":["

    rowNo = 0
    If Not rs.EOF Then
        rs.MoveFirst
        Do Until rs.EOF
            If rowNo > 0 Then out = out & ","
            out = out & RowJson(rs, rowNo)
            rowNo = rowNo + 1
            rs.MoveNext
        Loop
    End If

    out = out & "]}"
    rs.Close
    DumpView = out
End Function

Function OpenRecordset(path)
    Dim rs
    Set rs = CreateObject("ADODB.Recordset")
    rs.CursorLocation = adUseClient
    rs.Open path, "Provider=MSPersist", adOpenStatic, adLockBatchOptimistic, adCmdFile
    Set OpenRecordset = rs
End Function

Function FieldJson(field, ordinal)
    FieldJson = "{""ordinal"":" & CStr(ordinal) & _
        ",""name"":" & JsonString(field.Name) & _
        ",""type"":" & CStr(SafeLongProp(field, "Type", -1)) & _
        ",""defined_size"":" & CStr(SafeLongProp(field, "DefinedSize", -1)) & _
        ",""actual_size"":" & CStr(SafeLongProp(field, "ActualSize", -1)) & _
        ",""attributes"":" & CStr(SafeLongProp(field, "Attributes", 0)) & _
        ",""precision"":" & CStr(SafeLongProp(field, "Precision", -1)) & _
        ",""numeric_scale"":" & CStr(SafeLongProp(field, "NumericScale", -1)) & _
        "}"
End Function

Function RowJson(rs, rowNo)
    Dim i, out
    out = "{""ordinal"":" & CStr(rowNo) & _
        ",""status"":" & CStr(rs.Status) & _
        ",""values"":["
    For i = 0 To rs.Fields.Count - 1
        If i > 0 Then out = out & ","
        out = out & ValueJson(rs.Fields(i))
    Next
    out = out & "]}"
    RowJson = out
End Function

Function ValueJson(field)
    Dim value, typeCode, errNo, errDesc, chapter
    If SafeLongProp(field, "Type", -1) = 136 Then
        On Error Resume Next
        Set chapter = field.Value
        errNo = Err.Number
        errDesc = Err.Description
        Err.Clear
        On Error GoTo 0

        If errNo <> 0 Then
            ValueJson = "{""kind"":""error"",""error"":" & ErrorJson(errNo, errDesc) & "}"
        Else
            ValueJson = ChapterJson(chapter)
        End If
        Exit Function
    End If

    On Error Resume Next
    value = field.Value
    errNo = Err.Number
    errDesc = Err.Description
    Err.Clear
    On Error GoTo 0

    If errNo <> 0 Then
        ValueJson = "{""kind"":""error"",""error"":" & ErrorJson(errNo, errDesc) & "}"
        Exit Function
    End If

    If IsNull(value) Then
        ValueJson = "{""kind"":""null""}"
        Exit Function
    End If

    If IsEmpty(value) Then
        ValueJson = "{""kind"":""empty""}"
        Exit Function
    End If

    typeCode = VarType(value)
    If (typeCode And 8192) <> 0 Then
        ValueJson = "{""kind"":""binary_hex"",""value"":" & JsonString(ByteArrayHex(value)) & "}"
        Exit Function
    End If

    Select Case typeCode
        Case 7
            ValueJson = "{""kind"":""date_time"",""value"":" & JsonString(IsoDate(value)) & "}"
        Case 11
            If CBool(value) Then
                ValueJson = "{""kind"":""boolean"",""value"":true}"
            Else
                ValueJson = "{""kind"":""boolean"",""value"":false}"
            End If
        Case 2, 3, 4, 5, 6, 14, 16, 17, 18, 19, 20, 21
            ValueJson = "{""kind"":""number"",""value"":" & JsonString(CStr(value)) & "}"
        Case Else
            ValueJson = "{""kind"":""string"",""value"":" & JsonString(CStr(value)) & "}"
    End Select
End Function

Function ChapterJson(rs)
    ChapterJson = "{""kind"":""chapter"",""fields"":" & RecordsetFieldsJson(rs) & _
        ",""rows"":" & RecordsetRowsJson(rs) & "}"
End Function

Function RecordsetRowsJson(rs)
    Dim out, rowNo
    out = "["
    rowNo = 0
    If Not rs.EOF Then
        rs.MoveFirst
        Do Until rs.EOF
            If rowNo > 0 Then out = out & ","
            out = out & RowJson(rs, rowNo)
            rowNo = rowNo + 1
            rs.MoveNext
        Loop
    End If
    RecordsetRowsJson = out & "]"
End Function

Function SafeLongProp(field, propName, defaultValue)
    On Error Resume Next
    Select Case propName
        Case "Type": SafeLongProp = CLng(field.Type)
        Case "DefinedSize": SafeLongProp = CLng(field.DefinedSize)
        Case "ActualSize": SafeLongProp = CLng(field.ActualSize)
        Case "Attributes": SafeLongProp = CLng(field.Attributes)
        Case "Precision": SafeLongProp = CLng(field.Precision)
        Case "NumericScale": SafeLongProp = CLng(field.NumericScale)
        Case Else: SafeLongProp = defaultValue
    End Select
    If Err.Number <> 0 Then
        SafeLongProp = defaultValue
        Err.Clear
    End If
    On Error GoTo 0
End Function

Function ByteArrayHex(value)
    Dim i, out, stream, text
    On Error Resume Next
    out = ""
    For i = LBound(value) To UBound(value)
        out = out & HexByte(CInt(value(i)))
    Next
    If Err.Number = 0 Then
        ByteArrayHex = out
        On Error GoTo 0
        Exit Function
    End If
    Err.Clear
    On Error GoTo 0

    Set stream = CreateObject("ADODB.Stream")
    stream.Type = adTypeBinary
    stream.Open
    stream.Write value
    stream.Position = 0
    stream.Type = adTypeText
    stream.Charset = "iso-8859-1"
    text = stream.ReadText
    stream.Close

    out = ""
    For i = 1 To Len(text)
        out = out & HexByte(AscW(Mid(text, i, 1)))
    Next
    ByteArrayHex = out
End Function

Function HexByte(value)
    HexByte = Right("0" & Hex(value And 255), 2)
End Function

Function IsoDate(value)
    Dim base, totalMillis, secondValue, millis, fraction, hourValue, minuteValue, serialValue
    serialValue = CDbl(value)
    hourValue = Hour(value)
    minuteValue = Minute(value)

    If serialValue >= 1 Then
        base = DateSerial(Year(value), Month(value), Day(value)) + TimeSerial(hourValue, minuteValue, 0)
        totalMillis = Fix(((serialValue - CDbl(base)) * 86400000) + 0.5)
        secondValue = totalMillis \ 1000
        millis = totalMillis Mod 1000
        If millis >= 999 Then
            secondValue = secondValue + 1
            millis = 0
        End If
        If secondValue >= 60 Then
            secondValue = 0
            minuteValue = minuteValue + 1
        End If
    Else
        secondValue = Second(value)
        millis = 0
    End If

    fraction = ""
    If millis <> 0 Then
        fraction = "." & TrimTrailingZeros(Right("000" & CStr(millis), 3))
    End If

    IsoDate = Pad4(Year(value)) & "-" & Pad2(Month(value)) & "-" & Pad2(Day(value)) & _
        "T" & Pad2(hourValue) & ":" & Pad2(minuteValue) & ":" & Pad2(secondValue) & fraction
End Function

Function TrimTrailingZeros(value)
    Do While Len(value) > 0 And Right(value, 1) = "0"
        value = Left(value, Len(value) - 1)
    Loop
    TrimTrailingZeros = value
End Function

Function Pad2(value)
    Pad2 = Right("0" & CStr(value), 2)
End Function

Function Pad4(value)
    Pad4 = Right("0000" & CStr(value), 4)
End Function

Function ErrorJson(number, description)
    ErrorJson = "{""number"":" & CStr(number) & ",""description"":" & JsonString(description) & "}"
End Function

Function JsonString(value)
    Dim text, out, i, code, ch
    text = CStr(value)
    out = """"
    For i = 1 To Len(text)
        ch = Mid(text, i, 1)
        code = AscW(ch)
        If code < 0 Then code = code + 65536
        Select Case ch
            Case """"
                out = out & "\"""
            Case "\"
                out = out & "\\"
            Case vbCr
                out = out & "\r"
            Case vbLf
                out = out & "\n"
            Case vbTab
                out = out & "\t"
            Case Else
                If code < 32 Then
                    out = out & "\u" & Right("0000" & Hex(code), 4)
                Else
                    out = out & ch
                End If
        End Select
    Next
    JsonString = out & """"
End Function

Sub WriteUtf8(path, text)
    Dim stream
    Set stream = CreateObject("ADODB.Stream")
    stream.Type = adTypeText
    stream.Charset = "utf-8"
    stream.Open
    stream.WriteText text
    stream.SaveToFile path, 2
    stream.Close
End Sub
