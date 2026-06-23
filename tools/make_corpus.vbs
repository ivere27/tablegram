Option Explicit

Const adUseClient = 3
Const adPersistADTG = 0
Const adPersistXML = 1
Const adFldIsNullable = 32
Const adInteger = 3
Const adDouble = 5
Const adCurrency = 6
Const adDate = 7
Const adBoolean = 11
Const adVarChar = 200
Const adVarWChar = 202
Const adLongVarWChar = 203
Const adBinary = 128
Const adLongVarBinary = 205
Const adTypeBinary = 1
Const adTypeText = 2

Dim fso
Set fso = CreateObject("Scripting.FileSystemObject")

Dim root
root = fso.BuildPath(fso.GetParentFolderName(WScript.ScriptFullName), "..\corpus\generated")
EnsureFolder root

MakeEmpty
MakeStringsAscii
MakeStringsKoreanUnicode
MakeStringsKoreanAnsi
MakeTypesBasic
MakeNulls
MakeBinary
MakeLongText

WScript.Echo "Generated ADO corpus in " & fso.GetAbsolutePathName(root)

Sub MakeEmpty()
    Dim rs
    Set rs = NewRecordset()
    rs.Fields.Append "ID", adInteger
    rs.Fields.Append "NAME", adVarWChar, 40, adFldIsNullable
    rs.Open
    SaveBoth rs, "empty"
    rs.Close
End Sub

Sub MakeStringsAscii()
    Dim rs
    Set rs = NewRecordset()
    rs.Fields.Append "ID", adInteger
    rs.Fields.Append "NAME", adVarWChar, 80, adFldIsNullable
    rs.Fields.Append "NOTE", adVarWChar, 120, adFldIsNullable
    rs.Open

    AddStringRow rs, 1, "alpha", "plain ascii"
    AddStringRow rs, 2, "reserved", "Joe's Garage & <xml>"
    AddStringRow rs, 3, "empty", ""

    SaveBoth rs, "strings_ascii"
    rs.Close
End Sub

Sub MakeStringsKoreanUnicode()
    Dim rs
    Set rs = NewRecordset()
    rs.Fields.Append "ID", adInteger
    rs.Fields.Append "WORD", adVarWChar, 80, adFldIsNullable
    rs.Fields.Append "TAIL", adVarWChar, 80, adFldIsNullable
    rs.Open

    AddStringRow rs, 1, ChrW(&HAC00), ChrW(&HAC01)
    AddStringRow rs, 2, ChrW(&HD55C) & ChrW(&HAE00), ChrW(&HB9C8) & ChrW(&HC9C0) & ChrW(&HB9C9)
    AddStringRow rs, 3, ChrW(&HD64D) & ChrW(&HAE38) & ChrW(&HB3D9), ChrW(&HB05D) & ChrW(&HAC12)

    SaveBoth rs, "strings_korean_unicode"
    rs.Close
End Sub

Sub MakeStringsKoreanAnsi()
    Dim rs
    Set rs = NewRecordset()
    rs.Fields.Append "ID", adInteger
    rs.Fields.Append "WORD", adVarChar, 80, adFldIsNullable
    rs.Fields.Append "TAIL", adVarChar, 80, adFldIsNullable
    rs.Open

    AddStringRow rs, 1, ChrW(&HAC00), ChrW(&HAC01)
    AddStringRow rs, 2, ChrW(&HD55C) & ChrW(&HAE00), ChrW(&HB9C8) & ChrW(&HC9C0) & ChrW(&HB9C9)
    AddStringRow rs, 3, ChrW(&HD64D) & ChrW(&HAE38) & ChrW(&HB3D9), ChrW(&HB05D) & ChrW(&HAC12)

    SaveBoth rs, "strings_korean_ansi"
    rs.Close
End Sub

Sub MakeTypesBasic()
    Dim rs
    Set rs = NewRecordset()
    rs.Fields.Append "ID", adInteger
    rs.Fields.Append "FLAG", adBoolean
    rs.Fields.Append "AMOUNT", adCurrency
    rs.Fields.Append "RATIO", adDouble
    rs.Fields.Append "WHEN_AT", adDate
    rs.Open

    rs.AddNew
    rs.Fields("ID").Value = 1
    rs.Fields("FLAG").Value = True
    rs.Fields("AMOUNT").Value = CCur("1234.56")
    rs.Fields("RATIO").Value = CDbl("3.14159")
    rs.Fields("WHEN_AT").Value = DateSerial(2001, 2, 3) + TimeSerial(4, 5, 6)
    rs.Update

    rs.AddNew
    rs.Fields("ID").Value = -2
    rs.Fields("FLAG").Value = False
    rs.Fields("AMOUNT").Value = CCur("-7.89")
    rs.Fields("RATIO").Value = CDbl("-0.25")
    rs.Fields("WHEN_AT").Value = DateSerial(1999, 12, 31) + TimeSerial(23, 59, 58)
    rs.Update

    SaveBoth rs, "types_basic"
    rs.Close
