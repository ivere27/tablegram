Option Explicit

Const adUseClient = 3
Const adOpenStatic = 3
Const adLockBatchOptimistic = 4
Const adCmdFile = 256
Const adPersistADTG = 0
Const adPersistXML = 1

If WScript.Arguments.Count < 3 Then
    WScript.Echo "usage: cscript //nologo roundtrip.vbs input output xml|adtg"
    WScript.Quit 2
End If

Dim inputPath, outputPath, formatName, persistFormat
inputPath = WScript.Arguments(0)
outputPath = WScript.Arguments(1)
formatName = LCase(WScript.Arguments(2))

Select Case formatName
    Case "xml"
        persistFormat = adPersistXML
    Case "adtg"
        persistFormat = adPersistADTG
    Case Else
        WScript.Echo "format must be xml or adtg"
        WScript.Quit 2
End Select

Dim fso
Set fso = CreateObject("Scripting.FileSystemObject")
If fso.FileExists(outputPath) Then fso.DeleteFile outputPath, True

Dim rs
Set rs = CreateObject("ADODB.Recordset")
rs.CursorLocation = adUseClient
rs.Open inputPath, "Provider=MSPersist", adOpenStatic, adLockBatchOptimistic, adCmdFile
rs.Save outputPath, persistFormat
rs.Close

WScript.Echo "Wrote " & outputPath

