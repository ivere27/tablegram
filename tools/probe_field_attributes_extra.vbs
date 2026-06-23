Option Explicit

Const adUseClient = 3
Const adInteger = 3
Const adVarWChar = 202
Const adLongVarWChar = 203
Const adDecimal = 14
Const adPersistADTG = 0
Const adPersistXML = 1
Const adFldIsNullable = 32
Const adFldLong = 128
Const adFldRowID = 256
Const adFldIsChapter = 8192
Const adFldNegativeScale = 16384
Const adFldIsRowURL = 65536
Const adFldIsDefaultStream = 131072
Const adFldIsCollection = 262144

Dim fso, root, xmlPath, adtgPath, failureCount
Set fso = CreateObject("Scripting.FileSystemObject")
root = fso.BuildPath(CreateObject("WScript.Shell").ExpandEnvironmentStrings("%TEMP%"), "ado_field_attributes_extra_probe")
EnsureFolder root
xmlPath = fso.BuildPath(root, "field_attributes_extra.xml")
adtgPath = fso.BuildPath(root, "field_attributes_extra.adtg")
DeleteIfExists xmlPath
DeleteIfExists adtgPath
failureCount = 0

Dim rs, clone
Set rs = CreateObject("ADODB.Recordset")
rs.CursorLocation = adUseClient
AppendProbeField rs, "ROW_ID_INT", adInteger, 0, adFldRowID
AppendProbeField rs, "ROW_URL_TEXT", adVarWChar, 120, adFldIsNullable + adFldIsRowURL
AppendProbeField rs, "DEFAULT_STREAM_TEXT", adLongVarWChar, 4000, adFldIsNullable + adFldLong + adFldIsDefaultStream
AppendProbeField rs, "COLLECTION_TEXT", adVarWChar, 120, adFldIsNullable + adFldIsCollection
AppendProbeField rs, "CHAPTER_INT", adInteger, 0, adFldIsChapter
AppendProbeField rs, "NEG_SCALE_DEC", adDecimal, 0, adFldIsNullable + adFldNegativeScale

On Error Resume Next
rs.Fields("NEG_SCALE_DEC").Precision = 9
rs.Fields("NEG_SCALE_DEC").NumericScale = 2
Err.Clear
On Error GoTo 0

If failureCount > 0 Then
    WScript.Echo "append failures=" & CStr(failureCount)
End If

rs.Open

SetValue rs

On Error Resume Next
rs.UpdateBatch 3
If Err.Number <> 0 Then
    WScript.Echo "UpdateBatch failure err=" & CStr(Err.Number) & " desc=" & Err.Description
    Err.Clear
End If
Set clone = rs.Clone
rs.Save xmlPath, adPersistXML
If Err.Number <> 0 Then
    WScript.Echo "XML save failure err=" & CStr(Err.Number) & " desc=" & Err.Description
    Err.Clear
End If
clone.Save adtgPath, adPersistADTG
If Err.Number <> 0 Then
    WScript.Echo "ADTG save failure err=" & CStr(Err.Number) & " desc=" & Err.Description
    Err.Clear
End If
On Error GoTo 0

clone.Close
rs.Close

If fso.FileExists(xmlPath) Then
    WScript.Echo xmlPath
    WScript.Echo ReadAll(xmlPath)
End If
If fso.FileExists(adtgPath) Then
    WScript.Echo adtgPath
End If

Sub SetValue(rs)
    On Error Resume Next
    rs.AddNew
    rs.Fields("ROW_ID_INT").Value = 1
    rs.Fields("ROW_URL_TEXT").Value = "row-url"
    rs.Fields("DEFAULT_STREAM_TEXT").Value = "default-stream"
    rs.Fields("COLLECTION_TEXT").Value = "collection"
    rs.Fields("CHAPTER_INT").Value = 7
    rs.Fields("NEG_SCALE_DEC").Value = "1234.56"
    If Err.Number <> 0 Then
        WScript.Echo "value assignment failure err=" & CStr(Err.Number) & " desc=" & Err.Description
        Err.Clear
    End If
    rs.Update
    If Err.Number <> 0 Then
        WScript.Echo "row update failure err=" & CStr(Err.Number) & " desc=" & Err.Description
        Err.Clear
    End If
    On Error GoTo 0
End Sub

Sub AppendProbeField(rs, name, typeCode, size, attributes)
    On Error Resume Next
    If size > 0 Then
        rs.Fields.Append name, typeCode, size, attributes
    Else
        rs.Fields.Append name, typeCode, , attributes
    End If
    If Err.Number <> 0 Then
        failureCount = failureCount + 1
        WScript.Echo "append failure field=" & name & " err=" & CStr(Err.Number) & " desc=" & Err.Description
        Err.Clear
    End If
    On Error GoTo 0
End Sub

Function ReadAll(path)
    Dim stream
    Set stream = fso.OpenTextFile(path, 1, False)
    ReadAll = stream.ReadAll
    stream.Close
End Function

Sub DeleteIfExists(path)
    If fso.FileExists(path) Then
        fso.DeleteFile path, True
    End If
End Sub

Sub EnsureFolder(path)
    If Not fso.FolderExists(path) Then
        fso.CreateFolder path
    End If
End Sub
