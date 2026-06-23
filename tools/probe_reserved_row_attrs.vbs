Option Explicit

Const adUseClient = 3
Const adInteger = 3
Const adVarWChar = 202
Const adFldIsNullable = 32
Const adAffectAll = 3
Const adPersistADTG = 0
Const adPersistXML = 1

Dim fso, root, xmlPath, adtgPath, rs
Set fso = CreateObject("Scripting.FileSystemObject")
root = fso.BuildPath(fso.GetParentFolderName(WScript.ScriptFullName), "..\corpus\fuzz")
xmlPath = fso.BuildPath(root, "probe_reserved_forcenull.xml")
adtgPath = fso.BuildPath(root, "probe_reserved_forcenull.adtg")

DeleteIfExists xmlPath
DeleteIfExists adtgPath

Set rs = CreateObject("ADODB.Recordset")
rs.CursorLocation = adUseClient
rs.Fields.Append "ID", adInteger
rs.Fields.Append "forcenull", adVarWChar, 80, adFldIsNullable
rs.Fields.Append "VALUE_FIELD", adVarWChar, 80, adFldIsNullable
rs.Open

rs.AddNew
rs.Fields("ID").Value = 1
rs.Fields("forcenull").Value = "field-value-1"
rs.Fields("VALUE_FIELD").Value = Null
rs.Update

rs.AddNew
rs.Fields("ID").Value = 2
rs.Fields("forcenull").Value = Null
rs.Fields("VALUE_FIELD").Value = "value-2"
rs.Update

rs.UpdateBatch adAffectAll
rs.MoveFirst
rs.Fields("forcenull").Value = "field-value-1-updated"
rs.Fields("VALUE_FIELD").Value = Null
rs.Update
rs.Save xmlPath, adPersistXML
rs.Save adtgPath, adPersistADTG
rs.Close

WScript.Echo fso.OpenTextFile(xmlPath, 1).ReadAll

Sub DeleteIfExists(path)
    If fso.FileExists(path) Then
        fso.DeleteFile path, True
    End If
End Sub
