Option Explicit

Const adUseClient = 3
Const adInteger = 3
Const adFileTime = 64
Const adPersistADTG = 0
Const adPersistXML = 1
Const adFldIsNullable = 32

Dim fso, root, xmlPath, adtgPath
Set fso = CreateObject("Scripting.FileSystemObject")
root = fso.BuildPath(CreateObject("WScript.Shell").ExpandEnvironmentStrings("%TEMP%"), "ado_filetime_fraction_probe")
EnsureFolder root
xmlPath = fso.BuildPath(root, "filetime_fraction.xml")
adtgPath = fso.BuildPath(root, "filetime_fraction.adtg")
DeleteIfExists xmlPath
DeleteIfExists adtgPath

Dim rs, clone
Set rs = CreateObject("ADODB.Recordset")
rs.CursorLocation = adUseClient
rs.Fields.Append "ID", adInteger
rs.Fields.Append "FT", adFileTime, , adFldIsNullable
rs.Open

AddTimeRow rs, 1, "2026-01-02 03:04:05.123"
AddTimeRow rs, 2, "2026-02-03 04:05:06.987"
AddTimeRow rs, 3, Null

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
    rs.AddNew
    rs.Fields("ID").Value = rowId
    rs.Fields("FT").Value = value
    rs.Update
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
