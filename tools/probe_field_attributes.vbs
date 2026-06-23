Option Explicit

Const adUseClient = 3
Const adInteger = 3
Const adVarWChar = 202
Const adDBTimeStamp = 135
Const adPersistADTG = 0
Const adPersistXML = 1
Const adFldMayDefer = 2
Const adFldUpdatable = 4
Const adFldUnknownUpdatable = 8
Const adFldIsNullable = 32
Const adFldMayBeNull = 64
Const adFldRowID = 256
Const adFldRowVersion = 512
Const adFldCacheDeferred = 4096
Const adFldKeyColumn = 32768

Dim fso, root, xmlPath, adtgPath
Set fso = CreateObject("Scripting.FileSystemObject")
root = fso.BuildPath(CreateObject("WScript.Shell").ExpandEnvironmentStrings("%TEMP%"), "ado_field_attributes_probe")
EnsureFolder root
xmlPath = fso.BuildPath(root, "field_attributes.xml")
adtgPath = fso.BuildPath(root, "field_attributes.adtg")
DeleteIfExists xmlPath
DeleteIfExists adtgPath

Dim rs, clone
Set rs = CreateObject("ADODB.Recordset")
rs.CursorLocation = adUseClient
AppendProbeField rs, "ID_KEY", adInteger, 0, adFldKeyColumn
AppendProbeField rs, "MAY_DEFER_TEXT", adVarWChar, 80, adFldIsNullable + adFldMayDefer
AppendProbeField rs, "MAYBENULL_TEXT", adVarWChar, 80, adFldMayBeNull
AppendProbeField rs, "UNKNOWN_TEXT", adVarWChar, 80, adFldIsNullable + adFldUnknownUpdatable
AppendProbeField rs, "ROW_VERSION_TS", adDBTimeStamp, 0, adFldRowVersion
AppendProbeField rs, "CACHE_TEXT", adVarWChar, 80, adFldIsNullable + adFldCacheDeferred
rs.Open

rs.AddNew
rs.Fields("ID_KEY").Value = 1
rs.Fields("MAY_DEFER_TEXT").Value = "defer"
rs.Fields("MAYBENULL_TEXT").Value = "maybe"
rs.Fields("UNKNOWN_TEXT").Value = "unknown"
rs.Fields("ROW_VERSION_TS").Value = DateSerial(2026, 6, 12) + TimeSerial(1, 2, 3)
rs.Fields("CACHE_TEXT").Value = "cache"
rs.Update
rs.UpdateBatch 3

Set clone = rs.Clone
rs.Save xmlPath, adPersistXML
clone.Save adtgPath, adPersistADTG
clone.Close
rs.Close

WScript.Echo xmlPath
WScript.Echo ReadAll(xmlPath)
WScript.Echo adtgPath

Sub AppendProbeField(rs, name, typeCode, size, attributes)
    On Error Resume Next
    If size > 0 Then
        rs.Fields.Append name, typeCode, size, attributes
    Else
        rs.Fields.Append name, typeCode, , attributes
    End If
    If Err.Number <> 0 Then
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
