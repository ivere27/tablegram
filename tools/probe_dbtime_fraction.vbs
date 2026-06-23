Option Explicit

Const adUseClient = 3
Const adInteger = 3
Const adDBTime = 134
Const adPersistADTG = 0
Const adPersistXML = 1
Const adFldIsNullable = 32

Dim fso, root, xmlPath, adtgPath
Set fso = CreateObject("Scripting.FileSystemObject")
root = fso.BuildPath(CreateObject("WScript.Shell").ExpandEnvironmentStrings("%TEMP%"), "ado_dbtime_fraction_probe")
EnsureFolder root
xmlPath = fso.BuildPath(root, "dbtime_fraction.xml")
adtgPath = fso.BuildPath(root, "dbtime_fraction.adtg")
DeleteIfExists xmlPath
DeleteIfExists adtgPath

Dim rs, clone, failureCount
failureCount = 0
Set rs = CreateObject("ADODB.Recordset")
rs.CursorLocation = adUseClient
rs.Fields.Append "ID", adInteger
rs.Fields.Append "T", adDBTime, , adFldIsNullable
rs.Open

AddTimeRow rs, 1, "03:04:05.123"
AddTimeRow rs, 2, "04:05:06.987"
AddTimeRow rs, 3, Null

If failureCount > 0 Then
    WScript.Echo "MDAC rejected fractional adDBTime assignment before persistence; failures=" & CStr(failureCount)
    rs.Close
    WScript.Quit 0
End If

rs.UpdateBatch 3
Set clone = rs.Clone
rs.Save xmlPath, adPersistXML
clone.Save adtgPath, adPersistADTG
clone.Close
rs.Close

WScript.Echo xmlPath
WScript.Echo ReadAll(xmlPath)
WScript.Echo adtgPath

Sub AddTimeRow(rs, rowId, value)
    On Error Resume Next
    rs.AddNew
    rs.Fields("ID").Value = rowId
    rs.Fields("T").Value = value
    If Err.Number <> 0 Then
        failureCount = failureCount + 1
        WScript.Echo "assign failure row=" & CStr(rowId) & " err=" & CStr(Err.Number) & " desc=" & Err.Description
        Err.Clear
        rs.CancelUpdate
        On Error GoTo 0
        Exit Sub
    End If
    rs.Update
    If Err.Number <> 0 Then
        failureCount = failureCount + 1
        WScript.Echo "update failure row=" & CStr(rowId) & " err=" & CStr(Err.Number) & " desc=" & Err.Description
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