End Sub

Sub MakeNulls()
    Dim rs
    Set rs = NewRecordset()
    rs.Fields.Append "ID", adInteger
    rs.Fields.Append "MAYBE_TEXT", adVarWChar, 40, adFldIsNullable
    rs.Fields.Append "MAYBE_INT", adInteger, , adFldIsNullable
    rs.Open

    rs.AddNew
    rs.Fields("ID").Value = 1
    rs.Fields("MAYBE_TEXT").Value = Null
    rs.Fields("MAYBE_INT").Value = Null
    rs.Update

    rs.AddNew
    rs.Fields("ID").Value = 2
    rs.Fields("MAYBE_TEXT").Value = ""
    rs.Fields("MAYBE_INT").Value = 0
    rs.Update

    SaveBoth rs, "nulls"
    rs.Close
End Sub

Sub MakeBinary()
    Dim rs
    Set rs = NewRecordset()
    rs.Fields.Append "ID", adInteger
    rs.Fields.Append "BYTES_FIXED", adBinary, 4, adFldIsNullable
    rs.Fields.Append "BYTES_LONG", adLongVarBinary, 64, adFldIsNullable
    rs.Open

    rs.AddNew
    rs.Fields("ID").Value = 1
    rs.Fields("BYTES_FIXED").Value = Bytes(Array(0, 1, 2, 255))
    rs.Fields("BYTES_LONG").Value = Bytes(Array(222, 173, 190, 239, 0, 16, 32, 48))
    rs.Update

    SaveBoth rs, "binary"
    rs.Close
End Sub

Sub MakeLongText()
    Dim rs
    Set rs = NewRecordset()
    rs.Fields.Append "ID", adInteger
    rs.Fields.Append "BODY", adLongVarWChar, 4000, adFldIsNullable
    rs.Open

    rs.AddNew
    rs.Fields("ID").Value = 1
    rs.Fields("BODY").Value = RepeatText("line-", 80)
    rs.Update

    SaveBoth rs, "long_text"
    rs.Close
End Sub

Function NewRecordset()
    Dim rs
    Set rs = CreateObject("ADODB.Recordset")
    rs.CursorLocation = adUseClient
    Set NewRecordset = rs
End Function

Sub AddStringRow(rs, id, word, tail)
    rs.AddNew
    rs.Fields("ID").Value = id
    If HasField(rs, "NAME") Then rs.Fields("NAME").Value = word
    If HasField(rs, "NOTE") Then rs.Fields("NOTE").Value = tail
    If HasField(rs, "WORD") Then rs.Fields("WORD").Value = word
    If HasField(rs, "TAIL") Then rs.Fields("TAIL").Value = tail
    rs.Update
End Sub

Function HasField(rs, name)
    On Error Resume Next
    Dim dummy
    dummy = rs.Fields(name).Name
    HasField = (Err.Number = 0)
    Err.Clear
    On Error GoTo 0
End Function

Sub SaveBoth(rs, name)
    Dim xmlPath, adtgPath, clone
    xmlPath = fso.BuildPath(root, name & ".xml")
    adtgPath = fso.BuildPath(root, name & ".adtg")
    DeleteIfExists xmlPath
    DeleteIfExists adtgPath

    Set clone = rs.Clone
    rs.Save xmlPath, adPersistXML
    clone.Save adtgPath, adPersistADTG
    clone.Close
End Sub

Sub DeleteIfExists(path)
    If fso.FileExists(path) Then
        fso.DeleteFile path, True
    End If
End Sub

Sub EnsureFolder(path)
    Dim parent
    If fso.FolderExists(path) Then Exit Sub
    parent = fso.GetParentFolderName(path)
    If Len(parent) > 0 And Not fso.FolderExists(parent) Then EnsureFolder parent
    fso.CreateFolder path
End Sub

Function Bytes(values)
    Dim stream, text, i
    text = ""
    For i = 0 To UBound(values)
        text = text & ChrW(CInt(values(i)))
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

Function RepeatText(text, count)
    Dim out, i
    out = ""
    For i = 1 To count
        out = out & text & CStr(i) & " "
    Next
    RepeatText = out
End Function
