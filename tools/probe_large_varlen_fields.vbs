Option Explicit

Const adUseClient = 3
Const adInteger = 3
Const adVarChar = 200
Const adVarWChar = 202
Const adVarBinary = 204
Const adPersistADTG = 0
Const adPersistXML = 1
Const adFldIsNullable = 32
Const adTypeBinary = 1
Const adTypeText = 2

Dim fso, root, xmlPath, adtgPath
Set fso = CreateObject("Scripting.FileSystemObject")
root = fso.BuildPath(CreateObject("WScript.Shell").ExpandEnvironmentStrings("%TEMP%"), "ado_large_varlen_probe")
EnsureFolder root
xmlPath = fso.BuildPath(root, "large_varlen_fields.xml")
adtgPath = fso.BuildPath(root, "large_varlen_fields.adtg")
DeleteIfExists xmlPath
DeleteIfExists adtgPath

Dim rs, clone
Set rs = CreateObject("ADODB.Recordset")
rs.CursorLocation = adUseClient
rs.Fields.Append "ID", adInteger
rs.Fields.Append "VC300", adVarChar, 300, adFldIsNullable
rs.Fields.Append "VWC300", adVarWChar, 300, adFldIsNullable
rs.Fields.Append "VB300", adVarBinary, 300, adFldIsNullable
rs.Open

rs.AddNew
rs.Fields("ID").Value = 1
rs.Fields("VC300").Value = LongAscii("A", 260)
rs.Fields("VWC300").Value = LongUnicode(260)
rs.Fields("VB300").Value = Bytes(260)
rs.Update

rs.AddNew
rs.Fields("ID").Value = 2
rs.Fields("VC300").Value = Null
rs.Fields("VWC300").Value = Null
rs.Fields("VB300").Value = Null
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

Function LongAscii(prefix, count)
    Dim out, i
    out = ""
    For i = 1 To count
        out = out & prefix
    Next
    LongAscii = out
End Function

Function LongUnicode(count)
    Dim out, i
    out = ""
    For i = 1 To count
        out = out & ChrW(&HD55C)
    Next
    LongUnicode = out
End Function

Function Bytes(count)
    Dim stream, text, i
    text = ""
    For i = 0 To count - 1
        text = text & ChrW(i Mod 256)
    Next

    Set stream = CreateObject("ADODB.Stream")
    stream.Type = adTypeText
    stream.Charset = "iso-8859-1"
    stream.Open
    stream.WriteText text
    stream.Position = 0
    stream.Type = adTypeBinary
    Bytes = stream.Read
    stream.Close
End Function

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
