Option Explicit

Const adUseClient = 3
Const adInteger = 3
Const adDate = 7
Const adPersistADTG = 0
Const adPersistXML = 1

Dim fso, root, xmlPath, adtgPath
Set fso = CreateObject("Scripting.FileSystemObject")
root = fso.BuildPath(CreateObject("WScript.Shell").ExpandEnvironmentStrings("%TEMP%"), "ado_pre_epoch_probe")
EnsureFolder root
xmlPath = fso.BuildPath(root, "pre_epoch_date_vbs.xml")
adtgPath = fso.BuildPath(root, "pre_epoch_date_vbs.adtg")
DeleteIfExists xmlPath
DeleteIfExists adtgPath

Dim rs, clone
Set rs = CreateObject("ADODB.Recordset")
rs.CursorLocation = adUseClient
rs.Fields.Append "ID", adInteger
rs.Fields.Append "D", adDate
rs.Open

AddDateRow rs, 1, DateSerial(1899, 12, 29) + TimeSerial(12, 34, 56)
AddDateRow rs, 2, DateSerial(1899, 12, 29) + TimeSerial(0, 0, 1)
AddDateRow rs, 3, DateSerial(1899, 12, 28) + TimeSerial(23, 59, 59)
AddDateRow rs, 4, DateSerial(1899, 12, 30) + TimeSerial(0, 0, 0)

rs.UpdateBatch 3
Set clone = rs.Clone
rs.Save xmlPath, adPersistXML
clone.Save adtgPath, adPersistADTG
clone.Close
rs.Close

WScript.Echo xmlPath
WScript.Echo ReadAll(xmlPath)
WScript.Echo adtgPath

Sub AddDateRow(rs, rowId, value)
    On Error Resume Next
    rs.AddNew
    rs.Fields("ID").Value = rowId
    rs.Fields("D").Value = value
    If Err.Number <> 0 Then
        WScript.Echo "assign failure row=" & CStr(rowId) & " err=" & CStr(Err.Number) & " desc=" & Err.Description
        Err.Clear
    End If
    rs.Update
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
