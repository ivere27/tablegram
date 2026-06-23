Option Explicit

Const adUseClient = 3
Const adOpenStatic = 3
Const adLockBatchOptimistic = 4
Const adCmdText = 1
Const adCmdFile = 256
Const adPersistADTG = 0
Const adPersistXML = 1

Dim fso
Set fso = CreateObject("Scripting.FileSystemObject")

Dim server, userName, password, databaseName, outDir, stem, query
server = ArgText(0, "SERVER")
userName = ArgText(1, "USER")
password = ArgText(2, "<password>")
databaseName = ArgText(3, "AdoRecordsetSales")
outDir = ArgText(4, fso.BuildPath(CreateObject("WScript.Shell").ExpandEnvironmentStrings("%TEMP%"), "ado_shape_probe"))
stem = ArgText(5, "shape_probe")
query = ArgText(6, "")

If Len(query) = 0 Then
    WScript.Echo "usage: cscript //nologo tools\probe_shape_query.vbs server user password database out_dir stem query"
    WScript.Quit 2
End If

EnsureFolder outDir

Dim cn
Set cn = CreateObject("ADODB.Connection")
cn.ConnectionTimeout = 15
cn.CommandTimeout = 120
cn.Open "Provider=MSDataShape;Data Provider=SQLOLEDB;Data Source=" & server & ";Initial Catalog=" & databaseName & ";User ID=" & userName & ";Password=" & password & ";"

Dim rs, xmlPath, adtgPath, roundtripPath
xmlPath = fso.BuildPath(outDir, stem & ".xml")
adtgPath = fso.BuildPath(outDir, stem & ".adtg")
roundtripPath = fso.BuildPath(outDir, stem & ".roundtrip.xml")
DeleteIfExists xmlPath
DeleteIfExists adtgPath
DeleteIfExists roundtripPath

Set rs = CreateObject("ADODB.Recordset")
rs.CursorLocation = adUseClient
rs.Open query, cn, adOpenStatic, adLockBatchOptimistic, adCmdText
rs.Save xmlPath, adPersistXML
rs.Save adtgPath, adPersistADTG
rs.Close

Set rs = CreateObject("ADODB.Recordset")
rs.CursorLocation = adUseClient
rs.Open adtgPath, "Provider=MSPersist", adOpenStatic, adLockBatchOptimistic, adCmdFile
rs.Save roundtripPath, adPersistXML
rs.Close

cn.Close
WScript.Echo xmlPath
WScript.Echo adtgPath
WScript.Echo roundtripPath

Function ArgText(index, defaultValue)
    If WScript.Arguments.Count > index Then
        ArgText = WScript.Arguments(index)
    Else
        ArgText = defaultValue
    End If
End Function

Sub DeleteIfExists(path)
    If fso.FileExists(path) Then fso.DeleteFile path, True
End Sub

Sub EnsureFolder(path)
    Dim parent
    If fso.FolderExists(path) Then Exit Sub
    parent = fso.GetParentFolderName(path)
    If Len(parent) > 0 And Not fso.FolderExists(parent) Then EnsureFolder parent
    fso.CreateFolder path
End Sub
