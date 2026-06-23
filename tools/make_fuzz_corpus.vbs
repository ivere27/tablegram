Option Explicit

Const adUseClient = 3
Const adOpenStatic = 3
Const adLockBatchOptimistic = 4
Const adCmdFile = 256
Const adAffectAll = 3
Const adPersistADTG = 0
Const adPersistXML = 1
Const adFldMayDefer = 2
Const adFldUnknownUpdatable = 8
Const adFldIsNullable = 32
Const adFldMayBeNull = 64
Const adFldLong = 128
Const adFldRowID = 256
Const adFldRowVersion = 512
Const adFldNegativeScale = 16384
Const adFldCacheDeferred = 4096
Const adFldIsChapter = 8192
Const adFldKeyColumn = 32768
Const adFldIsRowURL = 65536
Const adFldIsDefaultStream = 131072
Const adFldIsCollection = 262144
Const adTypeBinary = 1
Const adTypeText = 2
Const adSaveCreateOverWrite = 2

Const adArray = 8192
Const adEmpty = 0
Const adTinyInt = 16
Const adUnsignedTinyInt = 17
Const adSmallInt = 2
Const adUnsignedSmallInt = 18
Const adInteger = 3
Const adUnsignedInt = 19
Const adBigInt = 20
Const adUnsignedBigInt = 21
Const adSingle = 4
Const adDouble = 5
Const adCurrency = 6
Const adBoolean = 11
Const adDate = 7
Const adDBDate = 133
Const adDBTime = 134
Const adDBTimeStamp = 135
Const adFileTime = 64
Const adGUID = 72
Const adBSTR = 8
Const adChar = 129
Const adWChar = 130
Const adVarChar = 200
Const adVarWChar = 202
Const adLongVarChar = 201
Const adLongVarWChar = 203
Const adBinary = 128
Const adVarBinary = 204
Const adLongVarBinary = 205
Const adNumeric = 131
Const adUserDefined = 132
Const adDecimal = 14
Const adVarNumeric = 139
Const adError = 10
Const adVariant = 12
Const adIDispatch = 9
Const adIUnknown = 13
Const adChapter = 136
Const adPropVariant = 138

Dim fso
Set fso = CreateObject("Scripting.FileSystemObject")

Dim caseCount, seed, root
caseCount = ArgNumber(0, 100)
seed = ArgNumber(1, 20260612)
root = ArgText(2, fso.BuildPath(fso.GetParentFolderName(WScript.ScriptFullName), "..\corpus\fuzz"))

EnsureFolder root
DeleteIfExists fso.BuildPath(root, "manifest.csv")
DeleteIfExists fso.BuildPath(root, "failures.csv")
DeleteIfExists fso.BuildPath(root, "type_matrix.csv")
DeleteIfExists fso.BuildPath(root, "field_attribute_matrix.csv")
DeleteIfExists fso.BuildPath(root, "schema_shape_matrix.csv")
DeleteIfExists fso.BuildPath(root, "xml_reader_matrix.csv")
DeleteIfExists fso.BuildPath(root, "stream_encoding_matrix.csv")
DeleteIfExists fso.BuildPath(root, "float_special_matrix.csv")
DeleteIfExists fso.BuildPath(root, "filter_save_matrix.csv")
DeleteIfExists fso.BuildPath(root, "utf16be_xml_stream.xml")
WriteLine fso.BuildPath(root, "manifest.csv"), "case,mode,fields,rows,xml,adtg,roundtrip_xml"
WriteLine fso.BuildPath(root, "failures.csv"), "case,stage,error_number,error_description"
WriteLine fso.BuildPath(root, "type_matrix.csv"), "type_name,type_code,result,xml,adtg,error_number,error_description"
WriteLine fso.BuildPath(root, "field_attribute_matrix.csv"), "attribute_name,field_type,attribute_flags,result,error_number,error_description"
WriteLine fso.BuildPath(root, "schema_shape_matrix.csv"), "case,stage,result,error_number,error_description"
WriteLine fso.BuildPath(root, "xml_reader_matrix.csv"), "case,stage,result,error_number,error_description"
WriteLine fso.BuildPath(root, "stream_encoding_matrix.csv"), "case,charset,result,stage,error_number,error_description"
WriteLine fso.BuildPath(root, "float_special_matrix.csv"), "type_name,type_code,value_name,result,stage,error_number,error_description"
WriteLine fso.BuildPath(root, "filter_save_matrix.csv"), "case,filter_name,filter_value,result,stage,error_number,error_description,default_view,pending_view,affected_view,conflicting_view"

Randomize CDbl(seed)

MakeTypeMatrix
MakeFieldAttributeMatrix
MakeSchemaShapeMatrix
MakeXmlReaderMatrix
MakeWideCase
MakeWide48Case
MakeWide65Case
MakeWide129Case
MakeAllSupportedTypesCase
MakeEmptyRowsetCase
MakeMultiChangeCase
MakeFilterSaveMatrix
MakeBinaryC1Case
MakeBinaryFullRangeCase
MakeBinaryZeroLengthCase
MakeLargeVarlenFieldsCase
MakeLargeFixedFieldsCase
MakeLongFlagFieldsCase
MakeFloatExtremesCase
MakeFloatSpecialMatrix
MakeRequiredFieldsCase
MakeFieldAttributesCase
MakeRowIdNegativeScaleCase
MakeFractionalTimestampCase
MakeFileTimeFractionCase
MakePreEpochDateCase
MakeTemporalExtremesCase
MakeNameMappingCase
MakeSpecialFieldNamesCase
MakeWhitespaceFieldNamesCase
MakeTextEscapesCase
MakeTextControlCharsCase
MakeReservedRowAttributeNamesCase
MakeDocumentedMinimalSchemaXmlCase
MakeDocumentedSchemaAttributeRefsXmlCase
MakeDocumentedBase64TypeFallbackXmlCase
MakeDocumentedDateTimeTzFallbackXmlCase
MakeDocumentedEmptyErrorVariantTypesXmlCase
MakeDocumentedFloatTypeAliasesXmlCase
MakeDocumentedNumericTypeAliasesXmlCase
MakeDocumentedNumberVarnumericXmlCase
MakeDocumentedNumberVarnumericSmallWidthXmlCase
MakeDocumentedNullableAttributeMatrixXmlCase
On Error Resume Next
MakeKoreanAnsiTextCase
If Err.Number <> 0 Then
    DeleteKoreanAnsiTextArtifacts
    WriteFailure "text_korean_ansi", "ansi_codepage", Err.Number, Err.Description
    Err.Clear
End If
On Error GoTo 0
MakeTextSpacesCase
MakeTextEmptyStringsCase
MakeSupplementaryUnicodeCase
MakeUtf16XmlStreamCase
MakeUtf16BeXmlStreamCase
MakeStreamEncodingMatrix

Dim caseNo
For caseNo = 0 To caseCount - 1
    On Error Resume Next
    MakeRandomCase caseNo
    If Err.Number <> 0 Then
        WriteFailure "random_" & Pad4(caseNo), "case", Err.Number, Err.Description
        Err.Clear
    End If
    On Error GoTo 0
Next

WScript.Echo "Generated COM fuzz corpus in " & fso.GetAbsolutePathName(root)

Sub MakeTypeMatrix()
    Dim i, def
    For i = 0 To TypeMatrixCount() - 1
        def = TypeMatrixAt(i)
        On Error Resume Next
        MakeSingleTypeCase def
        If Err.Number <> 0 Then
            DeleteTypeCaseArtifacts def(0)
            WriteTypeMatrix def(0), def(1), "fail", "", "", Err.Number, Err.Description
            Err.Clear
        End If
        On Error GoTo 0
    Next
End Sub

Sub MakeFieldAttributeMatrix()
    WriteFieldAttributeProbe "IsRowURL", "adVarWChar", adVarWChar, 120, adFldIsNullable + adFldIsRowURL
    WriteFieldAttributeProbe "IsDefaultStream", "adLongVarWChar", adLongVarWChar, 4000, adFldIsNullable + adFldLong + adFldIsDefaultStream
    WriteFieldAttributeProbe "IsCollection", "adVarWChar", adVarWChar, 120, adFldIsNullable + adFldIsCollection
    WriteFieldAttributeProbe "IsChapter", "adInteger", adInteger, 0, adFldIsChapter
End Sub

Sub MakeSchemaShapeMatrix()
    ProbeEmptyFieldName
    ProbeDuplicateFieldName
    ProbeZeroFieldOpen
End Sub

Sub MakeXmlReaderMatrix()
    ProbeXmlReaderMissingRequiredCurrent
    ProbeXmlReaderInvalidIntValue
    ProbeXmlReaderInvalidBooleanValue
    ProbeXmlReaderInvalidBinHexValue
    ProbeXmlReaderSimpleType "empty_type", "empty", "anything"
    ProbeXmlReaderSimpleType "error_type", "error", "5"
    ProbeXmlReaderSimpleType "variant_type", "variant", "plain text"
    ProbeXmlReaderNumberWithoutDbType
    ProbeXmlReaderNumberLen1WithoutDbType
    ProbeXmlReaderNumberLen2WithoutDbType
    ProbeXmlReaderNumberLen4OverflowWithoutDbType
    ProbeXmlReaderUnbracedUuidValue
    ProbeXmlReaderInvalidUuidValue
End Sub

Sub WriteFieldAttributeProbe(attributeName, fieldTypeName, typeCode, size, attributeFlags)
    Dim rs
    Set rs = NewRecordset()

    On Error Resume Next
    If size > 0 Then
        rs.Fields.Append attributeName & "_FIELD", typeCode, size, attributeFlags
    Else
        rs.Fields.Append attributeName & "_FIELD", typeCode, , attributeFlags
    End If
    If Err.Number <> 0 Then
        WriteFieldAttributeMatrix attributeName, fieldTypeName, attributeFlags, "fail", Err.Number, Err.Description
        Err.Clear
    Else
        WriteFieldAttributeMatrix attributeName, fieldTypeName, attributeFlags, "ok", "", ""
    End If
    On Error GoTo 0
End Sub

Sub ProbeEmptyFieldName()
    Dim rs
    Set rs = NewRecordset()

    On Error Resume Next
    rs.Fields.Append "", adInteger
    If Err.Number <> 0 Then
        WriteSchemaShapeMatrix "empty_field_name", "append", "fail", Err.Number, Err.Description
        Err.Clear
    Else
        WriteSchemaShapeMatrix "empty_field_name", "append", "ok", "", ""
    End If
    On Error GoTo 0
End Sub

Sub ProbeDuplicateFieldName()
    Dim rs
    Set rs = NewRecordset()

    On Error Resume Next
    rs.Fields.Append "DUP", adInteger
    rs.Fields.Append "DUP", adInteger
    If Err.Number <> 0 Then
        WriteSchemaShapeMatrix "duplicate_field_name", "append", "fail", Err.Number, Err.Description
        Err.Clear
    Else
        WriteSchemaShapeMatrix "duplicate_field_name", "append", "ok", "", ""
    End If
    On Error GoTo 0
End Sub

Sub ProbeZeroFieldOpen()
    Dim rs
    Set rs = NewRecordset()

    On Error Resume Next
    rs.Open
    If Err.Number <> 0 Then
        WriteSchemaShapeMatrix "zero_fields", "open", "fail", Err.Number, Err.Description
        Err.Clear
    Else
        WriteSchemaShapeMatrix "zero_fields", "open", "ok", "", ""
        rs.Close
    End If
    On Error GoTo 0
End Sub

Sub ProbeXmlReaderMissingRequiredCurrent()
    Dim path
    path = fso.BuildPath(root, "_xml_reader_missing_required_current.xml")
    DeleteIfExists path

    WriteLine path, "<xml xmlns:s=""uuid:BDC6E3F0-6DA3-11d1-A2A3-00AA00C14882"""
    WriteLine path, "     xmlns:dt=""uuid:C2F41010-65B3-11d1-A29F-00AA00C14882"""
    WriteLine path, "     xmlns:rs=""urn:schemas-microsoft-com:rowset"""
    WriteLine path, "     xmlns:z=""#RowsetSchema"">"
    WriteLine path, "  <s:Schema id=""RowsetSchema"">"
    WriteLine path, "    <s:ElementType name=""row"" content=""eltOnly"">"
    WriteLine path, "      <s:AttributeType name=""ID"" dt:type=""int"" rs:maybenull=""false""/>"
    WriteLine path, "      <s:AttributeType name=""REQ_INT"" dt:type=""int"" rs:maybenull=""false""/>"
    WriteLine path, "      <s:Extends type=""rs:rowbase""/>"
    WriteLine path, "    </s:ElementType>"
    WriteLine path, "  </s:Schema>"
    WriteLine path, "  <rs:data>"
    WriteLine path, "    <z:row ID=""1""/>"
    WriteLine path, "  </rs:data>"
    WriteLine path, "</xml>"

    ProbeXmlReaderDefaultView "missing_required_current", path
End Sub

Sub ProbeXmlReaderInvalidIntValue()
    Dim path
    path = fso.BuildPath(root, "_xml_reader_invalid_int.xml")
    DeleteIfExists path

    WriteLine path, "<xml xmlns:s=""uuid:BDC6E3F0-6DA3-11d1-A2A3-00AA00C14882"""
    WriteLine path, "     xmlns:dt=""uuid:C2F41010-65B3-11d1-A29F-00AA00C14882"""
    WriteLine path, "     xmlns:rs=""urn:schemas-microsoft-com:rowset"""
    WriteLine path, "     xmlns:z=""#RowsetSchema"">"
    WriteLine path, "  <s:Schema id=""RowsetSchema"">"
    WriteLine path, "    <s:ElementType name=""row"" content=""eltOnly"">"
    WriteLine path, "      <s:AttributeType name=""ID"" dt:type=""int""/>"
    WriteLine path, "      <s:AttributeType name=""VALUE_FIELD"" dt:type=""int""/>"
    WriteLine path, "      <s:Extends type=""rs:rowbase""/>"
    WriteLine path, "    </s:ElementType>"
    WriteLine path, "  </s:Schema>"
    WriteLine path, "  <rs:data>"
    WriteLine path, "    <z:row ID=""1"" VALUE_FIELD=""not-an-int""/>"
    WriteLine path, "  </rs:data>"
    WriteLine path, "</xml>"

    ProbeXmlReaderDefaultView "invalid_int_value", path
End Sub

Sub ProbeXmlReaderInvalidBooleanValue()
    Dim path
    path = fso.BuildPath(root, "_xml_reader_invalid_bool.xml")
    DeleteIfExists path

    WriteLine path, "<xml xmlns:s=""uuid:BDC6E3F0-6DA3-11d1-A2A3-00AA00C14882"""
    WriteLine path, "     xmlns:dt=""uuid:C2F41010-65B3-11d1-A29F-00AA00C14882"""
    WriteLine path, "     xmlns:rs=""urn:schemas-microsoft-com:rowset"""
    WriteLine path, "     xmlns:z=""#RowsetSchema"">"
    WriteLine path, "  <s:Schema id=""RowsetSchema"">"
    WriteLine path, "    <s:ElementType name=""row"" content=""eltOnly"">"
    WriteLine path, "      <s:AttributeType name=""ID"" dt:type=""int""/>"
    WriteLine path, "      <s:AttributeType name=""VALUE_FIELD"" dt:type=""boolean""/>"
    WriteLine path, "      <s:Extends type=""rs:rowbase""/>"
    WriteLine path, "    </s:ElementType>"
    WriteLine path, "  </s:Schema>"
    WriteLine path, "  <rs:data>"
    WriteLine path, "    <z:row ID=""1"" VALUE_FIELD=""maybe""/>"
    WriteLine path, "  </rs:data>"
    WriteLine path, "</xml>"

    ProbeXmlReaderDefaultView "invalid_boolean_value", path
End Sub

Sub ProbeXmlReaderInvalidBinHexValue()
    Dim path
    path = fso.BuildPath(root, "_xml_reader_invalid_bin_hex.xml")
    DeleteIfExists path

    WriteLine path, "<xml xmlns:s=""uuid:BDC6E3F0-6DA3-11d1-A2A3-00AA00C14882"""
    WriteLine path, "     xmlns:dt=""uuid:C2F41010-65B3-11d1-A29F-00AA00C14882"""
    WriteLine path, "     xmlns:rs=""urn:schemas-microsoft-com:rowset"""
    WriteLine path, "     xmlns:z=""#RowsetSchema"">"
    WriteLine path, "  <s:Schema id=""RowsetSchema"">"
    WriteLine path, "    <s:ElementType name=""row"" content=""eltOnly"">"
    WriteLine path, "      <s:AttributeType name=""ID"" dt:type=""int""/>"
    WriteLine path, "      <s:AttributeType name=""VALUE_FIELD"" dt:type=""bin.hex""/>"
    WriteLine path, "      <s:Extends type=""rs:rowbase""/>"
    WriteLine path, "    </s:ElementType>"
    WriteLine path, "  </s:Schema>"
    WriteLine path, "  <rs:data>"
    WriteLine path, "    <z:row ID=""1"" VALUE_FIELD=""ABC""/>"
    WriteLine path, "  </rs:data>"
    WriteLine path, "</xml>"

    ProbeXmlReaderDefaultView "invalid_bin_hex_value", path
End Sub

Sub ProbeXmlReaderSimpleType(caseName, typeName, valueText)
    Dim path
    path = fso.BuildPath(root, "_xml_reader_" & caseName & ".xml")
    DeleteIfExists path

    WriteLine path, "<xml xmlns:s=""uuid:BDC6E3F0-6DA3-11d1-A2A3-00AA00C14882"""
    WriteLine path, "     xmlns:dt=""uuid:C2F41010-65B3-11d1-A29F-00AA00C14882"""
    WriteLine path, "     xmlns:rs=""urn:schemas-microsoft-com:rowset"""
    WriteLine path, "     xmlns:z=""#RowsetSchema"">"
    WriteLine path, "  <s:Schema id=""RowsetSchema"">"
    WriteLine path, "    <s:ElementType name=""row"" content=""eltOnly"">"
    WriteLine path, "      <s:AttributeType name=""ID"" dt:type=""int""/>"
    WriteLine path, "      <s:AttributeType name=""VALUE_FIELD"" dt:type=""" & typeName & """ rs:maybenull=""true""/>"
    WriteLine path, "      <s:Extends type=""rs:rowbase""/>"
    WriteLine path, "    </s:ElementType>"
    WriteLine path, "  </s:Schema>"
    WriteLine path, "  <rs:data>"
    WriteLine path, "    <z:row ID=""1"" VALUE_FIELD=""" & XmlAttr(valueText) & """/>"
    WriteLine path, "    <z:row ID=""2""/>"
    WriteLine path, "  </rs:data>"
    WriteLine path, "</xml>"

    ProbeXmlReaderDefaultView caseName, path
End Sub

Sub ProbeXmlReaderNumberWithoutDbType()
    Dim path
    path = fso.BuildPath(root, "_xml_reader_number_without_dbtype.xml")
    DeleteIfExists path

    WriteLine path, "<xml xmlns:s=""uuid:BDC6E3F0-6DA3-11d1-A2A3-00AA00C14882"""
    WriteLine path, "     xmlns:dt=""uuid:C2F41010-65B3-11d1-A29F-00AA00C14882"""
    WriteLine path, "     xmlns:rs=""urn:schemas-microsoft-com:rowset"""
    WriteLine path, "     xmlns:z=""#RowsetSchema"">"
    WriteLine path, "  <s:Schema id=""RowsetSchema"">"
    WriteLine path, "    <s:ElementType name=""row"" content=""eltOnly"">"
    WriteLine path, "      <s:AttributeType name=""ID"" dt:type=""int""/>"
    WriteLine path, "      <s:AttributeType name=""VALUE_FIELD"" dt:type=""number""/>"
    WriteLine path, "      <s:Extends type=""rs:rowbase""/>"
    WriteLine path, "    </s:ElementType>"
    WriteLine path, "  </s:Schema>"
    WriteLine path, "  <rs:data>"
    WriteLine path, "    <z:row ID=""1"" VALUE_FIELD=""1234.5""/>"
    WriteLine path, "  </rs:data>"
    WriteLine path, "</xml>"

    ProbeXmlReaderDefaultView "number_without_dbtype", path
End Sub

Sub ProbeXmlReaderNumberLen1WithoutDbType()
    Dim path
    path = fso.BuildPath(root, "_xml_reader_number_len1_without_dbtype.xml")
    DeleteIfExists path

    WriteLine path, "<xml xmlns:s=""uuid:BDC6E3F0-6DA3-11d1-A2A3-00AA00C14882"""
    WriteLine path, "     xmlns:dt=""uuid:C2F41010-65B3-11d1-A29F-00AA00C14882"""
    WriteLine path, "     xmlns:rs=""urn:schemas-microsoft-com:rowset"""
    WriteLine path, "     xmlns:z=""#RowsetSchema"">"
    WriteLine path, "  <s:Schema id=""RowsetSchema"">"
    WriteLine path, "    <s:ElementType name=""row"" content=""eltOnly"">"
    WriteLine path, "      <s:AttributeType name=""ID"" dt:type=""int""/>"
    WriteLine path, "      <s:AttributeType name=""VALUE_FIELD"" dt:type=""number"" dt:maxLength=""1"" rs:maybenull=""true""/>"
    WriteLine path, "      <s:Extends type=""rs:rowbase""/>"
    WriteLine path, "    </s:ElementType>"
    WriteLine path, "  </s:Schema>"
    WriteLine path, "  <rs:data>"
    WriteLine path, "    <z:row ID=""1"" VALUE_FIELD=""1""/>"
    WriteLine path, "  </rs:data>"
    WriteLine path, "</xml>"

    ProbeXmlReaderDefaultView "number_len1_without_dbtype", path
End Sub

Sub ProbeXmlReaderNumberLen2WithoutDbType()
    Dim path
    path = fso.BuildPath(root, "_xml_reader_number_len2_without_dbtype.xml")
    DeleteIfExists path

    WriteLine path, "<xml xmlns:s=""uuid:BDC6E3F0-6DA3-11d1-A2A3-00AA00C14882"""
    WriteLine path, "     xmlns:dt=""uuid:C2F41010-65B3-11d1-A29F-00AA00C14882"""
    WriteLine path, "     xmlns:rs=""urn:schemas-microsoft-com:rowset"""
    WriteLine path, "     xmlns:z=""#RowsetSchema"">"
    WriteLine path, "  <s:Schema id=""RowsetSchema"">"
    WriteLine path, "    <s:ElementType name=""row"" content=""eltOnly"">"
    WriteLine path, "      <s:AttributeType name=""ID"" dt:type=""int""/>"
    WriteLine path, "      <s:AttributeType name=""VALUE_FIELD"" dt:type=""number"" dt:maxLength=""2"" rs:maybenull=""true""/>"
    WriteLine path, "      <s:Extends type=""rs:rowbase""/>"
    WriteLine path, "    </s:ElementType>"
    WriteLine path, "  </s:Schema>"
    WriteLine path, "  <rs:data>"
    WriteLine path, "    <z:row ID=""1"" VALUE_FIELD=""12""/>"
    WriteLine path, "  </rs:data>"
    WriteLine path, "</xml>"

    ProbeXmlReaderDefaultView "number_len2_without_dbtype", path
End Sub

Sub ProbeXmlReaderNumberLen4OverflowWithoutDbType()
    Dim path
    path = fso.BuildPath(root, "_xml_reader_number_len4_overflow_without_dbtype.xml")
    DeleteIfExists path

    WriteLine path, "<xml xmlns:s=""uuid:BDC6E3F0-6DA3-11d1-A2A3-00AA00C14882"""
    WriteLine path, "     xmlns:dt=""uuid:C2F41010-65B3-11d1-A29F-00AA00C14882"""
    WriteLine path, "     xmlns:rs=""urn:schemas-microsoft-com:rowset"""
    WriteLine path, "     xmlns:z=""#RowsetSchema"">"
    WriteLine path, "  <s:Schema id=""RowsetSchema"">"
    WriteLine path, "    <s:ElementType name=""row"" content=""eltOnly"">"
    WriteLine path, "      <s:AttributeType name=""ID"" dt:type=""int""/>"
    WriteLine path, "      <s:AttributeType name=""VALUE_FIELD"" dt:type=""number"" dt:maxLength=""4"" rs:maybenull=""true""/>"
    WriteLine path, "      <s:Extends type=""rs:rowbase""/>"
    WriteLine path, "    </s:ElementType>"
    WriteLine path, "  </s:Schema>"
    WriteLine path, "  <rs:data>"
    WriteLine path, "    <z:row ID=""1"" VALUE_FIELD=""6000.75""/>"
    WriteLine path, "  </rs:data>"
    WriteLine path, "</xml>"

    ProbeXmlReaderDefaultView "number_len4_overflow_without_dbtype", path
End Sub

Sub ProbeXmlReaderUnbracedUuidValue()
    Dim path
    path = fso.BuildPath(root, "_xml_reader_unbraced_uuid_value.xml")
    DeleteIfExists path

    WriteLine path, "<xml xmlns:s=""uuid:BDC6E3F0-6DA3-11d1-A2A3-00AA00C14882"""
    WriteLine path, "     xmlns:dt=""uuid:C2F41010-65B3-11d1-A29F-00AA00C14882"""
    WriteLine path, "     xmlns:rs=""urn:schemas-microsoft-com:rowset"""
    WriteLine path, "     xmlns:z=""#RowsetSchema"">"
    WriteLine path, "  <s:Schema id=""RowsetSchema"">"
    WriteLine path, "    <s:ElementType name=""row"" content=""eltOnly"">"
    WriteLine path, "      <s:AttributeType name=""ID"" dt:type=""int""/>"
    WriteLine path, "      <s:AttributeType name=""VALUE_FIELD"" dt:type=""uuid""/>"
    WriteLine path, "      <s:Extends type=""rs:rowbase""/>"
    WriteLine path, "    </s:ElementType>"
    WriteLine path, "  </s:Schema>"
    WriteLine path, "  <rs:data>"
    WriteLine path, "    <z:row ID=""1"" VALUE_FIELD=""00000000-0000-0000-0000-000000005678""/>"
    WriteLine path, "  </rs:data>"
    WriteLine path, "</xml>"

    ProbeXmlReaderDefaultView "unbraced_uuid_value", path
End Sub

Sub ProbeXmlReaderInvalidUuidValue()
    Dim path
    path = fso.BuildPath(root, "_xml_reader_invalid_uuid_value.xml")
    DeleteIfExists path

    WriteLine path, "<xml xmlns:s=""uuid:BDC6E3F0-6DA3-11d1-A2A3-00AA00C14882"""
    WriteLine path, "     xmlns:dt=""uuid:C2F41010-65B3-11d1-A29F-00AA00C14882"""
    WriteLine path, "     xmlns:rs=""urn:schemas-microsoft-com:rowset"""
    WriteLine path, "     xmlns:z=""#RowsetSchema"">"
    WriteLine path, "  <s:Schema id=""RowsetSchema"">"
    WriteLine path, "    <s:ElementType name=""row"" content=""eltOnly"">"
    WriteLine path, "      <s:AttributeType name=""ID"" dt:type=""int""/>"
    WriteLine path, "      <s:AttributeType name=""VALUE_FIELD"" dt:type=""uuid""/>"
    WriteLine path, "      <s:Extends type=""rs:rowbase""/>"
    WriteLine path, "    </s:ElementType>"
    WriteLine path, "  </s:Schema>"
    WriteLine path, "  <rs:data>"
    WriteLine path, "    <z:row ID=""1"" VALUE_FIELD=""{not-a-guid}""/>"
    WriteLine path, "  </rs:data>"
    WriteLine path, "</xml>"

    ProbeXmlReaderDefaultView "invalid_uuid_value", path
End Sub

Sub ProbeXmlReaderDefaultView(caseName, path)
    Dim rs, errNo, errDesc, stage, rowCount
    stage = "open"
    errNo = 0
    errDesc = ""

    On Error Resume Next
    Set rs = CreateObject("ADODB.Recordset")
    rs.CursorLocation = adUseClient
    rs.Open path, "Provider=MSPersist", adOpenStatic, adLockBatchOptimistic, adCmdFile
    errNo = Err.Number
    errDesc = Err.Description
    Err.Clear

    If errNo = 0 Then
        stage = "default_view"
        rs.Filter = 0
        errNo = Err.Number
        errDesc = Err.Description
        Err.Clear
    End If

    If errNo = 0 Then
        rowCount = rs.RecordCount
        errNo = Err.Number
        errDesc = Err.Description
        Err.Clear
    End If

    If errNo = 0 Then
        WriteXmlReaderMatrix caseName, stage, "ok", "", ""
    Else
        WriteXmlReaderMatrix caseName, stage, "fail", errNo, errDesc
    End If

    rs.Close
    On Error GoTo 0
    DeleteIfExists path
End Sub

Sub MakeSingleTypeCase(def)
    Dim name, rs, xmlPath, adtgPath, rtPath
    name = "type_" & SafeName(def(0))
    xmlPath = fso.BuildPath(root, name & ".xml")
    adtgPath = fso.BuildPath(root, name & ".adtg")
    rtPath = fso.BuildPath(root, name & ".roundtrip.xml")
    DeleteIfExists xmlPath
    DeleteIfExists adtgPath
    DeleteIfExists rtPath

    Set rs = NewRecordset()
    rs.Fields.Append "ID", adInteger
    AppendTypedField rs, "VALUE_FIELD", def
    rs.Open

    rs.AddNew
    rs.Fields("ID").Value = 1
    SetFieldValue rs, "VALUE_FIELD", def(0), 1, 0
    rs.Update
    rs.UpdateBatch adAffectAll

    SaveBoth rs, xmlPath, adtgPath
    rs.Close
    RoundtripAdtgToXml adtgPath, rtPath
    WriteTypeMatrix def(0), def(1), "ok", xmlPath, adtgPath, "", ""
End Sub

Sub DeleteTypeCaseArtifacts(typeName)
    Dim name
    name = "type_" & SafeName(typeName)
    DeleteIfExists fso.BuildPath(root, name & ".xml")
    DeleteIfExists fso.BuildPath(root, name & ".adtg")
    DeleteIfExists fso.BuildPath(root, name & ".roundtrip.xml")
End Sub

Sub MakeWideCase()
    Dim fieldCount, rowCount, rs, names(), defs(), i, rowNo
    Dim xmlPath, adtgPath, rtPath, caseName

    fieldCount = 16
    rowCount = 4
    ReDim names(fieldCount - 1)
    ReDim defs(fieldCount - 1)

    Set rs = NewRecordset()
    rs.Fields.Append "ID", adInteger
    names(0) = "ID"
    defs(0) = Array("Integer", adInteger, 0)

    For i = 1 To fieldCount - 1
        defs(i) = PersistableTypeAt((i * 7) Mod PersistableTypeCount())
        names(i) = "W" & Pad2(i) & "_" & defs(i)(0)
        AppendTypedField rs, names(i), defs(i)
    Next

    rs.Open

    For rowNo = 1 To rowCount
        rs.AddNew
        FillRow rs, names, defs, rowNo, 0, False
        rs.Update
    Next
    rs.UpdateBatch adAffectAll

    rs.MoveFirst
    FillRow rs, names, defs, 1, 1, True
    rs.Update

    rs.MoveNext
    rs.Delete

    rs.AddNew
    FillRow rs, names, defs, rowCount + 1, 2, False
    rs.Update

    caseName = "wide_0016"
    xmlPath = fso.BuildPath(root, caseName & ".xml")
    adtgPath = fso.BuildPath(root, caseName & ".adtg")
    rtPath = fso.BuildPath(root, caseName & ".roundtrip.xml")
    DeleteIfExists xmlPath
    DeleteIfExists adtgPath
    DeleteIfExists rtPath

    SaveBoth rs, xmlPath, adtgPath
    rs.Close
    RoundtripAdtgToXml adtgPath, rtPath
    WriteLine fso.BuildPath(root, "manifest.csv"), Csv(Array(caseName, "wide_update_delete_insert", fieldCount, rowCount, xmlPath, adtgPath, rtPath))
End Sub

Sub MakeWide48Case()
    Dim fieldCount, rowCount, rs, names(), defs(), i, rowNo
    Dim xmlPath, adtgPath, rtPath, caseName

    fieldCount = 48
    rowCount = 5
    ReDim names(fieldCount - 1)
    ReDim defs(fieldCount - 1)

    Set rs = NewRecordset()
    rs.Fields.Append "ID", adInteger
    names(0) = "ID"
    defs(0) = Array("Integer", adInteger, 0)

    For i = 1 To fieldCount - 1
        defs(i) = PersistableTypeAt((i * 11) Mod PersistableTypeCount())
        names(i) = "W48_" & Pad2(i) & "_" & defs(i)(0)
        AppendTypedField rs, names(i), defs(i)
    Next

    rs.Open

    For rowNo = 1 To rowCount
        rs.AddNew
        FillRow rs, names, defs, rowNo, 0, False
        rs.Update
    Next
    rs.UpdateBatch adAffectAll

    rs.MoveFirst
    FillRow rs, names, defs, 1, 1, True
    rs.Update

    rs.MoveNext
    rs.Delete

    rs.AddNew
    FillRow rs, names, defs, rowCount + 1, 2, False
    rs.Update

    caseName = "wide_0048"
    xmlPath = fso.BuildPath(root, caseName & ".xml")
    adtgPath = fso.BuildPath(root, caseName & ".adtg")
    rtPath = fso.BuildPath(root, caseName & ".roundtrip.xml")
    DeleteIfExists xmlPath
    DeleteIfExists adtgPath
    DeleteIfExists rtPath

    SaveBoth rs, xmlPath, adtgPath
    rs.Close
    RoundtripAdtgToXml adtgPath, rtPath
    WriteLine fso.BuildPath(root, "manifest.csv"), Csv(Array(caseName, "wide_48_update_delete_insert", fieldCount, rowCount, xmlPath, adtgPath, rtPath))
End Sub

Sub MakeWide65Case()
    Dim fieldCount, rowCount, rs, names(), defs(), i, rowNo
    Dim xmlPath, adtgPath, rtPath, caseName

    fieldCount = 65
    rowCount = 6
    ReDim names(fieldCount - 1)
    ReDim defs(fieldCount - 1)

    Set rs = NewRecordset()
    rs.Fields.Append "ID", adInteger
    names(0) = "ID"
    defs(0) = Array("Integer", adInteger, 0)

    For i = 1 To fieldCount - 1
        defs(i) = PersistableTypeAt((i * 13) Mod PersistableTypeCount())
        names(i) = "W65_" & Pad2(i) & "_" & defs(i)(0)
        AppendTypedField rs, names(i), defs(i)
    Next

    rs.Open

    For rowNo = 1 To rowCount
        rs.AddNew
        FillRow rs, names, defs, rowNo, 0, False
        rs.Update
    Next
    rs.UpdateBatch adAffectAll

    rs.MoveFirst
    FillRow rs, names, defs, 1, 1, True
    rs.Update

    rs.MoveNext
    rs.Delete

    rs.AddNew
    FillRow rs, names, defs, rowCount + 1, 2, False
    rs.Update

    caseName = "wide_0065"
    xmlPath = fso.BuildPath(root, caseName & ".xml")
    adtgPath = fso.BuildPath(root, caseName & ".adtg")
    rtPath = fso.BuildPath(root, caseName & ".roundtrip.xml")
    DeleteIfExists xmlPath
    DeleteIfExists adtgPath
    DeleteIfExists rtPath

    SaveBoth rs, xmlPath, adtgPath
    rs.Close
    RoundtripAdtgToXml adtgPath, rtPath
    WriteLine fso.BuildPath(root, "manifest.csv"), Csv(Array(caseName, "wide_65_update_delete_insert", fieldCount, rowCount, xmlPath, adtgPath, rtPath))
End Sub

Sub MakeWide129Case()
    Dim fieldCount, rowCount, rs, names(), defs(), i, rowNo
    Dim xmlPath, adtgPath, rtPath, caseName

    fieldCount = 129
    rowCount = 7
    ReDim names(fieldCount - 1)
    ReDim defs(fieldCount - 1)

    Set rs = NewRecordset()
    rs.Fields.Append "ID", adInteger
    names(0) = "ID"
    defs(0) = Array("Integer", adInteger, 0)

    For i = 1 To fieldCount - 1
        defs(i) = PersistableTypeAt((i * 17) Mod PersistableTypeCount())
        names(i) = "W129_" & Pad2(i) & "_" & defs(i)(0)
        AppendTypedField rs, names(i), defs(i)
    Next

    rs.Open

    For rowNo = 1 To rowCount
        rs.AddNew
        FillRow rs, names, defs, rowNo, 0, False
        rs.Update
    Next
    rs.UpdateBatch adAffectAll

    rs.MoveFirst
    FillRow rs, names, defs, 1, 1, True
    rs.Update

    rs.MoveNext
    rs.Delete

    rs.AddNew
    FillRow rs, names, defs, rowCount + 1, 2, False
    rs.Update

    caseName = "wide_0129"
    xmlPath = fso.BuildPath(root, caseName & ".xml")
    adtgPath = fso.BuildPath(root, caseName & ".adtg")
    rtPath = fso.BuildPath(root, caseName & ".roundtrip.xml")
    DeleteIfExists xmlPath
    DeleteIfExists adtgPath
    DeleteIfExists rtPath

    SaveBoth rs, xmlPath, adtgPath
    rs.Close
    RoundtripAdtgToXml adtgPath, rtPath
    WriteLine fso.BuildPath(root, "manifest.csv"), Csv(Array(caseName, "wide_129_update_delete_insert", fieldCount, rowCount, xmlPath, adtgPath, rtPath))
End Sub

Sub MakeAllSupportedTypesCase()
    Dim fieldCount, rowCount, rs, names(), defs(), i, rowNo, def
    Dim xmlPath, adtgPath, rtPath, caseName

    fieldCount = SupportedFlatTypeCount() + 1
    rowCount = 4
    ReDim names(fieldCount - 1)
    ReDim defs(fieldCount - 1)

    Set rs = NewRecordset()
    rs.Fields.Append "ID", adInteger
    names(0) = "ID"
    defs(0) = Array("Integer", adInteger, 0)

    For i = 0 To SupportedFlatTypeCount() - 1
        def = SupportedFlatTypeAt(i)
        names(i + 1) = "ALL_" & Pad2(i + 1) & "_" & def(0)
        defs(i + 1) = def
        AppendTypedField rs, names(i + 1), defs(i + 1)
    Next

    rs.Open

    For rowNo = 1 To rowCount
        rs.AddNew
        FillRow rs, names, defs, rowNo, 0, False
        rs.Update
    Next
    rs.UpdateBatch adAffectAll

    rs.MoveFirst
    FillRow rs, names, defs, 1, 1, True
    rs.Update

    rs.MoveNext
    rs.Delete

    rs.AddNew
    FillRow rs, names, defs, rowCount + 1, 2, False
    rs.Update

    caseName = "all_supported_types"
    xmlPath = fso.BuildPath(root, caseName & ".xml")
    adtgPath = fso.BuildPath(root, caseName & ".adtg")
    rtPath = fso.BuildPath(root, caseName & ".roundtrip.xml")
    DeleteIfExists xmlPath
    DeleteIfExists adtgPath
    DeleteIfExists rtPath

    SaveBoth rs, xmlPath, adtgPath
    rs.Close
    RoundtripAdtgToXml adtgPath, rtPath
    WriteLine fso.BuildPath(root, "manifest.csv"), Csv(Array(caseName, "all_supported_types_update_delete_insert", fieldCount, rowCount, xmlPath, adtgPath, rtPath))
End Sub

Sub MakeEmptyRowsetCase()
    Dim rs, xmlPath, adtgPath, rtPath, caseName

    Set rs = NewRecordset()
    rs.Fields.Append "ID", adInteger
    rs.Fields.Append "EMPTY_TEXT", adVarWChar, 80, adFldIsNullable
    rs.Fields.Append "EMPTY_BIN", adVarBinary, 16, adFldIsNullable
    rs.Fields.Append "EMPTY_TS", adDBTimeStamp, , adFldIsNullable
    rs.Open

    caseName = "empty_rowset"
    xmlPath = fso.BuildPath(root, caseName & ".xml")
    adtgPath = fso.BuildPath(root, caseName & ".adtg")
    rtPath = fso.BuildPath(root, caseName & ".roundtrip.xml")
    DeleteIfExists xmlPath
    DeleteIfExists adtgPath
    DeleteIfExists rtPath

    SaveBoth rs, xmlPath, adtgPath
    rs.Close
    RoundtripAdtgToXml adtgPath, rtPath
    WriteLine fso.BuildPath(root, "manifest.csv"), Csv(Array(caseName, "empty_rowset", 4, 0, xmlPath, adtgPath, rtPath))
End Sub

Sub MakeMultiChangeCase()
    Dim fieldCount, rowCount, rs, names(), defs(), rowNo
    Dim xmlPath, adtgPath, rtPath, caseName

    fieldCount = 7
    rowCount = 6
    ReDim names(fieldCount - 1)
    ReDim defs(fieldCount - 1)

    Set rs = NewRecordset()
    rs.Fields.Append "ID", adInteger
    names(0) = "ID"
    defs(0) = Array("Integer", adInteger, 0)

    defs(1) = Array("VarWChar", adVarWChar, 80)
    defs(2) = Array("Integer", adInteger, 0)
    defs(3) = Array("Numeric", adNumeric, 0)
    defs(4) = Array("DBTimeStamp", adDBTimeStamp, 0)
    defs(5) = Array("VarBinary", adVarBinary, 32)
    defs(6) = Array("Boolean", adBoolean, 0)

    names(1) = "M01_TEXT"
    names(2) = "M02_INT"
    names(3) = "M03_NUM"
    names(4) = "M04_TS"
    names(5) = "M05_BIN"
    names(6) = "M06_BOOL"

    For rowNo = 1 To fieldCount - 1
        AppendTypedField rs, names(rowNo), defs(rowNo)
    Next

    rs.Open

    For rowNo = 1 To rowCount
        rs.AddNew
        FillRow rs, names, defs, rowNo, 0, False
        rs.Update
    Next
    rs.UpdateBatch adAffectAll

    rs.MoveFirst
    FillRow rs, names, defs, 1, 1, True
    rs.Update

    rs.MoveNext
    rs.Delete

    rs.MoveNext
    FillRow rs, names, defs, 3, 2, True
    rs.Update

    rs.MoveNext
    rs.MoveNext
    rs.Delete

    rs.AddNew
    FillRow rs, names, defs, 7, 3, False
    rs.Update

    rs.AddNew
    FillRow rs, names, defs, 8, 4, False
    rs.Update

    caseName = "multi_changes"
    xmlPath = fso.BuildPath(root, caseName & ".xml")
    adtgPath = fso.BuildPath(root, caseName & ".adtg")
    rtPath = fso.BuildPath(root, caseName & ".roundtrip.xml")
    DeleteIfExists xmlPath
    DeleteIfExists adtgPath
    DeleteIfExists rtPath

    SaveBoth rs, xmlPath, adtgPath
    rs.Close
    RoundtripAdtgToXml adtgPath, rtPath
    WriteLine fso.BuildPath(root, "manifest.csv"), Csv(Array(caseName, "multi_update_delete_insert", fieldCount, rowCount, xmlPath, adtgPath, rtPath))
End Sub

Sub MakeBinaryC1Case()
    Dim rs, xmlPath, adtgPath, rtPath, caseName

    Set rs = NewRecordset()
    rs.Fields.Append "ID", adInteger
    rs.Fields.Append "B_FIXED", adBinary, 32, adFldIsNullable
    rs.Fields.Append "B_VAR", adVarBinary, 40, adFldIsNullable
    rs.Fields.Append "B_LONG", adLongVarBinary, 4000, adFldIsNullable
    rs.Open

    rs.AddNew
    rs.Fields("ID").Value = 1
    rs.Fields("B_FIXED").Value = Bytes(ByteRange(&H80, &H9F))
    rs.Fields("B_VAR").Value = Bytes(ByteRange(&H7C, &HA3))
    rs.Fields("B_LONG").Value = Bytes(ByteCycle(&H80, 96))
    rs.Update

    rs.AddNew
    rs.Fields("ID").Value = 2
    rs.Fields("B_FIXED").Value = Bytes(ByteRange(0, &H1F))
    rs.Fields("B_VAR").Value = Bytes(ByteRange(&H20, &H47))
    rs.Fields("B_LONG").Value = Bytes(ByteCycle(&H60, 96))
    rs.Update

    rs.AddNew
    rs.Fields("ID").Value = 3
    rs.Fields("B_FIXED").Value = Null
    rs.Fields("B_VAR").Value = Bytes(Array(&H80, &H81, &H8D, &H8F, &H90, &H9D, &H9E, &H9F))
    rs.Fields("B_LONG").Value = Null
    rs.Update

    rs.UpdateBatch adAffectAll

    caseName = "binary_c1"
    xmlPath = fso.BuildPath(root, caseName & ".xml")
    adtgPath = fso.BuildPath(root, caseName & ".adtg")
    rtPath = fso.BuildPath(root, caseName & ".roundtrip.xml")
    DeleteIfExists xmlPath
    DeleteIfExists adtgPath
    DeleteIfExists rtPath

    SaveBoth rs, xmlPath, adtgPath
    rs.Close
    RoundtripAdtgToXml adtgPath, rtPath
    WriteLine fso.BuildPath(root, "manifest.csv"), Csv(Array(caseName, "binary_c1", 4, 3, xmlPath, adtgPath, rtPath))
End Sub

Sub MakeBinaryFullRangeCase()
    Dim rs, xmlPath, adtgPath, rtPath, caseName

    Set rs = NewRecordset()
    rs.Fields.Append "ID", adInteger
    rs.Fields.Append "B_FIXED_256", adBinary, 256, adFldIsNullable
    rs.Fields.Append "B_VAR_256", adVarBinary, 256, adFldIsNullable
    rs.Fields.Append "B_LONG_256", adLongVarBinary, 4000, adFldIsNullable
    rs.Open

    rs.AddNew
    rs.Fields("ID").Value = 1
    SetBinaryFullRangeValues rs, ByteRange(0, &HFF)
    rs.Update

    rs.AddNew
    rs.Fields("ID").Value = 2
    SetBinaryFullRangeValues rs, ByteRangeDescending(&HFF, 0)
    rs.Update

    rs.AddNew
    rs.Fields("ID").Value = 3
    rs.Fields("B_FIXED_256").Value = Null
    rs.Fields("B_VAR_256").Value = Null
    rs.Fields("B_LONG_256").Value = Null
    rs.Update

    rs.UpdateBatch adAffectAll

    rs.MoveFirst
    SetBinaryFullRangeValues rs, ByteCycle(&H11, 256)
    rs.Update

    rs.MoveNext
    rs.Delete

    rs.AddNew
    rs.Fields("ID").Value = 4
    SetBinaryFullRangeValues rs, ByteCycle(&H40, 256)
    rs.Update

    caseName = "binary_full_range"
    xmlPath = fso.BuildPath(root, caseName & ".xml")
    adtgPath = fso.BuildPath(root, caseName & ".adtg")
    rtPath = fso.BuildPath(root, caseName & ".roundtrip.xml")
    DeleteIfExists xmlPath
    DeleteIfExists adtgPath
    DeleteIfExists rtPath

    SaveBoth rs, xmlPath, adtgPath
    rs.Close
    RoundtripAdtgToXml adtgPath, rtPath
    WriteLine fso.BuildPath(root, "manifest.csv"), Csv(Array(caseName, "binary_full_range", 4, 3, xmlPath, adtgPath, rtPath))
End Sub

Sub SetBinaryFullRangeValues(rs, values)
    Dim payload
    payload = Bytes(values)
    rs.Fields("B_FIXED_256").Value = payload
    rs.Fields("B_VAR_256").Value = payload
    rs.Fields("B_LONG_256").Value = payload
End Sub

Sub MakeBinaryZeroLengthCase()
    Dim rs, xmlPath, adtgPath, rtPath, caseName

    Set rs = NewRecordset()
    rs.Fields.Append "ID", adInteger
    rs.Fields.Append "B_VAR_EMPTY", adVarBinary, 16, adFldIsNullable
    rs.Fields.Append "B_LONG_EMPTY", adLongVarBinary, 4000, adFldIsNullable
    rs.Open

    rs.AddNew
    rs.Fields("ID").Value = 1
    rs.Fields("B_VAR_EMPTY").Value = EmptyBytes()
    rs.Fields("B_LONG_EMPTY").Value = EmptyBytes()
    rs.Update

    rs.AddNew
    rs.Fields("ID").Value = 2
    rs.Fields("B_VAR_EMPTY").Value = Bytes(Array(0, 1, 2))
    rs.Fields("B_LONG_EMPTY").Value = Bytes(Array(&HDE, &HAD, &HBE, &HEF))
    rs.Update

    rs.AddNew
    rs.Fields("ID").Value = 3
    rs.Fields("B_VAR_EMPTY").Value = Null
    rs.Fields("B_LONG_EMPTY").Value = EmptyBytes()
    rs.Update

    rs.UpdateBatch adAffectAll

    caseName = "binary_zero_length"
    xmlPath = fso.BuildPath(root, caseName & ".xml")
    adtgPath = fso.BuildPath(root, caseName & ".adtg")
    rtPath = fso.BuildPath(root, caseName & ".roundtrip.xml")
    DeleteIfExists xmlPath
    DeleteIfExists adtgPath
    DeleteIfExists rtPath

    SaveBoth rs, xmlPath, adtgPath
    rs.Close
    RoundtripAdtgToXml adtgPath, rtPath
    WriteLine fso.BuildPath(root, "manifest.csv"), Csv(Array(caseName, "binary_zero_length", 3, 3, xmlPath, adtgPath, rtPath))
End Sub

Sub MakeLargeVarlenFieldsCase()
    Dim rs, xmlPath, adtgPath, rtPath, caseName

    Set rs = NewRecordset()
    rs.Fields.Append "ID", adInteger
    rs.Fields.Append "VC300", adVarChar, 300, adFldIsNullable
    rs.Fields.Append "VWC300", adVarWChar, 300, adFldIsNullable
    rs.Fields.Append "VB300", adVarBinary, 300, adFldIsNullable
    rs.Open

    rs.AddNew
    rs.Fields("ID").Value = 1
    rs.Fields("VC300").Value = RepeatText("A", 260)
    rs.Fields("VWC300").Value = RepeatText(ChrW(&HD55C), 260)
    rs.Fields("VB300").Value = Bytes(ByteCycle(0, 260))
    rs.Update

    rs.AddNew
    rs.Fields("ID").Value = 2
    rs.Fields("VC300").Value = Null
    rs.Fields("VWC300").Value = Null
    rs.Fields("VB300").Value = Null
    rs.Update
    rs.UpdateBatch adAffectAll

    caseName = "large_varlen_fields"
    xmlPath = fso.BuildPath(root, caseName & ".xml")
    adtgPath = fso.BuildPath(root, caseName & ".adtg")
    rtPath = fso.BuildPath(root, caseName & ".roundtrip.xml")
    DeleteIfExists xmlPath
    DeleteIfExists adtgPath
    DeleteIfExists rtPath

    SaveBoth rs, xmlPath, adtgPath
    rs.Close
    RoundtripAdtgToXml adtgPath, rtPath
    WriteLine fso.BuildPath(root, "manifest.csv"), Csv(Array(caseName, "large_varlen_fields", 4, 2, xmlPath, adtgPath, rtPath))
End Sub

Sub MakeLargeFixedFieldsCase()
    Dim rs, xmlPath, adtgPath, rtPath, caseName

    Set rs = NewRecordset()
    rs.Fields.Append "ID", adInteger
    rs.Fields.Append "FC300", adChar, 300, adFldIsNullable
    rs.Fields.Append "FWC300", adWChar, 300, adFldIsNullable
    rs.Fields.Append "FB300", adBinary, 300, adFldIsNullable
    rs.Open

    rs.AddNew
    rs.Fields("ID").Value = 1
    SetLargeFixedFieldsValues rs, RepeatText("A", 300), RepeatText(ChrW(&HD55C), 300), ByteCycle(&H01, 300)
    rs.Update

    rs.AddNew
    rs.Fields("ID").Value = 2
    SetLargeFixedFieldsValues rs, RepeatText("D", 300), RepeatText(ChrW(&HAC12), 300), ByteCycle(&H41, 300)
    rs.Update

    rs.AddNew
    rs.Fields("ID").Value = 3
    rs.Fields("FC300").Value = Null
    rs.Fields("FWC300").Value = Null
    rs.Fields("FB300").Value = Null
    rs.Update

    rs.UpdateBatch adAffectAll

    rs.MoveFirst
    SetLargeFixedFieldsValues rs, RepeatText("U", 300), RepeatText(ChrW(&HB098), 300), ByteCycle(&H31, 300)
    rs.Update

    rs.MoveNext
    rs.Delete

    rs.AddNew
    rs.Fields("ID").Value = 4
    SetLargeFixedFieldsValues rs, RepeatText("I", 300), RepeatText(ChrW(&H20AC), 300), ByteCycle(&H51, 300)
    rs.Update

    caseName = "large_fixed_fields"
    xmlPath = fso.BuildPath(root, caseName & ".xml")
    adtgPath = fso.BuildPath(root, caseName & ".adtg")
    rtPath = fso.BuildPath(root, caseName & ".roundtrip.xml")
    DeleteIfExists xmlPath
    DeleteIfExists adtgPath
    DeleteIfExists rtPath

    SaveBoth rs, xmlPath, adtgPath
    rs.Close
    RoundtripAdtgToXml adtgPath, rtPath
    WriteLine fso.BuildPath(root, "manifest.csv"), Csv(Array(caseName, "large_fixed_fields", 4, 3, xmlPath, adtgPath, rtPath))
End Sub

Sub SetLargeFixedFieldsValues(rs, asciiValue, wideValue, binaryValues)
    Dim payload
    payload = Bytes(binaryValues)
    rs.Fields("FC300").Value = asciiValue
    rs.Fields("FWC300").Value = wideValue
    rs.Fields("FB300").Value = payload
End Sub

Sub MakeLongFlagFieldsCase()
    Dim rs, xmlPath, adtgPath, rtPath, caseName

    Set rs = NewRecordset()
    rs.Fields.Append "ID", adInteger
    rs.Fields.Append "LONG_FLAG_VARWCHAR", adVarWChar, 120, adFldIsNullable + adFldLong
    rs.Fields.Append "LONG_FLAG_VARBINARY", adVarBinary, 16, adFldIsNullable + adFldLong
    rs.Open

    AddLongFlagFieldsRow rs, 1, 0
    AddLongFlagFieldsRow rs, 2, 0
    rs.AddNew
    rs.Fields("ID").Value = 3
    rs.Fields("LONG_FLAG_VARWCHAR").Value = Null
    rs.Fields("LONG_FLAG_VARBINARY").Value = Null
    rs.Update
    rs.UpdateBatch adAffectAll

    rs.MoveFirst
    SetLongFlagFieldsValues rs, 1, 1
    rs.Update

    rs.MoveNext
    rs.Delete

    AddLongFlagFieldsRow rs, 4, 2

    caseName = "long_flag_fields"
    xmlPath = fso.BuildPath(root, caseName & ".xml")
    adtgPath = fso.BuildPath(root, caseName & ".adtg")
    rtPath = fso.BuildPath(root, caseName & ".roundtrip.xml")
    DeleteIfExists xmlPath
    DeleteIfExists adtgPath
    DeleteIfExists rtPath

    SaveBoth rs, xmlPath, adtgPath
    rs.Close
    RoundtripAdtgToXml adtgPath, rtPath
    WriteLine fso.BuildPath(root, "manifest.csv"), Csv(Array(caseName, "long_flag_fields", 3, 3, xmlPath, adtgPath, rtPath))
End Sub

Sub MakeFloatExtremesCase()
    Dim rs, xmlPath, adtgPath, rtPath, caseName

    Set rs = NewRecordset()
    rs.Fields.Append "ID", adInteger
    rs.Fields.Append "SINGLE_VALUE", adSingle, , adFldIsNullable
    rs.Fields.Append "DOUBLE_VALUE", adDouble, , adFldIsNullable
    rs.Open

    AddFloatExtremesRow rs, 1, "1.25", "1.25"
    AddFloatExtremesRow rs, 2, "-3.402823E+38", "-1.7976931348623157E+308"
    AddFloatExtremesRow rs, 3, "1.401298E-45", "4.94065645841247E-324"
    AddFloatExtremesNullRow rs, 4
    rs.UpdateBatch adAffectAll

    rs.MoveFirst
    SetFloatExtremesValues rs, 1, "3.402823E+38", "1.7976931348623157E+308"
    rs.Update

    rs.MoveNext
    rs.Delete

    AddFloatExtremesRow rs, 5, "-1.401298E-45", "-4.94065645841247E-324"
    AddFloatExtremesRow rs, 6, "-0", "-0"

    caseName = "float_extremes"
    xmlPath = fso.BuildPath(root, caseName & ".xml")
    adtgPath = fso.BuildPath(root, caseName & ".adtg")
    rtPath = fso.BuildPath(root, caseName & ".roundtrip.xml")
    DeleteIfExists xmlPath
    DeleteIfExists adtgPath
    DeleteIfExists rtPath

    SaveBoth rs, xmlPath, adtgPath
    rs.Close
    RoundtripAdtgToXml adtgPath, rtPath
    WriteLine fso.BuildPath(root, "manifest.csv"), Csv(Array(caseName, "float_extremes", 3, 4, xmlPath, adtgPath, rtPath))
End Sub

Sub MakeRequiredFieldsCase()
    Dim rs, xmlPath, adtgPath, rtPath, caseName

    Set rs = NewRecordset()
    rs.Fields.Append "ID", adInteger
    rs.Fields.Append "REQ_TEXT", adVarWChar, 80
    rs.Fields.Append "REQ_INT", adInteger
    rs.Fields.Append "REQ_BIN", adVarBinary, 16
    rs.Fields.Append "REQ_TS", adDBTimeStamp
    rs.Open

    AddRequiredFieldsRow rs, 1, "alpha", 10, Array(1, 2, 3), DateSerial(2026, 1, 2) + TimeSerial(3, 4, 5)
    AddRequiredFieldsRow rs, 2, "beta", 20, Array(4, 5, 6), DateSerial(2026, 2, 3) + TimeSerial(4, 5, 6)
    AddRequiredFieldsRow rs, 3, "gamma", 30, Array(7, 8, 9), DateSerial(2026, 3, 4) + TimeSerial(5, 6, 7)
    rs.UpdateBatch adAffectAll

    rs.MoveFirst
    rs.Fields("REQ_TEXT").Value = "alpha-updated"
    rs.Fields("REQ_BIN").Value = Bytes(Array(&HAA, &HBB, &HCC))
    rs.Update

    rs.MoveNext
    rs.Delete

    AddRequiredFieldsRow rs, 4, "delta", 40, Array(&HDE, &HAD, &HBE, &HEF), DateSerial(2026, 4, 5) + TimeSerial(6, 7, 8)

    caseName = "required_fields"
    xmlPath = fso.BuildPath(root, caseName & ".xml")
    adtgPath = fso.BuildPath(root, caseName & ".adtg")
    rtPath = fso.BuildPath(root, caseName & ".roundtrip.xml")
    DeleteIfExists xmlPath
    DeleteIfExists adtgPath
    DeleteIfExists rtPath

    SaveBoth rs, xmlPath, adtgPath
    rs.Close
    RoundtripAdtgToXml adtgPath, rtPath
    WriteLine fso.BuildPath(root, "manifest.csv"), Csv(Array(caseName, "required_fields", 5, 3, xmlPath, adtgPath, rtPath))
End Sub

Sub MakeFractionalTimestampCase()
    Dim rs, xmlPath, adtgPath, rtPath, caseName

    Set rs = NewRecordset()
    rs.Fields.Append "ID", adInteger
    rs.Fields.Append "TS", adDBTimeStamp, , adFldIsNullable
    rs.Open

    AddFractionalTimestampRow rs, 1, "2026-01-02 03:04:05.123"
    AddFractionalTimestampRow rs, 2, "2026-02-03 04:05:06.5"
    AddFractionalTimestampRow rs, 3, Null
    rs.UpdateBatch adAffectAll

    rs.MoveFirst
    rs.Fields("TS").Value = "2026-01-02 03:04:05.987"
    rs.Update

    rs.MoveNext
    rs.Delete

    AddFractionalTimestampRow rs, 4, "2026-04-05 06:07:08.25"

    caseName = "fractional_timestamp"
    xmlPath = fso.BuildPath(root, caseName & ".xml")
    adtgPath = fso.BuildPath(root, caseName & ".adtg")
    rtPath = fso.BuildPath(root, caseName & ".roundtrip.xml")
    DeleteIfExists xmlPath
    DeleteIfExists adtgPath
    DeleteIfExists rtPath

    SaveBoth rs, xmlPath, adtgPath
    rs.Close
    RoundtripAdtgToXml adtgPath, rtPath
    WriteLine fso.BuildPath(root, "manifest.csv"), Csv(Array(caseName, "fractional_timestamp", 2, 3, xmlPath, adtgPath, rtPath))
End Sub

Sub AddFractionalTimestampRow(rs, rowId, timestampValue)
    rs.AddNew
    rs.Fields("ID").Value = rowId
    rs.Fields("TS").Value = timestampValue
    rs.Update
End Sub

Sub MakeFileTimeFractionCase()
    Dim rs, xmlPath, adtgPath, rtPath, caseName

    Set rs = NewRecordset()
    rs.Fields.Append "ID", adInteger
    rs.Fields.Append "FT", adFileTime, , adFldIsNullable
    rs.Open

    AddFileTimeFractionRow rs, 1, "2026-01-02 03:04:05.123"
    AddFileTimeFractionRow rs, 2, "2026-02-03 04:05:06.987"
    AddFileTimeFractionRow rs, 3, Null
    rs.UpdateBatch adAffectAll

    rs.MoveFirst
    rs.Fields("FT").Value = "2026-01-02 03:04:05.25"
    rs.Update

    rs.MoveNext
    rs.Delete

    AddFileTimeFractionRow rs, 4, "2026-04-05 06:07:08.5"

    caseName = "filetime_fraction"
    xmlPath = fso.BuildPath(root, caseName & ".xml")
    adtgPath = fso.BuildPath(root, caseName & ".adtg")
    rtPath = fso.BuildPath(root, caseName & ".roundtrip.xml")
    DeleteIfExists xmlPath
    DeleteIfExists adtgPath
    DeleteIfExists rtPath

    SaveBoth rs, xmlPath, adtgPath
    rs.Close
    RoundtripAdtgToXml adtgPath, rtPath
    WriteLine fso.BuildPath(root, "manifest.csv"), Csv(Array(caseName, "filetime_fraction", 2, 3, xmlPath, adtgPath, rtPath))
End Sub

Sub AddFileTimeFractionRow(rs, rowId, fileTimeValue)
    rs.AddNew
    rs.Fields("ID").Value = rowId
    rs.Fields("FT").Value = fileTimeValue
    rs.Update
End Sub

Sub MakePreEpochDateCase()
    Dim rs, xmlPath, adtgPath, rtPath, caseName

    Set rs = NewRecordset()
    rs.Fields.Append "ID", adInteger
    rs.Fields.Append "D", adDate, , adFldIsNullable
    rs.Open

    AddPreEpochDateRow rs, 1, DateSerial(1899, 12, 29) + TimeSerial(12, 34, 56)
    AddPreEpochDateRow rs, 2, DateSerial(1899, 12, 29) + TimeSerial(0, 0, 1)
    AddPreEpochDateRow rs, 3, Null
    rs.UpdateBatch adAffectAll

    rs.MoveFirst
    rs.MoveNext
    rs.Delete

    AddPreEpochDateRow rs, 4, DateSerial(1899, 12, 28) + TimeSerial(12, 0, 0)

    caseName = "pre_epoch_date"
    xmlPath = fso.BuildPath(root, caseName & ".xml")
    adtgPath = fso.BuildPath(root, caseName & ".adtg")
    rtPath = fso.BuildPath(root, caseName & ".roundtrip.xml")
    DeleteIfExists xmlPath
    DeleteIfExists adtgPath
    DeleteIfExists rtPath

    SaveBoth rs, xmlPath, adtgPath
    rs.Close
    RoundtripAdtgToXml adtgPath, rtPath
    WriteLine fso.BuildPath(root, "manifest.csv"), Csv(Array(caseName, "pre_epoch_date", 2, 3, xmlPath, adtgPath, rtPath))
End Sub

Sub AddPreEpochDateRow(rs, rowId, dateValue)
    rs.AddNew
    rs.Fields("ID").Value = rowId
    rs.Fields("D").Value = dateValue
    rs.Update
End Sub

Sub MakeTemporalExtremesCase()
    Dim rs, xmlPath, adtgPath, rtPath, caseName

    Set rs = NewRecordset()
    rs.Fields.Append "ID", adInteger
    rs.Fields.Append "D", adDate, , adFldIsNullable
    rs.Fields.Append "DD", adDBDate, , adFldIsNullable
    rs.Fields.Append "T", adDBTime, , adFldIsNullable
    rs.Fields.Append "TS", adDBTimeStamp, , adFldIsNullable
    rs.Fields.Append "FT", adFileTime, , adFldIsNullable
    rs.Open

    AddTemporalExtremesRow rs, 1, _
        DateSerial(100, 1, 1) + TimeSerial(0, 0, 0), _
        DateSerial(100, 1, 1), _
        TimeSerial(0, 0, 0), _
        DateSerial(100, 1, 1) + TimeSerial(0, 0, 0), _
        DateSerial(1601, 1, 1) + TimeSerial(0, 0, 0)
    AddTemporalExtremesRow rs, 2, _
        DateSerial(9999, 12, 31) + TimeSerial(23, 59, 59), _
        DateSerial(9999, 12, 31), _
        TimeSerial(23, 59, 59), _
        DateSerial(9999, 12, 31) + TimeSerial(23, 59, 59), _
        DateSerial(9999, 12, 31) + TimeSerial(23, 59, 59)
    AddTemporalExtremesNullRow rs, 3
    rs.UpdateBatch adAffectAll

    caseName = "temporal_extremes"
    xmlPath = fso.BuildPath(root, caseName & ".xml")
    adtgPath = fso.BuildPath(root, caseName & ".adtg")
    rtPath = fso.BuildPath(root, caseName & ".roundtrip.xml")
    DeleteIfExists xmlPath
    DeleteIfExists adtgPath
    DeleteIfExists rtPath

    SaveBoth rs, xmlPath, adtgPath
    rs.Close
    RoundtripAdtgToXml adtgPath, rtPath
    WriteLine fso.BuildPath(root, "manifest.csv"), Csv(Array(caseName, "temporal_extremes", 6, 3, xmlPath, adtgPath, rtPath))
End Sub

Sub AddTemporalExtremesRow(rs, rowId, dateValue, dbDateValue, dbTimeValue, timestampValue, fileTimeValue)
    rs.AddNew
    rs.Fields("ID").Value = rowId
    rs.Fields("D").Value = dateValue
    rs.Fields("DD").Value = dbDateValue
    rs.Fields("T").Value = dbTimeValue
    rs.Fields("TS").Value = timestampValue
    rs.Fields("FT").Value = fileTimeValue
    rs.Update
End Sub

Sub AddTemporalExtremesNullRow(rs, rowId)
    rs.AddNew
    rs.Fields("ID").Value = rowId
    rs.Fields("D").Value = Null
    rs.Fields("DD").Value = Null
    rs.Fields("T").Value = Null
    rs.Fields("TS").Value = Null
    rs.Fields("FT").Value = Null
    rs.Update
End Sub

Sub AddRequiredFieldsRow(rs, rowId, textValue, intValue, byteValues, dateValue)
    rs.AddNew
    rs.Fields("ID").Value = rowId
    rs.Fields("REQ_TEXT").Value = textValue
    rs.Fields("REQ_INT").Value = intValue
    rs.Fields("REQ_BIN").Value = Bytes(byteValues)
    rs.Fields("REQ_TS").Value = dateValue
    rs.Update
End Sub

Sub AddLongFlagFieldsRow(rs, rowId, phase)
    rs.AddNew
    SetLongFlagFieldsValues rs, rowId, phase
    rs.Update
End Sub

Sub SetLongFlagFieldsValues(rs, rowId, phase)
    rs.Fields("ID").Value = rowId
    rs.Fields("LONG_FLAG_VARWCHAR").Value = "longflag|" & CStr(rowId) & "|" & CStr(phase) & "|" & ChrW(&HD55C) & ChrW(&HAE00)
    rs.Fields("LONG_FLAG_VARBINARY").Value = Bytes(Array(rowId Mod 256, phase Mod 256, &HDE, &HAD, &HBE, &HEF))
End Sub

Sub AddFloatExtremesRow(rs, rowId, singleValue, doubleValue)
    rs.AddNew
    SetFloatExtremesValues rs, rowId, singleValue, doubleValue
    rs.Update
End Sub

Sub SetFloatExtremesValues(rs, rowId, singleValue, doubleValue)
    rs.Fields("ID").Value = rowId
    rs.Fields("SINGLE_VALUE").Value = singleValue
    rs.Fields("DOUBLE_VALUE").Value = doubleValue
End Sub

Sub AddFloatExtremesNullRow(rs, rowId)
    rs.AddNew
    rs.Fields("ID").Value = rowId
    rs.Fields("SINGLE_VALUE").Value = Null
    rs.Fields("DOUBLE_VALUE").Value = Null
    rs.Update
End Sub

Sub MakeFieldAttributesCase()
    Dim rs, xmlPath, adtgPath, rtPath, caseName

    Set rs = NewRecordset()
    rs.Fields.Append "ID_KEY", adInteger, , adFldKeyColumn
    rs.Fields.Append "MAY_DEFER_TEXT", adVarWChar, 80, adFldIsNullable + adFldMayDefer
    rs.Fields.Append "MAYBENULL_TEXT", adVarWChar, 80, adFldMayBeNull
    rs.Fields.Append "UNKNOWN_TEXT", adVarWChar, 80, adFldIsNullable + adFldUnknownUpdatable
    rs.Fields.Append "ROW_VERSION_TS", adDBTimeStamp, , adFldRowVersion
    rs.Fields.Append "CACHE_TEXT", adVarWChar, 80, adFldIsNullable + adFldCacheDeferred
    rs.Open

    rs.AddNew
    rs.Fields("ID_KEY").Value = 1
    rs.Fields("MAY_DEFER_TEXT").Value = "defer"
    rs.Fields("MAYBENULL_TEXT").Value = "maybe"
    rs.Fields("UNKNOWN_TEXT").Value = "unknown"
    rs.Fields("ROW_VERSION_TS").Value = DateSerial(2026, 6, 12) + TimeSerial(1, 2, 3)
    rs.Fields("CACHE_TEXT").Value = "cache"
    rs.Update
    rs.UpdateBatch adAffectAll

    caseName = "field_attributes"
    xmlPath = fso.BuildPath(root, caseName & ".xml")
    adtgPath = fso.BuildPath(root, caseName & ".adtg")
    rtPath = fso.BuildPath(root, caseName & ".roundtrip.xml")
    DeleteIfExists xmlPath
    DeleteIfExists adtgPath
    DeleteIfExists rtPath

    SaveBoth rs, xmlPath, adtgPath
    rs.Close
    RoundtripAdtgToXml adtgPath, rtPath
    WriteLine fso.BuildPath(root, "manifest.csv"), Csv(Array(caseName, "field_attributes", 6, 1, xmlPath, adtgPath, rtPath))
End Sub

Sub MakeRowIdNegativeScaleCase()
    Dim rs, xmlPath, adtgPath, rtPath, caseName

    Set rs = NewRecordset()
    rs.Fields.Append "ROW_ID_INT", adInteger, , adFldRowID
    rs.Fields.Append "NEG_SCALE_DEC", adDecimal, , adFldIsNullable + adFldNegativeScale
    rs.Fields("NEG_SCALE_DEC").Precision = 9
    rs.Fields("NEG_SCALE_DEC").NumericScale = 2
    rs.Open

    rs.AddNew
    rs.Fields("ROW_ID_INT").Value = 1
    rs.Fields("NEG_SCALE_DEC").Value = "1234.56"
    rs.Update
    rs.UpdateBatch adAffectAll

    caseName = "rowid_negative_scale"
    xmlPath = fso.BuildPath(root, caseName & ".xml")
    adtgPath = fso.BuildPath(root, caseName & ".adtg")
    rtPath = fso.BuildPath(root, caseName & ".roundtrip.xml")
    DeleteIfExists xmlPath
    DeleteIfExists adtgPath
    DeleteIfExists rtPath

    SaveBoth rs, xmlPath, adtgPath
    rs.Close
    RoundtripAdtgToXml adtgPath, rtPath
    WriteLine fso.BuildPath(root, "manifest.csv"), Csv(Array(caseName, "rowid_negative_scale", 2, 1, xmlPath, adtgPath, rtPath))
End Sub

Sub MakeNameMappingCase()
    Dim rs, xmlPath, adtgPath, rtPath, caseName

    Set rs = NewRecordset()
    rs.Fields.Append "ID", adInteger
    rs.Fields.Append "Field Space Text", adVarWChar, 80, adFldIsNullable
    rs.Fields.Append "1LeadingInteger", adInteger, , adFldIsNullable
    rs.Fields.Append "Name-With-Dash", adVarChar, 80, adFldIsNullable
    rs.Fields.Append KoreanFieldName(), adVarWChar, 80, adFldIsNullable
    rs.Open

    AddNameMappingRow rs, 1, "alpha", 10, "dash-one", KoreanValue(1)
    AddNameMappingRow rs, 2, "beta", 20, "dash-two", KoreanValue(2)
    AddNameMappingRow rs, 3, "gamma", 30, "dash-three", KoreanValue(3)
    rs.UpdateBatch adAffectAll

    rs.MoveFirst
    SetNameMappingValues rs, 1, "alpha-updated", 11, "dash-one-updated", KoreanValue(4)
    rs.Update

    rs.MoveNext
    rs.Delete

    AddNameMappingRow rs, 4, "delta", 40, "dash-four", KoreanValue(5)

    caseName = "name_mapping"
    xmlPath = fso.BuildPath(root, caseName & ".xml")
    adtgPath = fso.BuildPath(root, caseName & ".adtg")
    rtPath = fso.BuildPath(root, caseName & ".roundtrip.xml")
    DeleteIfExists xmlPath
    DeleteIfExists adtgPath
    DeleteIfExists rtPath

    SaveBoth rs, xmlPath, adtgPath
    rs.Close
    RoundtripAdtgToXml adtgPath, rtPath
    WriteLine fso.BuildPath(root, "manifest.csv"), Csv(Array(caseName, "xml_name_mapping", 5, 3, xmlPath, adtgPath, rtPath))
End Sub

Sub MakeSpecialFieldNamesCase()
    Dim rs, xmlPath, adtgPath, rtPath, caseName

    Set rs = NewRecordset()
    rs.Fields.Append "ID", adInteger
    rs.Fields.Append AmpFieldName(), adVarWChar, 80, adFldIsNullable
    rs.Fields.Append QuoteFieldName(), adVarWChar, 80, adFldIsNullable
    rs.Fields.Append ApostropheFieldName(), adVarWChar, 80, adFldIsNullable
    rs.Fields.Append LessFieldName(), adVarWChar, 80, adFldIsNullable
    rs.Fields.Append GreaterFieldName(), adVarWChar, 80, adFldIsNullable
    rs.Open

    AddSpecialFieldNamesRow rs, 1, 0
    AddSpecialFieldNamesRow rs, 2, 0
    AddSpecialFieldNamesRow rs, 3, 0
    rs.UpdateBatch adAffectAll

    rs.MoveFirst
    SetSpecialFieldNamesValues rs, 1, 1
    rs.Update

    rs.MoveNext
    rs.Delete

    AddSpecialFieldNamesRow rs, 4, 2

    caseName = "special_field_names"
    xmlPath = fso.BuildPath(root, caseName & ".xml")
    adtgPath = fso.BuildPath(root, caseName & ".adtg")
    rtPath = fso.BuildPath(root, caseName & ".roundtrip.xml")
    DeleteIfExists xmlPath
    DeleteIfExists adtgPath
    DeleteIfExists rtPath

    SaveBoth rs, xmlPath, adtgPath
    rs.Close
    RoundtripAdtgToXml adtgPath, rtPath
    WriteLine fso.BuildPath(root, "manifest.csv"), Csv(Array(caseName, "xml_special_field_names", 6, 3, xmlPath, adtgPath, rtPath))
End Sub

Sub MakeWhitespaceFieldNamesCase()
    Dim rs, xmlPath, adtgPath, rtPath, caseName

    Set rs = NewRecordset()
    rs.Fields.Append "ID", adInteger
    rs.Fields.Append SpaceOnlyFieldName(), adVarWChar, 80, adFldIsNullable
    rs.Fields.Append EdgeSpaceFieldName(), adVarWChar, 80, adFldIsNullable
    rs.Fields.Append TabFieldName(), adVarWChar, 80, adFldIsNullable
    rs.Fields.Append LfFieldName(), adVarWChar, 80, adFldIsNullable
    rs.Fields.Append CrFieldName(), adVarWChar, 80, adFldIsNullable
    rs.Open

    AddWhitespaceFieldNamesRow rs, 1, 0
    AddWhitespaceFieldNamesRow rs, 2, 0
    AddWhitespaceFieldNamesRow rs, 3, 0
    rs.UpdateBatch adAffectAll

    rs.MoveFirst
    SetWhitespaceFieldNamesValues rs, 1, 1
    rs.Update

    rs.MoveNext
    rs.Delete

    AddWhitespaceFieldNamesRow rs, 4, 2

    caseName = "whitespace_field_names"
    xmlPath = fso.BuildPath(root, caseName & ".xml")
    adtgPath = fso.BuildPath(root, caseName & ".adtg")
    rtPath = fso.BuildPath(root, caseName & ".roundtrip.xml")
    DeleteIfExists xmlPath
    DeleteIfExists adtgPath
    DeleteIfExists rtPath

    SaveBoth rs, xmlPath, adtgPath
    rs.Close
    RoundtripAdtgToXml adtgPath, rtPath
    WriteLine fso.BuildPath(root, "manifest.csv"), Csv(Array(caseName, "xml_whitespace_field_names", 6, 3, xmlPath, adtgPath, rtPath))
End Sub

Sub MakeTextEscapesCase()
    Dim rs, xmlPath, adtgPath, rtPath, caseName

    Set rs = NewRecordset()
    rs.Fields.Append "ID", adInteger
    rs.Fields.Append "TXT_VAR", adVarChar, 160, adFldIsNullable
    rs.Fields.Append "TXT_WIDE", adVarWChar, 220, adFldIsNullable
    rs.Fields.Append "TXT_LONG", adLongVarWChar, 4000, adFldIsNullable
    rs.Open

    AddTextEscapesRow rs, 1, 0
    AddTextEscapesRow rs, 2, 0
    rs.AddNew
    rs.Fields("ID").Value = 3
    rs.Fields("TXT_VAR").Value = Null
    rs.Fields("TXT_WIDE").Value = Null
    rs.Fields("TXT_LONG").Value = Null
    rs.Update
    rs.UpdateBatch adAffectAll

    rs.MoveFirst
    SetTextEscapesValues rs, 1, 1
    rs.Update

    rs.MoveNext
    rs.Delete

    AddTextEscapesRow rs, 4, 2

    caseName = "text_escapes"
    xmlPath = fso.BuildPath(root, caseName & ".xml")
    adtgPath = fso.BuildPath(root, caseName & ".adtg")
    rtPath = fso.BuildPath(root, caseName & ".roundtrip.xml")
    DeleteIfExists xmlPath
    DeleteIfExists adtgPath
    DeleteIfExists rtPath

    SaveBoth rs, xmlPath, adtgPath
    rs.Close
    RoundtripAdtgToXml adtgPath, rtPath
    WriteLine fso.BuildPath(root, "manifest.csv"), Csv(Array(caseName, "text_escaping_control_chars", 4, 3, xmlPath, adtgPath, rtPath))
End Sub

Sub MakeTextControlCharsCase()
    Dim rs, xmlPath, adtgPath, rtPath, caseName

    Set rs = NewRecordset()
    rs.Fields.Append "ID", adInteger
    rs.Fields.Append "FIXED_ANSI_CTL", adChar, 16, adFldIsNullable
    rs.Fields.Append "FIXED_WIDE_CTL", adWChar, 16, adFldIsNullable
    rs.Fields.Append "VAR_ANSI_CTL", adVarChar, 40, adFldIsNullable
    rs.Fields.Append "VAR_WIDE_CTL", adVarWChar, 40, adFldIsNullable
    rs.Fields.Append "LONG_ANSI_CTL", adLongVarChar, 4000, adFldIsNullable
    rs.Fields.Append "LONG_WIDE_CTL", adLongVarWChar, 4000, adFldIsNullable
    rs.Open

    AddTextControlCharsRow rs, 1, 0
    AddTextControlCharsRow rs, 2, 0
    rs.AddNew
    rs.Fields("ID").Value = 3
    rs.Fields("FIXED_ANSI_CTL").Value = Null
    rs.Fields("FIXED_WIDE_CTL").Value = Null
    rs.Fields("VAR_ANSI_CTL").Value = Null
    rs.Fields("VAR_WIDE_CTL").Value = Null
    rs.Fields("LONG_ANSI_CTL").Value = Null
    rs.Fields("LONG_WIDE_CTL").Value = Null
    rs.Update
    rs.UpdateBatch adAffectAll

    rs.MoveFirst
    SetTextControlCharsValues rs, 1, 1
    rs.Update

    rs.MoveNext
    rs.Delete

    AddTextControlCharsRow rs, 4, 2

    caseName = "text_controls"
    xmlPath = fso.BuildPath(root, caseName & ".xml")
    adtgPath = fso.BuildPath(root, caseName & ".adtg")
    rtPath = fso.BuildPath(root, caseName & ".roundtrip.xml")
    DeleteIfExists xmlPath
    DeleteIfExists adtgPath
    DeleteIfExists rtPath

    SaveBoth rs, xmlPath, adtgPath
    rs.Close
    RoundtripAdtgToXml adtgPath, rtPath
    WriteLine fso.BuildPath(root, "manifest.csv"), Csv(Array(caseName, "text_literal_xml_illegal_controls", 7, 3, xmlPath, adtgPath, rtPath))
End Sub

Sub MakeReservedRowAttributeNamesCase()
    Dim rs, xmlPath, adtgPath, rtPath, caseName

    Set rs = NewRecordset()
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

    caseName = "reserved_row_attrs"
    xmlPath = fso.BuildPath(root, caseName & ".xml")
    adtgPath = fso.BuildPath(root, caseName & ".adtg")
    rtPath = fso.BuildPath(root, caseName & ".roundtrip.xml")
    DeleteIfExists xmlPath
    DeleteIfExists adtgPath
    DeleteIfExists rtPath

    SaveBoth rs, xmlPath, adtgPath
    rs.Close
    RoundtripAdtgToXml adtgPath, rtPath
    WriteLine fso.BuildPath(root, "manifest.csv"), Csv(Array(caseName, "reserved_row_attribute_names", 3, 2, xmlPath, adtgPath, rtPath))
End Sub

Sub MakeDocumentedMinimalSchemaXmlCase()
    Dim caseName, xmlPath, adtgPath, rtPath

    caseName = "doc_minimal_schema"
    xmlPath = fso.BuildPath(root, caseName & ".xml")
    adtgPath = fso.BuildPath(root, caseName & ".adtg")
    rtPath = fso.BuildPath(root, caseName & ".roundtrip.xml")
    DeleteIfExists xmlPath
    DeleteIfExists adtgPath
    DeleteIfExists rtPath

    WriteLine xmlPath, "<xml xmlns:s=""uuid:BDC6E3F0-6DA3-11d1-A2A3-00AA00C14882"""
    WriteLine xmlPath, "     xmlns:dt=""uuid:C2F41010-65B3-11d1-A29F-00AA00C14882"""
    WriteLine xmlPath, "     xmlns:rs=""urn:schemas-microsoft-com:rowset"""
    WriteLine xmlPath, "     xmlns:z=""#RowsetSchema"">"
    WriteLine xmlPath, "  <s:Schema id=""RowsetSchema"">"
    WriteLine xmlPath, "    <s:ElementType name=""row"" content=""eltOnly"">"
    WriteLine xmlPath, "      <s:AttributeType name=""ID"" dt:type=""int""/>"
    WriteLine xmlPath, "      <s:AttributeType name=""s1"" rs:name=""Friendly Name""/>"
    WriteLine xmlPath, "      <s:AttributeType name=""s2"" rs:name=""Direct Int"" dt:type=""int"" rs:maybenull=""true""/>"
    WriteLine xmlPath, "      <s:AttributeType name=""s3"" rs:name=""Direct Binary"" dt:type=""bin.hex"" rs:maybenull=""true""/>"
    WriteLine xmlPath, "      <s:Extends type=""rs:rowbase""/>"
    WriteLine xmlPath, "    </s:ElementType>"
    WriteLine xmlPath, "  </s:Schema>"
    WriteLine xmlPath, "  <rs:data>"
    WriteLine xmlPath, "    <z:row ID=""1"" s1=""alpha"" s2=""42"" s3=""000102"" ignored=""this attribute is not in the schema""/>"
    WriteLine xmlPath, "    <z:row ID=""2"" s1=""""/>"
    WriteLine xmlPath, "  </rs:data>"
    WriteLine xmlPath, "</xml>"

    RoundtripXmlToAdtg xmlPath, adtgPath
    RoundtripAdtgToXml adtgPath, rtPath
    WriteLine fso.BuildPath(root, "manifest.csv"), Csv(Array(caseName, "documented_minimal_schema_xml", 4, 2, xmlPath, adtgPath, rtPath))
End Sub

Sub MakeDocumentedSchemaAttributeRefsXmlCase()
    Dim caseName, xmlPath, adtgPath, rtPath

    caseName = "doc_schema_attribute_refs"
    xmlPath = fso.BuildPath(root, caseName & ".xml")
    adtgPath = fso.BuildPath(root, caseName & ".adtg")
    rtPath = fso.BuildPath(root, caseName & ".roundtrip.xml")
    DeleteIfExists xmlPath
    DeleteIfExists adtgPath
    DeleteIfExists rtPath

    WriteLine xmlPath, "<xml xmlns:s=""uuid:BDC6E3F0-6DA3-11d1-A2A3-00AA00C14882"""
    WriteLine xmlPath, "     xmlns:dt=""uuid:C2F41010-65B3-11d1-A29F-00AA00C14882"""
    WriteLine xmlPath, "     xmlns:rs=""urn:schemas-microsoft-com:rowset"""
    WriteLine xmlPath, "     xmlns:z=""#RowsetSchema"">"
    WriteLine xmlPath, "  <s:Schema id=""RowsetSchema"">"
    WriteLine xmlPath, "    <s:ElementType name=""row"" content=""eltOnly"">"
    WriteLine xmlPath, "      <s:attribute type=""s2""/>"
    WriteLine xmlPath, "      <s:attribute type=""s1""/>"
    WriteLine xmlPath, "      <s:attribute type=""s3""/>"
    WriteLine xmlPath, "      <s:extends type=""rs:rowbase""/>"
    WriteLine xmlPath, "    </s:ElementType>"
    WriteLine xmlPath, "    <s:AttributeType name=""s1"" rs:name=""Number After Text"" dt:type=""int""/>"
    WriteLine xmlPath, "    <s:AttributeType name=""s2"" rs:name=""Text First""/>"
    WriteLine xmlPath, "    <s:AttributeType name=""unused"" dt:type=""int""/>"
    WriteLine xmlPath, "    <s:AttributeType name=""s3"" rs:name=""Binary Third"" dt:type=""bin.hex"" rs:maybenull=""true""/>"
    WriteLine xmlPath, "  </s:Schema>"
    WriteLine xmlPath, "  <rs:data>"
    WriteLine xmlPath, "    <z:row s1=""10"" s2=""alpha"" s3=""0A0B""/>"
    WriteLine xmlPath, "    <z:row s1=""20"" s2=""beta""/>"
    WriteLine xmlPath, "  </rs:data>"
    WriteLine xmlPath, "</xml>"

    RoundtripXmlToAdtg xmlPath, adtgPath
    RoundtripAdtgToXml adtgPath, rtPath
    WriteLine fso.BuildPath(root, "manifest.csv"), Csv(Array(caseName, "documented_schema_attribute_refs_xml", 3, 2, xmlPath, adtgPath, rtPath))
End Sub

Sub MakeDocumentedBase64TypeFallbackXmlCase()
    Dim caseName, xmlPath, adtgPath, rtPath

    caseName = "doc_base64_type_fallback"
    xmlPath = fso.BuildPath(root, caseName & ".xml")
    adtgPath = fso.BuildPath(root, caseName & ".adtg")
    rtPath = fso.BuildPath(root, caseName & ".roundtrip.xml")
    DeleteIfExists xmlPath
    DeleteIfExists adtgPath
    DeleteIfExists rtPath

    WriteLine xmlPath, "<xml xmlns:s=""uuid:BDC6E3F0-6DA3-11d1-A2A3-00AA00C14882"""
    WriteLine xmlPath, "     xmlns:dt=""uuid:C2F41010-65B3-11d1-A29F-00AA00C14882"""
    WriteLine xmlPath, "     xmlns:rs=""urn:schemas-microsoft-com:rowset"""
    WriteLine xmlPath, "     xmlns:z=""#RowsetSchema"">"
    WriteLine xmlPath, "  <s:Schema id=""RowsetSchema"">"
    WriteLine xmlPath, "    <s:ElementType name=""row"" content=""eltOnly"">"
    WriteLine xmlPath, "      <s:AttributeType name=""ID"" dt:type=""int""/>"
    WriteLine xmlPath, "      <s:AttributeType name=""BASE64_LONG"" dt:type=""bin.base64"" rs:maybenull=""true""/>"
    WriteLine xmlPath, "      <s:AttributeType name=""BASE64_VAR"" dt:type=""bin.base64"" dt:maxLength=""12"" rs:maybenull=""true""/>"
    WriteLine xmlPath, "      <s:AttributeType name=""BASE64_FIXED"" dt:type=""bin.base64"" dt:maxLength=""12"" rs:fixedlength=""true"" rs:maybenull=""true""/>"
    WriteLine xmlPath, "      <s:AttributeType name=""BASE64_CHILD"" rs:maybenull=""true"">"
    WriteLine xmlPath, "        <s:datatype dt:type=""bin.base64""/>"
    WriteLine xmlPath, "      </s:AttributeType>"
    WriteLine xmlPath, "      <s:Extends type=""rs:rowbase""/>"
    WriteLine xmlPath, "    </s:ElementType>"
    WriteLine xmlPath, "  </s:Schema>"
    WriteLine xmlPath, "  <rs:data>"
    WriteLine xmlPath, "    <z:row ID=""1"" BASE64_LONG=""AAECAwQF+v8="" BASE64_VAR=""YWJj"" BASE64_FIXED=""MTIzNA=="" BASE64_CHILD=""ZmllbGQ=""/>"
    WriteLine xmlPath, "    <z:row ID=""2""/>"
    WriteLine xmlPath, "  </rs:data>"
    WriteLine xmlPath, "</xml>"

    RoundtripXmlToAdtg xmlPath, adtgPath
    RoundtripAdtgToXml adtgPath, rtPath
    WriteLine fso.BuildPath(root, "manifest.csv"), Csv(Array(caseName, "documented_base64_type_fallback_xml", 5, 2, xmlPath, adtgPath, rtPath))
End Sub

Sub MakeDocumentedDateTimeTzFallbackXmlCase()
    Dim caseName, xmlPath, adtgPath, rtPath

    caseName = "doc_datetime_tz_fallback"
    xmlPath = fso.BuildPath(root, caseName & ".xml")
    adtgPath = fso.BuildPath(root, caseName & ".adtg")
    rtPath = fso.BuildPath(root, caseName & ".roundtrip.xml")
    DeleteIfExists xmlPath
    DeleteIfExists adtgPath
    DeleteIfExists rtPath

    WriteLine xmlPath, "<xml xmlns:s=""uuid:BDC6E3F0-6DA3-11d1-A2A3-00AA00C14882"""
    WriteLine xmlPath, "     xmlns:dt=""uuid:C2F41010-65B3-11d1-A29F-00AA00C14882"""
    WriteLine xmlPath, "     xmlns:rs=""urn:schemas-microsoft-com:rowset"""
    WriteLine xmlPath, "     xmlns:z=""#RowsetSchema"">"
    WriteLine xmlPath, "  <s:Schema id=""RowsetSchema"">"
    WriteLine xmlPath, "    <s:ElementType name=""row"" content=""eltOnly"">"
    WriteLine xmlPath, "      <s:AttributeType name=""ID"" dt:type=""int""/>"
    WriteLine xmlPath, "      <s:AttributeType name=""DT_TZ_LONG"" dt:type=""dateTime.tz"" rs:maybenull=""true""/>"
    WriteLine xmlPath, "      <s:AttributeType name=""DT_TZ_VAR"" dt:type=""dateTime.tz"" dt:maxLength=""32"" rs:maybenull=""true""/>"
    WriteLine xmlPath, "      <s:AttributeType name=""DT_TZ_FIXED"" dt:type=""dateTime.tz"" dt:maxLength=""32"" rs:fixedlength=""true"" rs:maybenull=""true""/>"
    WriteLine xmlPath, "      <s:AttributeType name=""DT_TZ_CHILD"" rs:maybenull=""true"">"
    WriteLine xmlPath, "        <s:datatype dt:type=""dateTime.tz""/>"
    WriteLine xmlPath, "      </s:AttributeType>"
    WriteLine xmlPath, "      <s:Extends type=""rs:rowbase""/>"
    WriteLine xmlPath, "    </s:ElementType>"
    WriteLine xmlPath, "  </s:Schema>"
    WriteLine xmlPath, "  <rs:data>"
    WriteLine xmlPath, "    <z:row ID=""1"" DT_TZ_LONG=""2026-06-12T01:02:03Z"" DT_TZ_VAR=""2026-06-12T01:02:03+09:30"" DT_TZ_FIXED=""2026-06-12T01:02:03-04:00"" DT_TZ_CHILD=""2026-06-12T01:02:03Z""/>"
    WriteLine xmlPath, "    <z:row ID=""2""/>"
    WriteLine xmlPath, "  </rs:data>"
    WriteLine xmlPath, "</xml>"

    RoundtripXmlToAdtg xmlPath, adtgPath
    RoundtripAdtgToXml adtgPath, rtPath
    WriteLine fso.BuildPath(root, "manifest.csv"), Csv(Array(caseName, "documented_datetime_tz_fallback_xml", 5, 2, xmlPath, adtgPath, rtPath))
End Sub

Sub MakeDocumentedEmptyErrorVariantTypesXmlCase()
    Dim caseName, xmlPath, adtgPath, rtPath

    caseName = "doc_empty_error_variant_types"
    xmlPath = fso.BuildPath(root, caseName & ".xml")
    adtgPath = fso.BuildPath(root, caseName & ".adtg")
    rtPath = fso.BuildPath(root, caseName & ".roundtrip.xml")
    DeleteIfExists xmlPath
    DeleteIfExists adtgPath
    DeleteIfExists rtPath

    WriteLine xmlPath, "<xml xmlns:s=""uuid:BDC6E3F0-6DA3-11d1-A2A3-00AA00C14882"""
    WriteLine xmlPath, "     xmlns:dt=""uuid:C2F41010-65B3-11d1-A29F-00AA00C14882"""
    WriteLine xmlPath, "     xmlns:rs=""urn:schemas-microsoft-com:rowset"""
    WriteLine xmlPath, "     xmlns:z=""#RowsetSchema"">"
    WriteLine xmlPath, "  <s:Schema id=""RowsetSchema"">"
    WriteLine xmlPath, "    <s:ElementType name=""row"" content=""eltOnly"">"
    WriteLine xmlPath, "      <s:AttributeType name=""ID"" dt:type=""int""/>"
    WriteLine xmlPath, "      <s:AttributeType name=""EMPTY_FIELD"" dt:type=""empty"" rs:maybenull=""true""/>"
    WriteLine xmlPath, "      <s:AttributeType name=""ERROR_FIELD"" dt:type=""error"" rs:maybenull=""true""/>"
    WriteLine xmlPath, "      <s:AttributeType name=""VARIANT_FIELD"" dt:type=""variant"" rs:maybenull=""true""/>"
    WriteLine xmlPath, "      <s:Extends type=""rs:rowbase""/>"
    WriteLine xmlPath, "    </s:ElementType>"
    WriteLine xmlPath, "  </s:Schema>"
    WriteLine xmlPath, "  <rs:data>"
    WriteLine xmlPath, "    <z:row ID=""1"" EMPTY_FIELD=""anything"" ERROR_FIELD=""5"" VARIANT_FIELD=""plain text""/>"
    WriteLine xmlPath, "    <z:row ID=""2""/>"
    WriteLine xmlPath, "  </rs:data>"
    WriteLine xmlPath, "</xml>"

    WriteLine fso.BuildPath(root, "manifest.csv"), Csv(Array(caseName, "documented_empty_error_variant_types_xml", 4, 2, xmlPath, "", ""))
End Sub

Sub MakeDocumentedFloatTypeAliasesXmlCase()
    Dim caseName, xmlPath, adtgPath, rtPath

    caseName = "doc_float_type_aliases"
    xmlPath = fso.BuildPath(root, caseName & ".xml")
    adtgPath = fso.BuildPath(root, caseName & ".adtg")
    rtPath = fso.BuildPath(root, caseName & ".roundtrip.xml")
    DeleteIfExists xmlPath
    DeleteIfExists adtgPath
    DeleteIfExists rtPath

    WriteLine xmlPath, "<xml xmlns:s=""uuid:BDC6E3F0-6DA3-11d1-A2A3-00AA00C14882"""
    WriteLine xmlPath, "     xmlns:dt=""uuid:C2F41010-65B3-11d1-A29F-00AA00C14882"""
    WriteLine xmlPath, "     xmlns:rs=""urn:schemas-microsoft-com:rowset"""
    WriteLine xmlPath, "     xmlns:z=""#RowsetSchema"">"
    WriteLine xmlPath, "  <s:Schema id=""RowsetSchema"">"
    WriteLine xmlPath, "    <s:ElementType name=""row"" content=""eltOnly"">"
    WriteLine xmlPath, "      <s:AttributeType name=""ID"" dt:type=""int""/>"
    WriteLine xmlPath, "      <s:AttributeType name=""FLOAT_NO_LEN"" dt:type=""float"" rs:maybenull=""true""/>"
    WriteLine xmlPath, "      <s:AttributeType name=""FLOAT_LEN4"" dt:type=""float"" dt:maxLength=""4"" rs:maybenull=""true""/>"
    WriteLine xmlPath, "      <s:AttributeType name=""FLOAT_LEN8"" dt:type=""float"" dt:maxLength=""8"" rs:maybenull=""true""/>"
    WriteLine xmlPath, "      <s:AttributeType name=""R4_NO_LEN"" dt:type=""r4"" rs:maybenull=""true""/>"
    WriteLine xmlPath, "      <s:AttributeType name=""R4_LEN8"" dt:type=""r4"" dt:maxLength=""8"" rs:maybenull=""true""/>"
    WriteLine xmlPath, "      <s:AttributeType name=""R8_NO_LEN"" dt:type=""r8"" rs:maybenull=""true""/>"
    WriteLine xmlPath, "      <s:AttributeType name=""R8_LEN4"" dt:type=""r8"" dt:maxLength=""4"" rs:maybenull=""true""/>"
    WriteLine xmlPath, "      <s:Extends type=""rs:rowbase""/>"
    WriteLine xmlPath, "    </s:ElementType>"
    WriteLine xmlPath, "  </s:Schema>"
    WriteLine xmlPath, "  <rs:data>"
    WriteLine xmlPath, "    <z:row ID=""1"" FLOAT_NO_LEN=""1.25"" FLOAT_LEN4=""2.5"" FLOAT_LEN8=""3.75"" R4_NO_LEN=""4.5"" R4_LEN8=""5.25"" R8_NO_LEN=""6.125"" R8_LEN4=""7.875""/>"
    WriteLine xmlPath, "    <z:row ID=""2""/>"
    WriteLine xmlPath, "  </rs:data>"
    WriteLine xmlPath, "</xml>"

    On Error Resume Next
    RoundtripXmlToAdtg xmlPath, adtgPath
    If Err.Number <> 0 Then
        WriteFailure caseName, "xml_to_adtg", Err.Number, Err.Description
        Err.Clear
        DeleteIfExists adtgPath
        DeleteIfExists rtPath
        WriteLine fso.BuildPath(root, "manifest.csv"), Csv(Array(caseName, "documented_float_type_aliases_xml", 8, 2, xmlPath, "", ""))
        On Error GoTo 0
        Exit Sub
    End If
    On Error GoTo 0

    RoundtripAdtgToXml adtgPath, rtPath
    WriteLine fso.BuildPath(root, "manifest.csv"), Csv(Array(caseName, "documented_float_type_aliases_xml", 8, 2, xmlPath, adtgPath, rtPath))
End Sub

Sub MakeDocumentedNumericTypeAliasesXmlCase()
    Dim caseName, xmlPath, adtgPath, rtPath

    caseName = "doc_numeric_type_aliases"
    xmlPath = fso.BuildPath(root, caseName & ".xml")
    adtgPath = fso.BuildPath(root, caseName & ".adtg")
    rtPath = fso.BuildPath(root, caseName & ".roundtrip.xml")
    DeleteIfExists xmlPath
    DeleteIfExists adtgPath
    DeleteIfExists rtPath

    WriteLine xmlPath, "<xml xmlns:s=""uuid:BDC6E3F0-6DA3-11d1-A2A3-00AA00C14882"""
    WriteLine xmlPath, "     xmlns:dt=""uuid:C2F41010-65B3-11d1-A29F-00AA00C14882"""
    WriteLine xmlPath, "     xmlns:rs=""urn:schemas-microsoft-com:rowset"""
    WriteLine xmlPath, "     xmlns:z=""#RowsetSchema"">"
    WriteLine xmlPath, "  <s:Schema id=""RowsetSchema"">"
    WriteLine xmlPath, "    <s:ElementType name=""row"" content=""eltOnly"">"
    WriteLine xmlPath, "      <s:AttributeType name=""ID"" dt:type=""int""/>"
    WriteLine xmlPath, "      <s:AttributeType name=""FIXED_14_4"" dt:type=""fixed.14.4"" rs:maybenull=""true""/>"
    WriteLine xmlPath, "      <s:AttributeType name=""CURRENCY_ALIAS"" dt:type=""currency"" rs:maybenull=""true""/>"
    WriteLine xmlPath, "      <s:AttributeType name=""DECIMAL_ALIAS"" dt:type=""decimal"" rs:maybenull=""true""/>"
    WriteLine xmlPath, "      <s:AttributeType name=""NUMBER_DB_CURRENCY"" dt:type=""number"" rs:dbtype=""currency"" rs:maybenull=""true""/>"
    WriteLine xmlPath, "      <s:AttributeType name=""NUMBER_DB_DECIMAL"" dt:type=""number"" rs:dbtype=""decimal"" rs:maybenull=""true""/>"
    WriteLine xmlPath, "      <s:AttributeType name=""NUMBER_DB_NUMERIC"" dt:type=""number"" rs:dbtype=""numeric"" rs:precision=""18"" rs:scale=""4"" rs:maybenull=""true""/>"
    WriteLine xmlPath, "      <s:Extends type=""rs:rowbase""/>"
    WriteLine xmlPath, "    </s:ElementType>"
    WriteLine xmlPath, "  </s:Schema>"
    WriteLine xmlPath, "  <rs:data>"
    WriteLine xmlPath, "    <z:row ID=""1"" FIXED_14_4=""1000.1234"" CURRENCY_ALIAS=""2000.5678"" DECIMAL_ALIAS=""3000.25"" NUMBER_DB_CURRENCY=""4000.1250"" NUMBER_DB_DECIMAL=""5000.50"" NUMBER_DB_NUMERIC=""6000.75""/>"
    WriteLine xmlPath, "    <z:row ID=""2""/>"
    WriteLine xmlPath, "  </rs:data>"
    WriteLine xmlPath, "</xml>"

    RoundtripXmlToAdtg xmlPath, adtgPath
    RoundtripAdtgToXml adtgPath, rtPath
    WriteLine fso.BuildPath(root, "manifest.csv"), Csv(Array(caseName, "documented_numeric_type_aliases_xml", 7, 2, xmlPath, adtgPath, rtPath))
End Sub

Sub MakeDocumentedNumberVarnumericXmlCase()
    Dim caseName, xmlPath, adtgPath, rtPath

    caseName = "doc_number_varnumeric"
    xmlPath = fso.BuildPath(root, caseName & ".xml")
    adtgPath = fso.BuildPath(root, caseName & ".adtg")
    rtPath = fso.BuildPath(root, caseName & ".roundtrip.xml")
    DeleteIfExists xmlPath
    DeleteIfExists adtgPath
    DeleteIfExists rtPath

    WriteLine xmlPath, "<xml xmlns:s=""uuid:BDC6E3F0-6DA3-11d1-A2A3-00AA00C14882"""
    WriteLine xmlPath, "     xmlns:dt=""uuid:C2F41010-65B3-11d1-A29F-00AA00C14882"""
    WriteLine xmlPath, "     xmlns:rs=""urn:schemas-microsoft-com:rowset"""
    WriteLine xmlPath, "     xmlns:z=""#RowsetSchema"">"
    WriteLine xmlPath, "  <s:Schema id=""RowsetSchema"">"
    WriteLine xmlPath, "    <s:ElementType name=""row"" content=""eltOnly"">"
    WriteLine xmlPath, "      <s:AttributeType name=""ID"" dt:type=""int""/>"
    WriteLine xmlPath, "      <s:AttributeType name=""NUMBER_LEN8"" dt:type=""number"" dt:maxLength=""8"" rs:maybenull=""true""/>"
    WriteLine xmlPath, "      <s:AttributeType name=""NUMBER_LEN16"" dt:type=""number"" dt:maxLength=""16"" rs:maybenull=""true""/>"
    WriteLine xmlPath, "      <s:AttributeType name=""NUMBER_LEN19"" dt:type=""number"" dt:maxLength=""19"" rs:maybenull=""true""/>"
    WriteLine xmlPath, "      <s:Extends type=""rs:rowbase""/>"
    WriteLine xmlPath, "    </s:ElementType>"
    WriteLine xmlPath, "  </s:Schema>"
    WriteLine xmlPath, "  <rs:data>"
    WriteLine xmlPath, "    <z:row ID=""1"" NUMBER_LEN8=""1234.5"" NUMBER_LEN16=""6000.75"" NUMBER_LEN19=""123456.7890""/>"
    WriteLine xmlPath, "    <z:row ID=""2""/>"
    WriteLine xmlPath, "  </rs:data>"
    WriteLine xmlPath, "</xml>"

    RoundtripXmlToAdtg xmlPath, adtgPath
    RoundtripAdtgToXml adtgPath, rtPath
    WriteLine fso.BuildPath(root, "manifest.csv"), Csv(Array(caseName, "documented_number_varnumeric_xml", 4, 2, xmlPath, adtgPath, rtPath))
End Sub

Sub MakeDocumentedNumberVarnumericSmallWidthXmlCase()
    Dim caseName, xmlPath

    caseName = "doc_number_varnumeric_small_width"
    xmlPath = fso.BuildPath(root, caseName & ".xml")
    DeleteIfExists xmlPath

    WriteLine xmlPath, "<xml xmlns:s=""uuid:BDC6E3F0-6DA3-11d1-A2A3-00AA00C14882"""
    WriteLine xmlPath, "     xmlns:dt=""uuid:C2F41010-65B3-11d1-A29F-00AA00C14882"""
    WriteLine xmlPath, "     xmlns:rs=""urn:schemas-microsoft-com:rowset"""
    WriteLine xmlPath, "     xmlns:z=""#RowsetSchema"">"
    WriteLine xmlPath, "  <s:Schema id=""RowsetSchema"">"
    WriteLine xmlPath, "    <s:ElementType name=""row"" content=""eltOnly"">"
    WriteLine xmlPath, "      <s:AttributeType name=""ID"" dt:type=""int""/>"
    WriteLine xmlPath, "      <s:AttributeType name=""NUMBER_LEN3_DECIMAL"" dt:type=""number"" dt:maxLength=""3"" rs:maybenull=""true""/>"
    WriteLine xmlPath, "      <s:AttributeType name=""NUMBER_LEN3_TRAILING_ZERO"" dt:type=""number"" dt:maxLength=""3"" rs:maybenull=""true""/>"
    WriteLine xmlPath, "      <s:AttributeType name=""NUMBER_LEN4_DECIMAL"" dt:type=""number"" dt:maxLength=""4"" rs:maybenull=""true""/>"
    WriteLine xmlPath, "      <s:AttributeType name=""NUMBER_LEN4_INTEGER"" dt:type=""number"" dt:maxLength=""4"" rs:maybenull=""true""/>"
    WriteLine xmlPath, "      <s:AttributeType name=""NUMBER_LEN5_DECIMAL"" dt:type=""number"" dt:maxLength=""5"" rs:maybenull=""true""/>"
    WriteLine xmlPath, "      <s:AttributeType name=""NUMBER_LEN6_DECIMAL"" dt:type=""number"" dt:maxLength=""6"" rs:maybenull=""true""/>"
    WriteLine xmlPath, "      <s:Extends type=""rs:rowbase""/>"
    WriteLine xmlPath, "    </s:ElementType>"
    WriteLine xmlPath, "  </s:Schema>"
    WriteLine xmlPath, "  <rs:data>"
    WriteLine xmlPath, "    <z:row ID=""1"" NUMBER_LEN3_DECIMAL=""12.3"" NUMBER_LEN3_TRAILING_ZERO=""100"" NUMBER_LEN4_DECIMAL=""12.25"" NUMBER_LEN4_INTEGER=""1234"" NUMBER_LEN5_DECIMAL=""6000.75"" NUMBER_LEN6_DECIMAL=""6000.75""/>"
    WriteLine xmlPath, "    <z:row ID=""2""/>"
    WriteLine xmlPath, "  </rs:data>"
    WriteLine xmlPath, "</xml>"

    WriteLine fso.BuildPath(root, "manifest.csv"), Csv(Array(caseName, "documented_number_varnumeric_small_width_xml", 7, 2, xmlPath, "", ""))
End Sub

Sub MakeDocumentedNullableAttributeMatrixXmlCase()
    Dim caseName, xmlPath, adtgPath, rtPath

    caseName = "doc_nullable_attr_matrix"
    xmlPath = fso.BuildPath(root, caseName & ".xml")
    adtgPath = fso.BuildPath(root, caseName & ".adtg")
    rtPath = fso.BuildPath(root, caseName & ".roundtrip.xml")
    DeleteIfExists xmlPath
    DeleteIfExists adtgPath
    DeleteIfExists rtPath

    WriteLine xmlPath, "<xml xmlns:s=""uuid:BDC6E3F0-6DA3-11d1-A2A3-00AA00C14882"""
    WriteLine xmlPath, "     xmlns:dt=""uuid:C2F41010-65B3-11d1-A29F-00AA00C14882"""
    WriteLine xmlPath, "     xmlns:rs=""urn:schemas-microsoft-com:rowset"""
    WriteLine xmlPath, "     xmlns:z=""#RowsetSchema"">"
    WriteLine xmlPath, "  <s:Schema id=""RowsetSchema"">"
    WriteLine xmlPath, "    <s:ElementType name=""row"" content=""eltOnly"">"
    WriteLine xmlPath, "      <s:AttributeType name=""ID"" dt:type=""int""/>"
    WriteLine xmlPath, "      <s:AttributeType name=""INT_DEFAULT"" dt:type=""int""/>"
    WriteLine xmlPath, "      <s:AttributeType name=""INT_MAYBE_TRUE"" dt:type=""int"" rs:maybenull=""true""/>"
    WriteLine xmlPath, "      <s:AttributeType name=""INT_MAYBE_FALSE"" dt:type=""int"" rs:maybenull=""false""/>"
    WriteLine xmlPath, "      <s:AttributeType name=""INT_NULLABLE_TRUE"" dt:type=""int"" rs:nullable=""true""/>"
    WriteLine xmlPath, "      <s:AttributeType name=""INT_NULLABLE_TRUE_MAYBE_FALSE"" dt:type=""int"" rs:nullable=""true"" rs:maybenull=""false""/>"
    WriteLine xmlPath, "      <s:AttributeType name=""INT_NULLABLE_FALSE_MAYBE_TRUE"" dt:type=""int"" rs:nullable=""false"" rs:maybenull=""true""/>"
    WriteLine xmlPath, "      <s:AttributeType name=""TEXT_DEFAULT""/>"
    WriteLine xmlPath, "      <s:AttributeType name=""TEXT_MAYBE_FALSE"" rs:maybenull=""false""/>"
    WriteLine xmlPath, "      <s:Extends type=""rs:rowbase""/>"
    WriteLine xmlPath, "    </s:ElementType>"
    WriteLine xmlPath, "  </s:Schema>"
    WriteLine xmlPath, "  <rs:data>"
    WriteLine xmlPath, "    <z:row ID=""1"" INT_DEFAULT=""10"" INT_MAYBE_TRUE=""11"" INT_MAYBE_FALSE=""12"" INT_NULLABLE_TRUE=""13"" INT_NULLABLE_TRUE_MAYBE_FALSE=""14"" INT_NULLABLE_FALSE_MAYBE_TRUE=""15"" TEXT_DEFAULT=""alpha"" TEXT_MAYBE_FALSE=""beta""/>"
    WriteLine xmlPath, "    <z:row ID=""2"" INT_DEFAULT=""20"" INT_MAYBE_TRUE=""21"" INT_MAYBE_FALSE=""22"" INT_NULLABLE_TRUE=""23"" INT_NULLABLE_TRUE_MAYBE_FALSE=""24"" INT_NULLABLE_FALSE_MAYBE_TRUE=""25"" TEXT_DEFAULT=""gamma"" TEXT_MAYBE_FALSE=""delta""/>"
    WriteLine xmlPath, "  </rs:data>"
    WriteLine xmlPath, "</xml>"

    RoundtripXmlToAdtg xmlPath, adtgPath
    RoundtripAdtgToXml adtgPath, rtPath
    WriteLine fso.BuildPath(root, "manifest.csv"), Csv(Array(caseName, "documented_nullable_attribute_matrix_xml", 9, 2, xmlPath, adtgPath, rtPath))
End Sub

Sub MakeKoreanAnsiTextCase()
    Dim rs, xmlPath, adtgPath, rtPath, caseName

    Set rs = NewRecordset()
    rs.Fields.Append "ID", adInteger
    rs.Fields.Append "FIXED_ANSI_KR", adChar, 10, adFldIsNullable
    rs.Fields.Append "VAR_ANSI_KR", adVarChar, 120, adFldIsNullable
    rs.Fields.Append "LONG_ANSI_KR", adLongVarChar, 4000, adFldIsNullable
    rs.Open

    AddKoreanAnsiTextRow rs, 1, 0
    AddKoreanAnsiTextRow rs, 2, 0
    rs.AddNew
    rs.Fields("ID").Value = 3
    rs.Fields("FIXED_ANSI_KR").Value = Null
    rs.Fields("VAR_ANSI_KR").Value = Null
    rs.Fields("LONG_ANSI_KR").Value = Null
    rs.Update
    rs.UpdateBatch adAffectAll

    rs.MoveFirst
    SetKoreanAnsiTextValues rs, 1, 1
    rs.Update

    rs.MoveNext
    rs.Delete

    AddKoreanAnsiTextRow rs, 4, 2

    caseName = "text_korean_ansi"
    xmlPath = fso.BuildPath(root, caseName & ".xml")
    adtgPath = fso.BuildPath(root, caseName & ".adtg")
    rtPath = fso.BuildPath(root, caseName & ".roundtrip.xml")
    DeleteIfExists xmlPath
    DeleteIfExists adtgPath
    DeleteIfExists rtPath

    SaveBoth rs, xmlPath, adtgPath
    rs.Close
    RoundtripAdtgToXml adtgPath, rtPath
    WriteLine fso.BuildPath(root, "manifest.csv"), Csv(Array(caseName, "text_korean_ansi", 4, 3, xmlPath, adtgPath, rtPath))
End Sub

Sub DeleteKoreanAnsiTextArtifacts()
    DeleteIfExists fso.BuildPath(root, "text_korean_ansi.xml")
    DeleteIfExists fso.BuildPath(root, "text_korean_ansi.adtg")
    DeleteIfExists fso.BuildPath(root, "text_korean_ansi.roundtrip.xml")
End Sub

Sub AddKoreanAnsiTextRow(rs, rowNo, phase)
    rs.AddNew
    rs.Fields("ID").Value = rowNo
    SetKoreanAnsiTextValues rs, rowNo, phase
    rs.Update
End Sub

Sub SetKoreanAnsiTextValues(rs, rowNo, phase)
    rs.Fields("FIXED_ANSI_KR").Value = KoreanAnsiFixedText(rowNo, phase)
    rs.Fields("VAR_ANSI_KR").Value = KoreanAnsiVarText(rowNo, phase)
    rs.Fields("LONG_ANSI_KR").Value = KoreanAnsiLongText(rowNo, phase)
End Sub

Sub MakeTextSpacesCase()
    Dim rs, xmlPath, adtgPath, rtPath, caseName

    Set rs = NewRecordset()
    rs.Fields.Append "ID", adInteger
    rs.Fields.Append "FIXED_ASCII", adChar, 16, adFldIsNullable
    rs.Fields.Append "FIXED_WIDE", adWChar, 16, adFldIsNullable
    rs.Fields.Append "VAR_ASCII", adVarChar, 80, adFldIsNullable
    rs.Fields.Append "VAR_WIDE", adVarWChar, 80, adFldIsNullable
    rs.Fields.Append "LONG_WIDE", adLongVarWChar, 4000, adFldIsNullable
    rs.Open

    AddTextSpacesRow rs, 1, 0
    AddTextSpacesRow rs, 2, 0
    rs.AddNew
    rs.Fields("ID").Value = 3
    rs.Fields("FIXED_ASCII").Value = Null
    rs.Fields("FIXED_WIDE").Value = Null
    rs.Fields("VAR_ASCII").Value = Null
    rs.Fields("VAR_WIDE").Value = Null
    rs.Fields("LONG_WIDE").Value = Null
    rs.Update
    rs.UpdateBatch adAffectAll

    rs.MoveFirst
    SetTextSpacesValues rs, 1, 1
    rs.Update

    rs.MoveNext
    rs.Delete

    AddTextSpacesRow rs, 4, 2

    caseName = "text_spaces"
    xmlPath = fso.BuildPath(root, caseName & ".xml")
    adtgPath = fso.BuildPath(root, caseName & ".adtg")
    rtPath = fso.BuildPath(root, caseName & ".roundtrip.xml")
    DeleteIfExists xmlPath
    DeleteIfExists adtgPath
    DeleteIfExists rtPath

    SaveBoth rs, xmlPath, adtgPath
    rs.Close
    RoundtripAdtgToXml adtgPath, rtPath
    WriteLine fso.BuildPath(root, "manifest.csv"), Csv(Array(caseName, "text_leading_repeated_trailing_spaces", 6, 3, xmlPath, adtgPath, rtPath))
End Sub

Sub MakeTextEmptyStringsCase()
    Dim rs, xmlPath, adtgPath, rtPath, caseName

    Set rs = NewRecordset()
    rs.Fields.Append "ID", adInteger
    rs.Fields.Append "FIXED_ASCII_EMPTY", adChar, 4, adFldIsNullable
    rs.Fields.Append "FIXED_WIDE_EMPTY", adWChar, 4, adFldIsNullable
    rs.Fields.Append "VAR_ASCII_EMPTY", adVarChar, 16, adFldIsNullable
    rs.Fields.Append "VAR_WIDE_EMPTY", adVarWChar, 16, adFldIsNullable
    rs.Fields.Append "LONG_ASCII_EMPTY", adLongVarChar, 4000, adFldIsNullable
    rs.Fields.Append "LONG_WIDE_EMPTY", adLongVarWChar, 4000, adFldIsNullable
    rs.Open

    AddTextEmptyStringsNonEmptyRow rs, 1
    AddTextEmptyStringsEmptyRow rs, 2
    AddTextEmptyStringsNullRow rs, 3
    rs.UpdateBatch adAffectAll

    rs.MoveFirst
    SetTextEmptyStringsEmptyValues rs, 1
    rs.Update

    rs.MoveNext
    rs.Delete

    AddTextEmptyStringsEmptyRow rs, 4

    caseName = "text_empty_strings"
    xmlPath = fso.BuildPath(root, caseName & ".xml")
    adtgPath = fso.BuildPath(root, caseName & ".adtg")
    rtPath = fso.BuildPath(root, caseName & ".roundtrip.xml")
    DeleteIfExists xmlPath
    DeleteIfExists adtgPath
    DeleteIfExists rtPath

    SaveBoth rs, xmlPath, adtgPath
    rs.Close
    RoundtripAdtgToXml adtgPath, rtPath
    WriteLine fso.BuildPath(root, "manifest.csv"), Csv(Array(caseName, "text_empty_strings", 7, 3, xmlPath, adtgPath, rtPath))
End Sub

Sub MakeSupplementaryUnicodeCase()
    Dim rs, xmlPath, adtgPath, rtPath, caseName

    Set rs = NewRecordset()
    rs.Fields.Append "ID", adInteger
    rs.Fields.Append "SUPP_FIXED", adWChar, 16, adFldIsNullable
    rs.Fields.Append "SUPP_VAR", adVarWChar, 80, adFldIsNullable
    rs.Fields.Append "SUPP_LONG", adLongVarWChar, 4000, adFldIsNullable
    rs.Open

    AddSupplementaryUnicodeRow rs, 1, 0
    AddSupplementaryUnicodeRow rs, 2, 0
    rs.AddNew
    rs.Fields("ID").Value = 3
    rs.Fields("SUPP_FIXED").Value = Null
    rs.Fields("SUPP_VAR").Value = Null
    rs.Fields("SUPP_LONG").Value = Null
    rs.Update
    rs.UpdateBatch adAffectAll

    rs.MoveFirst
    SetSupplementaryUnicodeValues rs, 1, 1
    rs.Update

    rs.MoveNext
    rs.Delete

    AddSupplementaryUnicodeRow rs, 4, 2

    caseName = "unicode_supplementary"
    xmlPath = fso.BuildPath(root, caseName & ".xml")
    adtgPath = fso.BuildPath(root, caseName & ".adtg")
    rtPath = fso.BuildPath(root, caseName & ".roundtrip.xml")
    DeleteIfExists xmlPath
    DeleteIfExists adtgPath
    DeleteIfExists rtPath

    SaveBoth rs, xmlPath, adtgPath
    rs.Close
    RoundtripAdtgToXml adtgPath, rtPath
    WriteLine fso.BuildPath(root, "manifest.csv"), Csv(Array(caseName, "unicode_supplementary_plane", 4, 3, xmlPath, adtgPath, rtPath))
End Sub

Sub MakeUtf16XmlStreamCase()
    Dim rs, xmlPath, adtgPath, rtPath, caseName

    Set rs = NewRecordset()
    rs.Fields.Append "ID", adInteger
    rs.Fields.Append "TXT", adVarWChar, 80, adFldIsNullable
    rs.Fields.Append "LONG_TXT", adLongVarWChar, 4000, adFldIsNullable
    rs.Open

    rs.AddNew
    rs.Fields("ID").Value = 1
    rs.Fields("TXT").Value = ChrW(&HD55C) & ChrW(&HAE00) & " stream"
    rs.Fields("LONG_TXT").Value = RepeatText(ChrW(&HD55C) & ChrW(&HAE00) & "|utf16|", 20)
    rs.Update

    rs.AddNew
    rs.Fields("ID").Value = 2
    rs.Fields("TXT").Value = "delete me"
    rs.Fields("LONG_TXT").Value = "deleted"
    rs.Update
    rs.UpdateBatch adAffectAll

    rs.MoveFirst
    rs.Fields("TXT").Value = ChrW(&H20AC) & " updated"
    rs.Fields("LONG_TXT").Value = RepeatText("updated|" & ChrW(&HD55C) & ChrW(&HAE00) & "|", 18)
    rs.Update

    rs.MoveNext
    rs.Delete

    rs.AddNew
    rs.Fields("ID").Value = 3
    rs.Fields("TXT").Value = "inserted"
    rs.Fields("LONG_TXT").Value = ChrW(&HD55C) & ChrW(&HAE00) & " inserted"
    rs.Update

    caseName = "utf16_xml_stream"
    xmlPath = fso.BuildPath(root, caseName & ".xml")
    adtgPath = fso.BuildPath(root, caseName & ".adtg")
    rtPath = fso.BuildPath(root, caseName & ".roundtrip.xml")
    DeleteIfExists xmlPath
    DeleteIfExists adtgPath
    DeleteIfExists rtPath

    SaveXmlUtf16AndAdtg rs, xmlPath, adtgPath
    rs.Close
    RoundtripAdtgToXml adtgPath, rtPath
    WriteLine fso.BuildPath(root, "manifest.csv"), Csv(Array(caseName, "utf16_xml_stream", 3, 3, xmlPath, adtgPath, rtPath))
End Sub

Sub MakeStreamEncodingMatrix()
    ProbeStreamEncoding "unicode_text_stream", "unicode"
    ProbeStreamEncoding "unicodefffe_text_stream", "unicodeFFFE"
    ProbeStreamEncoding "utf16_text_stream", "utf-16"
    ProbeStreamEncoding "utf16be_text_stream", "utf-16BE"
    ProbeStreamEncoding "utf8_text_stream", "utf-8"
End Sub

Sub MakeFloatSpecialMatrix()
    Dim shell, scriptPath, psPath, command, exitCode, csvPath
    scriptPath = fso.BuildPath(fso.GetParentFolderName(WScript.ScriptFullName), "probe_float_specials.ps1")
    psPath = fso.BuildPath(fso.GetSpecialFolder(0), "SysWOW64\WindowsPowerShell\v1.0\powershell.exe")
    csvPath = fso.BuildPath(root, "float_special_matrix.csv")
    If Not fso.FileExists(scriptPath) Then Err.Raise 53, "probe_float_specials.ps1", "float special probe script not found"
    If Not fso.FileExists(psPath) Then Err.Raise 53, "powershell.exe", "32-bit PowerShell not found"

    Set shell = CreateObject("WScript.Shell")
    command = """" & psPath & """ -NoProfile -ExecutionPolicy Bypass -File """ & scriptPath & """ -Root """ & root & """ -CsvPath """ & csvPath & """"
    exitCode = shell.Run(command, 0, True)
    If exitCode <> 0 Then Err.Raise exitCode, "probe_float_specials.ps1", "float special matrix probe failed"
End Sub

Sub MakeFilterSaveMatrix()
    Dim shell, scriptPath, psPath, command, exitCode, csvPath, manifestPath
    scriptPath = fso.BuildPath(fso.GetParentFolderName(WScript.ScriptFullName), "probe_filter_save.ps1")
    psPath = fso.BuildPath(fso.GetSpecialFolder(0), "SysWOW64\WindowsPowerShell\v1.0\powershell.exe")
    csvPath = fso.BuildPath(root, "filter_save_matrix.csv")
    manifestPath = fso.BuildPath(root, "manifest.csv")
    If Not fso.FileExists(scriptPath) Then Err.Raise 53, "probe_filter_save.ps1", "filter save probe script not found"
    If Not fso.FileExists(psPath) Then Err.Raise 53, "powershell.exe", "32-bit PowerShell not found"

    Set shell = CreateObject("WScript.Shell")
    command = """" & psPath & """ -NoProfile -ExecutionPolicy Bypass -File """ & scriptPath & """ -Root """ & root & """ -CsvPath """ & csvPath & """ -ManifestPath """ & manifestPath & """"
    exitCode = shell.Run(command, 0, True)
    If exitCode <> 0 Then Err.Raise exitCode, "probe_filter_save.ps1", "filter save matrix probe failed"
End Sub

Sub MakeUtf16BeXmlStreamCase()
    Dim shell, scriptPath, psPath, command, exitCode
    scriptPath = fso.BuildPath(fso.GetParentFolderName(WScript.ScriptFullName), "make_utf16be_xml.ps1")
    psPath = fso.BuildPath(fso.GetSpecialFolder(0), "SysWOW64\WindowsPowerShell\v1.0\powershell.exe")
    If Not fso.FileExists(scriptPath) Then Err.Raise 53, "make_utf16be_xml.ps1", "UTF-16BE XML supplement script not found"
    If Not fso.FileExists(psPath) Then Err.Raise 53, "powershell.exe", "32-bit PowerShell not found"

    Set shell = CreateObject("WScript.Shell")
    command = """" & psPath & """ -NoProfile -ExecutionPolicy Bypass -File """ & scriptPath & """ """ & root & """"
    exitCode = shell.Run(command, 0, True)
    If exitCode <> 0 Then Err.Raise exitCode, "make_utf16be_xml.ps1", "UTF-16BE XML supplement failed"
End Sub

Sub ProbeStreamEncoding(caseName, charset)
    Dim rs, stream, path, check, saveErr, saveDesc, fileErr, fileDesc, openErr, openDesc

    path = fso.BuildPath(root, "_" & caseName & ".xml")
    DeleteIfExists path

    Set rs = NewRecordset()
    rs.Fields.Append "ID", adInteger, , adFldIsNullable
    rs.Fields.Append "TXT", adVarWChar, 120, adFldIsNullable
    rs.Open
    rs.AddNew
    rs.Fields("ID").Value = 1
    rs.Fields("TXT").Value = ChrW(&HD55C) & ChrW(&HAE00) & " " & ChrW(&H20AC)
    rs.Update

    Set stream = CreateObject("ADODB.Stream")
    stream.Type = adTypeText
    stream.Charset = charset
    stream.Open

    On Error Resume Next
    rs.Save stream, adPersistXML
    saveErr = Err.Number
    saveDesc = Err.Description
    Err.Clear
    If saveErr <> 0 Then
        WriteStreamEncodingMatrix caseName, charset, "fail", "save", saveErr, saveDesc
        stream.Close
        rs.Close
        DeleteIfExists path
        On Error GoTo 0
        Exit Sub
    End If

    stream.SaveToFile path, adSaveCreateOverWrite
    fileErr = Err.Number
    fileDesc = Err.Description
    Err.Clear
    If fileErr <> 0 Then
        WriteStreamEncodingMatrix caseName, charset, "fail", "save_to_file", fileErr, fileDesc
        stream.Close
        rs.Close
        DeleteIfExists path
        On Error GoTo 0
        Exit Sub
    End If

    Set check = CreateObject("ADODB.Recordset")
    check.CursorLocation = adUseClient
    check.Open path, "Provider=MSPersist", adOpenStatic, adLockBatchOptimistic, adCmdFile
    openErr = Err.Number
    openDesc = Err.Description
    Err.Clear
    If openErr <> 0 Then
        WriteStreamEncodingMatrix caseName, charset, "fail", "reopen", openErr, openDesc
    Else
        check.Close
        WriteStreamEncodingMatrix caseName, charset, "ok", "reopen", "", ""
    End If
    On Error GoTo 0

    stream.Close
    rs.Close
    DeleteIfExists path
End Sub

Sub AddNameMappingRow(rs, rowId, textValue, intValue, dashValue, koreanValue)
    rs.AddNew
    SetNameMappingValues rs, rowId, textValue, intValue, dashValue, koreanValue
    rs.Update
End Sub

Sub SetNameMappingValues(rs, rowId, textValue, intValue, dashValue, koreanValue)
    rs.Fields("ID").Value = rowId
    rs.Fields("Field Space Text").Value = textValue
    rs.Fields("1LeadingInteger").Value = intValue
    rs.Fields("Name-With-Dash").Value = dashValue
    rs.Fields(KoreanFieldName()).Value = koreanValue
End Sub

Sub AddSpecialFieldNamesRow(rs, rowId, phase)
    rs.AddNew
    SetSpecialFieldNamesValues rs, rowId, phase
    rs.Update
End Sub

Sub SetSpecialFieldNamesValues(rs, rowId, phase)
    rs.Fields("ID").Value = rowId
    rs.Fields(AmpFieldName()).Value = SpecialFieldValue("amp", rowId, phase)
    rs.Fields(QuoteFieldName()).Value = SpecialFieldValue("quote", rowId, phase)
    rs.Fields(ApostropheFieldName()).Value = SpecialFieldValue("apostrophe", rowId, phase)
    rs.Fields(LessFieldName()).Value = SpecialFieldValue("less", rowId, phase)
    rs.Fields(GreaterFieldName()).Value = SpecialFieldValue("greater", rowId, phase)
End Sub

Sub AddWhitespaceFieldNamesRow(rs, rowId, phase)
    rs.AddNew
    SetWhitespaceFieldNamesValues rs, rowId, phase
    rs.Update
End Sub

Sub SetWhitespaceFieldNamesValues(rs, rowId, phase)
    rs.Fields(0).Value = rowId
    rs.Fields(1).Value = WhitespaceFieldValue("space", rowId, phase)
    rs.Fields(2).Value = WhitespaceFieldValue("edge", rowId, phase)
    rs.Fields(3).Value = WhitespaceFieldValue("tab", rowId, phase)
    rs.Fields(4).Value = WhitespaceFieldValue("lf", rowId, phase)
    rs.Fields(5).Value = WhitespaceFieldValue("cr", rowId, phase)
End Sub

Sub MakeRandomCase(caseNo)
    Dim mode, rowCount, fieldCount, rs, names(), defs(), i, rowNo
    Dim xmlPath, adtgPath, rtPath, caseName

    mode = caseNo Mod 3
    rowCount = 1 + RandInt(6)
    fieldCount = 2 + RandInt(7)
    ReDim names(fieldCount - 1)
    ReDim defs(fieldCount - 1)

    Set rs = NewRecordset()
    rs.Fields.Append "ID", adInteger
    names(0) = "ID"
    defs(0) = Array("Integer", adInteger, 0)

    For i = 1 To fieldCount - 1
        defs(i) = RandomPersistableType()
        names(i) = RandomFieldName(caseNo, i, defs(i)(0))
        AppendTypedField rs, names(i), defs(i)
    Next

    rs.Open

    For rowNo = 1 To rowCount
        rs.AddNew
        FillRow rs, names, defs, rowNo, 0, False
        rs.Update
    Next

    If mode = 0 Or mode = 2 Then
        rs.UpdateBatch adAffectAll
    End If

    If mode = 2 And rowCount > 0 Then
        rs.MoveFirst
        FillRow rs, names, defs, 1, 1, True
        rs.Update

        If rowCount > 1 Then
            rs.MoveNext
            rs.Delete
        End If

        rs.AddNew
        FillRow rs, names, defs, rowCount + 1, 2, False
        rs.Update
    End If

    caseName = "random_" & Pad4(caseNo)
    xmlPath = fso.BuildPath(root, caseName & ".xml")
    adtgPath = fso.BuildPath(root, caseName & ".adtg")
    rtPath = fso.BuildPath(root, caseName & ".roundtrip.xml")
    DeleteIfExists xmlPath
    DeleteIfExists adtgPath
    DeleteIfExists rtPath

    SaveBoth rs, xmlPath, adtgPath
    rs.Close
    RoundtripAdtgToXml adtgPath, rtPath
    WriteLine fso.BuildPath(root, "manifest.csv"), Csv(Array(caseName, ModeName(mode), fieldCount, rowCount, xmlPath, adtgPath, rtPath))
End Sub

Sub FillRow(rs, names, defs, rowNo, phase, updateOnly)
    Dim i
    rs.Fields("ID").Value = rowNo
    For i = 1 To UBound(names)
        If updateOnly And i Mod 2 = 0 Then
            ' Let ADO emit sparse update rows with omitted unchanged columns.
        ElseIf ShouldWriteNull(rowNo, i, phase, defs(i)(0)) Then
            rs.Fields(names(i)).Value = Null
        Else
            SetFieldValue rs, names(i), defs(i)(0), rowNo, phase
        End If
    Next
End Sub

Sub SetFieldValue(rs, fieldName, kind, rowNo, phase)
    Select Case kind
        Case "TinyInt", "UnsignedTinyInt"
            rs.Fields(fieldName).Value = CByte((rowNo + phase) Mod 127)
        Case "SmallInt", "UnsignedSmallInt", "Integer", "UnsignedInt", "BigInt", "UnsignedBigInt"
            rs.Fields(fieldName).Value = CLng((rowNo * 100) + phase)
        Case "Single", "Double"
            rs.Fields(fieldName).Value = CDbl(rowNo) + (CDbl(phase) / 10) + 0.125
        Case "Currency", "Numeric", "Decimal", "VarNumeric"
            rs.Fields(fieldName).Value = CCur((rowNo * 1000) + phase + 0.1234)
        Case "Boolean"
            rs.Fields(fieldName).Value = ((rowNo + phase) Mod 2 = 0)
        Case "Date", "DBDate", "DBTime", "DBTimeStamp", "FileTime"
            rs.Fields(fieldName).Value = DateSerial(2000 + (rowNo Mod 20), 1 + ((rowNo + phase) Mod 12), 1 + ((rowNo + phase) Mod 27)) + TimeSerial((rowNo + phase) Mod 24, (rowNo * 3) Mod 60, (phase * 7) Mod 60)
        Case "GUID"
            rs.Fields(fieldName).Value = "{00000000-0000-0000-0000-" & Right("000000000000" & CStr((rowNo * 1000) + phase), 12) & "}"
        Case "Char", "WChar"
            rs.Fields(fieldName).Value = FixedText(rowNo, phase, 12)
        Case "VarChar", "VarWChar", "BSTR"
            rs.Fields(fieldName).Value = ShortText(rowNo, phase)
        Case "LongVarChar", "LongVarWChar"
            rs.Fields(fieldName).Value = LongText(rowNo, phase)
        Case "Binary", "VarBinary"
            rs.Fields(fieldName).Value = Bytes(Array(rowNo Mod 256, phase Mod 256, 222, 173))
        Case "LongVarBinary"
            rs.Fields(fieldName).Value = Bytes(Array(222, 173, 190, 239, rowNo Mod 256, phase Mod 256, 0, 1, 2, 3))
        Case "Error"
            rs.Fields(fieldName).Value = CLng(1000 + rowNo + phase)
        Case Else
            rs.Fields(fieldName).Value = ShortText(rowNo, phase)
    End Select
End Sub

Function ShouldWriteNull(rowNo, fieldNo, phase, kind)
    If kind = "ID" Then
        ShouldWriteNull = False
        Exit Function
    End If
    ShouldWriteNull = (((rowNo + fieldNo + phase) Mod 11) = 0)
End Function

Function ByteRange(firstValue, lastValue)
    Dim values(), i, value
    ReDim values(lastValue - firstValue)
    i = 0
    For value = firstValue To lastValue
        values(i) = value
        i = i + 1
    Next
    ByteRange = values
End Function

Function ByteRangeDescending(firstValue, lastValue)
    Dim values(), i, value
    ReDim values(firstValue - lastValue)
    i = 0
    For value = firstValue To lastValue Step -1
        values(i) = value
        i = i + 1
    Next
    ByteRangeDescending = values
End Function

Function ByteCycle(firstValue, count)
    Dim values(), i
    ReDim values(count - 1)
    For i = 0 To count - 1
        values(i) = (firstValue + i) Mod 256
    Next
    ByteCycle = values
End Function

Function NewRecordset()
    Dim rs
    Set rs = CreateObject("ADODB.Recordset")
    rs.CursorLocation = adUseClient
    Set NewRecordset = rs
End Function

Sub AppendTypedField(rs, fieldName, def)
    Dim kind, typeCode, size
    kind = def(0)
    typeCode = def(1)
    size = def(2)

    If size > 0 Then
        rs.Fields.Append fieldName, typeCode, size, adFldIsNullable
    Else
        rs.Fields.Append fieldName, typeCode, , adFldIsNullable
    End If

    If kind = "Numeric" Or kind = "Decimal" Or kind = "VarNumeric" Then
        On Error Resume Next
        rs.Fields(fieldName).Precision = 18
        rs.Fields(fieldName).NumericScale = 4
        Err.Clear
        On Error GoTo 0
    End If
End Sub

Sub SaveBoth(rs, xmlPath, adtgPath)
    Dim clone
    Set clone = rs.Clone
    rs.Save xmlPath, adPersistXML
    clone.Save adtgPath, adPersistADTG
    clone.Close
End Sub

Sub SaveXmlUtf16AndAdtg(rs, xmlPath, adtgPath)
    Dim clone, stream
    Set clone = rs.Clone
    Set stream = CreateObject("ADODB.Stream")
    stream.Type = adTypeText
    stream.Charset = "unicode"
    stream.Open
    rs.Save stream, adPersistXML
    stream.SaveToFile xmlPath, adSaveCreateOverWrite
    stream.Close
    clone.Save adtgPath, adPersistADTG
    clone.Close
End Sub

Sub RoundtripAdtgToXml(adtgPath, xmlPath)
    Dim rs
    Set rs = CreateObject("ADODB.Recordset")
    rs.CursorLocation = adUseClient
    rs.Open adtgPath, "Provider=MSPersist", adOpenStatic, adLockBatchOptimistic, adCmdFile
    rs.Save xmlPath, adPersistXML
    rs.Close
End Sub

Sub RoundtripXmlToAdtg(xmlPath, adtgPath)
    Dim rs
    Set rs = CreateObject("ADODB.Recordset")
    rs.CursorLocation = adUseClient
    rs.Open xmlPath, "Provider=MSPersist", adOpenStatic, adLockBatchOptimistic, adCmdFile
    rs.Save adtgPath, adPersistADTG
    rs.Close
End Sub

Function RandomPersistableType()
    RandomPersistableType = PersistableTypeAt(RandInt(PersistableTypeCount()))
End Function

Function PersistableTypeCount()
    PersistableTypeCount = 26
End Function

Function PersistableTypeAt(index)
    Select Case index
        Case 0: PersistableTypeAt = Array("TinyInt", adTinyInt, 0)
        Case 1: PersistableTypeAt = Array("UnsignedTinyInt", adUnsignedTinyInt, 0)
        Case 2: PersistableTypeAt = Array("SmallInt", adSmallInt, 0)
        Case 3: PersistableTypeAt = Array("UnsignedSmallInt", adUnsignedSmallInt, 0)
        Case 4: PersistableTypeAt = Array("Integer", adInteger, 0)
        Case 5: PersistableTypeAt = Array("UnsignedInt", adUnsignedInt, 0)
        Case 6: PersistableTypeAt = Array("BigInt", adBigInt, 0)
        Case 7: PersistableTypeAt = Array("UnsignedBigInt", adUnsignedBigInt, 0)
        Case 8: PersistableTypeAt = Array("Single", adSingle, 0)
        Case 9: PersistableTypeAt = Array("Double", adDouble, 0)
        Case 10: PersistableTypeAt = Array("Currency", adCurrency, 0)
        Case 11: PersistableTypeAt = Array("Boolean", adBoolean, 0)
        Case 12: PersistableTypeAt = Array("Date", adDate, 0)
        Case 13: PersistableTypeAt = Array("DBDate", adDBDate, 0)
        Case 14: PersistableTypeAt = Array("DBTime", adDBTime, 0)
        Case 15: PersistableTypeAt = Array("DBTimeStamp", adDBTimeStamp, 0)
        Case 16: PersistableTypeAt = Array("GUID", adGUID, 0)
        Case 17: PersistableTypeAt = Array("Char", adChar, 12)
        Case 18: PersistableTypeAt = Array("WChar", adWChar, 12)
        Case 19: PersistableTypeAt = Array("VarChar", adVarChar, 80)
        Case 20: PersistableTypeAt = Array("VarWChar", adVarWChar, 80)
        Case 21: PersistableTypeAt = Array("LongVarChar", adLongVarChar, 2000)
        Case 22: PersistableTypeAt = Array("LongVarWChar", adLongVarWChar, 2000)
        Case 23: PersistableTypeAt = Array("Binary", adBinary, 8)
        Case 24: PersistableTypeAt = Array("VarBinary", adVarBinary, 16)
        Case Else: PersistableTypeAt = Array("LongVarBinary", adLongVarBinary, 2000)
    End Select
End Function

Function TypeMatrixCount()
    TypeMatrixCount = 40
End Function

Function SupportedFlatTypeCount()
    SupportedFlatTypeCount = 30
End Function

Function SupportedFlatTypeAt(index)
    SupportedFlatTypeAt = TypeMatrixAt(index)
End Function

Function TypeMatrixAt(index)
    Select Case index
        Case 0: TypeMatrixAt = Array("TinyInt", adTinyInt, 0)
        Case 1: TypeMatrixAt = Array("UnsignedTinyInt", adUnsignedTinyInt, 0)
        Case 2: TypeMatrixAt = Array("SmallInt", adSmallInt, 0)
        Case 3: TypeMatrixAt = Array("UnsignedSmallInt", adUnsignedSmallInt, 0)
        Case 4: TypeMatrixAt = Array("Integer", adInteger, 0)
        Case 5: TypeMatrixAt = Array("UnsignedInt", adUnsignedInt, 0)
        Case 6: TypeMatrixAt = Array("BigInt", adBigInt, 0)
        Case 7: TypeMatrixAt = Array("UnsignedBigInt", adUnsignedBigInt, 0)
        Case 8: TypeMatrixAt = Array("Single", adSingle, 0)
        Case 9: TypeMatrixAt = Array("Double", adDouble, 0)
        Case 10: TypeMatrixAt = Array("Currency", adCurrency, 0)
        Case 11: TypeMatrixAt = Array("Boolean", adBoolean, 0)
        Case 12: TypeMatrixAt = Array("Date", adDate, 0)
        Case 13: TypeMatrixAt = Array("DBDate", adDBDate, 0)
        Case 14: TypeMatrixAt = Array("DBTime", adDBTime, 0)
        Case 15: TypeMatrixAt = Array("DBTimeStamp", adDBTimeStamp, 0)
        Case 16: TypeMatrixAt = Array("FileTime", adFileTime, 0)
        Case 17: TypeMatrixAt = Array("GUID", adGUID, 0)
        Case 18: TypeMatrixAt = Array("BSTR", adBSTR, 120)
        Case 19: TypeMatrixAt = Array("Char", adChar, 12)
        Case 20: TypeMatrixAt = Array("WChar", adWChar, 12)
        Case 21: TypeMatrixAt = Array("VarChar", adVarChar, 120)
        Case 22: TypeMatrixAt = Array("VarWChar", adVarWChar, 120)
        Case 23: TypeMatrixAt = Array("LongVarChar", adLongVarChar, 2000)
        Case 24: TypeMatrixAt = Array("LongVarWChar", adLongVarWChar, 2000)
        Case 25: TypeMatrixAt = Array("Binary", adBinary, 8)
        Case 26: TypeMatrixAt = Array("VarBinary", adVarBinary, 16)
        Case 27: TypeMatrixAt = Array("LongVarBinary", adLongVarBinary, 2000)
        Case 28: TypeMatrixAt = Array("Numeric", adNumeric, 0)
        Case 29: TypeMatrixAt = Array("Decimal", adDecimal, 0)
        Case 30: TypeMatrixAt = Array("Empty", adEmpty, 0)
        Case 31: TypeMatrixAt = Array("VarNumeric", adVarNumeric, 0)
        Case 32: TypeMatrixAt = Array("Error", adError, 0)
        Case 33: TypeMatrixAt = Array("Variant", adVariant, 0)
        Case 34: TypeMatrixAt = Array("IDispatch", adIDispatch, 0)
        Case 35: TypeMatrixAt = Array("IUnknown", adIUnknown, 0)
        Case 36: TypeMatrixAt = Array("Chapter", adChapter, 0)
        Case 37: TypeMatrixAt = Array("PropVariant", adPropVariant, 0)
        Case 38: TypeMatrixAt = Array("UserDefined", adUserDefined, 0)
        Case Else: TypeMatrixAt = Array("ArrayInteger", adArray + adInteger, 0)
    End Select
End Function

Function ShortText(rowNo, phase)
    ShortText = "r" & CStr(rowNo) & "_p" & CStr(phase) & "_" & ChrW(&HD55C) & ChrW(&HAE00) & "_<&'>"
End Function

Function FixedText(rowNo, phase, size)
    Dim value
    value = "r" & CStr(rowNo) & "p" & CStr(phase)
    If Len(value) < size Then value = value & Space(size - Len(value))
    FixedText = Left(value, size)
End Function

Function LongText(rowNo, phase)
    Dim out, i
    out = ""
    For i = 1 To 40
        out = out & ShortText(rowNo, phase) & "_" & CStr(i) & " "
    Next
    LongText = out
End Function

Function RepeatText(value, count)
    Dim out, i
    out = ""
    For i = 1 To count
        out = out & value
    Next
    RepeatText = out
End Function

Function KoreanFieldName()
    KoreanFieldName = ChrW(&HD55C) & ChrW(&HAE00) & " " & ChrW(&HD544) & ChrW(&HB4DC)
End Function

Function KoreanValue(index)
    KoreanValue = ChrW(&HD55C) & ChrW(&HAE00) & "_" & CStr(index) & "_" & ChrW(&HAC12)
End Function

Function KoreanAnsiFixedText(rowNo, phase)
    If rowNo = 1 And phase = 1 Then
        KoreanAnsiFixedText = ChrW(&HCE74) & ChrW(&HD0C0) & ChrW(&HD30C) & ChrW(&HD558) & ChrW(&HAC12)
    ElseIf rowNo = 2 And phase = 0 Then
        KoreanAnsiFixedText = ChrW(&HBC14) & ChrW(&HC0AC) & ChrW(&HC544) & ChrW(&HC790) & ChrW(&HCC28)
    ElseIf rowNo = 4 And phase = 2 Then
        KoreanAnsiFixedText = ChrW(&HD55C) & ChrW(&HAE00) & ChrW(&HC790) & ChrW(&HB8CC) & ChrW(&HB05D)
    Else
        KoreanAnsiFixedText = ChrW(&HAC00) & ChrW(&HB098) & ChrW(&HB2E4) & ChrW(&HB77C) & ChrW(&HB9C8)
    End If
End Function

Function KoreanAnsiVarText(rowNo, phase)
    KoreanAnsiVarText = ChrW(&HD55C) & ChrW(&HAE00) & "_" & CStr(rowNo) & "_p" & CStr(phase) & "_" & ChrW(&HAC12) & ChrW(&HCE74)
End Function

Function KoreanAnsiLongText(rowNo, phase)
    Dim out, i
    out = ""
    For i = 1 To 24
        out = out & KoreanAnsiVarText(rowNo, phase) & "|" & CStr(i) & ";"
    Next
    KoreanAnsiLongText = out
End Function

Function AmpFieldName()
    AmpFieldName = "Amp & Field"
End Function

Function QuoteFieldName()
    QuoteFieldName = "Quote "" Field"
End Function

Function ApostropheFieldName()
    ApostropheFieldName = "Apostrophe ' Field"
End Function

Function LessFieldName()
    LessFieldName = "Less < Field"
End Function

Function GreaterFieldName()
    GreaterFieldName = "Greater > Field"
End Function

Function SpecialFieldValue(kind, rowId, phase)
    SpecialFieldValue = kind & "|row=" & CStr(rowId) & "|phase=" & CStr(phase) & "|" & ChrW(&HD55C) & ChrW(&HAE00)
End Function

Function SpaceOnlyFieldName()
    SpaceOnlyFieldName = " "
End Function

Function EdgeSpaceFieldName()
    EdgeSpaceFieldName = "  Edge Name  "
End Function

Function TabFieldName()
    TabFieldName = "Tab" & ChrW(9) & "Field"
End Function

Function LfFieldName()
    LfFieldName = "Lf" & ChrW(10) & "Field"
End Function

Function CrFieldName()
    CrFieldName = "Cr" & ChrW(13) & "Field"
End Function

Function WhitespaceFieldValue(kind, rowId, phase)
    WhitespaceFieldValue = kind & "|row=" & CStr(rowId) & "|phase=" & CStr(phase)
End Function

Sub AddTextEscapesRow(rs, rowId, phase)
    rs.AddNew
    SetTextEscapesValues rs, rowId, phase
    rs.Update
End Sub

Sub SetTextEscapesValues(rs, rowId, phase)
    rs.Fields("ID").Value = rowId
    rs.Fields("TXT_VAR").Value = TextEscapeAscii(rowId, phase)
    rs.Fields("TXT_WIDE").Value = TextEscapeWide(rowId, phase)
    rs.Fields("TXT_LONG").Value = TextEscapeLong(rowId, phase)
End Sub

Function TextEscapeAscii(rowId, phase)
    TextEscapeAscii = "row=" & CStr(rowId) & "|phase=" & CStr(phase) & "|""dq""|'sq'|<&>|tab" & ChrW(9) & "cr" & ChrW(13) & "lf" & ChrW(10) & "end"
End Function

Function TextEscapeWide(rowId, phase)
    TextEscapeWide = TextEscapeAscii(rowId, phase) & "|wide=" & ChrW(&HD55C) & ChrW(&HAE00) & "_" & ChrW(&HAC12) & "_" & ChrW(&H20AC)
End Function

Function TextEscapeLong(rowId, phase)
    Dim out, i
    out = ""
    For i = 1 To 18
        out = out & TextEscapeWide(rowId, phase) & "|part=" & CStr(i) & ChrW(13) & ChrW(10)
    Next
    TextEscapeLong = out
End Function

Sub AddTextControlCharsRow(rs, rowId, phase)
    rs.AddNew
    SetTextControlCharsValues rs, rowId, phase
    rs.Update
End Sub

Sub SetTextControlCharsValues(rs, rowId, phase)
    Dim asciiValue, wideValue
    rs.Fields("ID").Value = rowId
    asciiValue = TextControlCharsValue("A", rowId, phase)
    wideValue = TextControlCharsValue("W", rowId, phase)
    rs.Fields("FIXED_ANSI_CTL").Value = asciiValue
    rs.Fields("FIXED_WIDE_CTL").Value = wideValue
    rs.Fields("VAR_ANSI_CTL").Value = asciiValue
    rs.Fields("VAR_WIDE_CTL").Value = wideValue
    rs.Fields("LONG_ANSI_CTL").Value = TextControlLongValue(asciiValue)
    rs.Fields("LONG_WIDE_CTL").Value = TextControlLongValue(wideValue)
End Sub

Function TextControlCharsValue(prefix, rowId, phase)
    TextControlCharsValue = prefix & CStr(rowId) & "p" & CStr(phase) & _
        ChrW(0) & ChrW(1) & ChrW(8) & ChrW(11) & ChrW(12) & ChrW(14) & ChrW(31) & "Z"
End Function

Function TextControlLongValue(value)
    TextControlLongValue = value & "|tail|" & value
End Function

Sub AddTextSpacesRow(rs, rowId, phase)
    rs.AddNew
    SetTextSpacesValues rs, rowId, phase
    rs.Update
End Sub

Sub SetTextSpacesValues(rs, rowId, phase)
    rs.Fields("ID").Value = rowId
    rs.Fields("FIXED_ASCII").Value = FixedSpaceAscii(rowId, phase)
    rs.Fields("FIXED_WIDE").Value = FixedSpaceWide(rowId, phase)
    rs.Fields("VAR_ASCII").Value = VariableSpaceAscii(rowId, phase)
    rs.Fields("VAR_WIDE").Value = VariableSpaceWide(rowId, phase)
    rs.Fields("LONG_WIDE").Value = LongSpaceWide(rowId, phase)
End Sub

Function FixedSpaceAscii(rowId, phase)
    FixedSpaceAscii = Left(" A" & CStr(rowId) & "  P" & CStr(phase) & "   Z        ", 16)
End Function

Function FixedSpaceWide(rowId, phase)
    FixedSpaceWide = Left(" " & ChrW(&HD55C) & CStr(rowId) & "  " & ChrW(&HAC12) & CStr(phase) & "   " & ChrW(&H20AC) & "        ", 16)
End Function

Function VariableSpaceAscii(rowId, phase)
    VariableSpaceAscii = "  row " & CStr(rowId) & "   phase " & CStr(phase) & "  end  "
End Function

Function VariableSpaceWide(rowId, phase)
    VariableSpaceWide = "  " & ChrW(&HD55C) & ChrW(&HAE00) & " " & CStr(rowId) & "   " & ChrW(&HAC12) & " " & CStr(phase) & "  " & ChrW(&H20AC) & "  "
End Function

Function LongSpaceWide(rowId, phase)
    Dim out, i
    out = ""
    For i = 1 To 20
        out = out & VariableSpaceWide(rowId, phase) & " block  " & CStr(i) & "   "
    Next
    LongSpaceWide = out
End Function

Sub AddTextEmptyStringsNonEmptyRow(rs, rowId)
    rs.AddNew
    rs.Fields("ID").Value = rowId
    rs.Fields("FIXED_ASCII_EMPTY").Value = "A"
    rs.Fields("FIXED_WIDE_EMPTY").Value = ChrW(&HD55C)
    rs.Fields("VAR_ASCII_EMPTY").Value = "abc"
    rs.Fields("VAR_WIDE_EMPTY").Value = ChrW(&HD55C) & ChrW(&HAE00)
    rs.Fields("LONG_ASCII_EMPTY").Value = "long"
    rs.Fields("LONG_WIDE_EMPTY").Value = ChrW(&HAE34) & ChrW(&HAE00)
    rs.Update
End Sub

Sub AddTextEmptyStringsEmptyRow(rs, rowId)
    rs.AddNew
    SetTextEmptyStringsEmptyValues rs, rowId
    rs.Update
End Sub

Sub SetTextEmptyStringsEmptyValues(rs, rowId)
    rs.Fields("ID").Value = rowId
    rs.Fields("FIXED_ASCII_EMPTY").Value = ""
    rs.Fields("FIXED_WIDE_EMPTY").Value = ""
    rs.Fields("VAR_ASCII_EMPTY").Value = ""
    rs.Fields("VAR_WIDE_EMPTY").Value = ""
    rs.Fields("LONG_ASCII_EMPTY").Value = ""
    rs.Fields("LONG_WIDE_EMPTY").Value = ""
End Sub

Sub AddTextEmptyStringsNullRow(rs, rowId)
    rs.AddNew
    rs.Fields("ID").Value = rowId
    rs.Fields("FIXED_ASCII_EMPTY").Value = Null
    rs.Fields("FIXED_WIDE_EMPTY").Value = Null
    rs.Fields("VAR_ASCII_EMPTY").Value = Null
    rs.Fields("VAR_WIDE_EMPTY").Value = Null
    rs.Fields("LONG_ASCII_EMPTY").Value = Null
    rs.Fields("LONG_WIDE_EMPTY").Value = Null
    rs.Update
End Sub

Sub AddSupplementaryUnicodeRow(rs, rowId, phase)
    rs.AddNew
    SetSupplementaryUnicodeValues rs, rowId, phase
    rs.Update
End Sub

Sub SetSupplementaryUnicodeValues(rs, rowId, phase)
    rs.Fields("ID").Value = rowId
    rs.Fields("SUPP_FIXED").Value = FixedSupplementaryText(rowId, phase)
    rs.Fields("SUPP_VAR").Value = SupplementaryText(rowId, phase)
    rs.Fields("SUPP_LONG").Value = LongSupplementaryText(rowId, phase)
End Sub

Function SupplementaryPairA()
    SupplementaryPairA = ChrW(&HD83D) & ChrW(&HDE00)
End Function

Function SupplementaryPairB()
    SupplementaryPairB = ChrW(&HD840) & ChrW(&HDC00)
End Function

Function SupplementaryText(rowId, phase)
    SupplementaryText = "row=" & CStr(rowId) & "|phase=" & CStr(phase) & "|" & SupplementaryPairA() & "|" & SupplementaryPairB() & "|" & ChrW(&HD55C) & ChrW(&HAE00)
End Function

Function FixedSupplementaryText(rowId, phase)
    FixedSupplementaryText = Left(" " & SupplementaryPairA() & " R" & CStr(rowId) & " P" & CStr(phase) & " " & SupplementaryPairB() & "        ", 16)
End Function

Function LongSupplementaryText(rowId, phase)
    Dim out, i
    out = ""
    For i = 1 To 24
        out = out & SupplementaryText(rowId, phase) & "|part=" & CStr(i) & " "
    Next
    LongSupplementaryText = out
End Function

Function RandomFieldName(caseNo, ordinal, kind)
    If (caseNo + ordinal) Mod 9 = 0 Then
        RandomFieldName = "Field Space " & CStr(ordinal) & " " & kind
    Else
        RandomFieldName = "F" & CStr(ordinal) & "_" & kind
    End If
End Function

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

Function EmptyBytes()
    Dim doc, node
    Set doc = CreateObject("MSXML2.DOMDocument.6.0")
    Set node = doc.createElement("bytes")
    node.dataType = "bin.hex"
    node.Text = ""
    EmptyBytes = node.nodeTypedValue
End Function

Function ArgNumber(index, defaultValue)
    If WScript.Arguments.Count > index Then
        ArgNumber = CLng(WScript.Arguments(index))
    Else
        ArgNumber = defaultValue
    End If
End Function

Function ArgText(index, defaultValue)
    If WScript.Arguments.Count > index Then
        ArgText = WScript.Arguments(index)
    Else
        ArgText = defaultValue
    End If
End Function

Function RandInt(maxExclusive)
    RandInt = Int(Rnd() * maxExclusive)
End Function

Function ModeName(mode)
    Select Case mode
        Case 0: ModeName = "current"
        Case 1: ModeName = "insert"
        Case Else: ModeName = "update_delete_insert"
    End Select
End Function

Function SafeName(text)
    SafeName = Replace(Replace(Replace(text, " ", "_"), ".", "_"), "-", "_")
End Function

Function Pad4(value)
    Pad4 = Right("0000" & CStr(value), 4)
End Function

Function Pad2(value)
    Pad2 = Right("00" & CStr(value), 2)
End Function

Function XmlAttr(value)
    Dim out
    out = Replace(CStr(value), "&", "&amp;")
    out = Replace(out, """", "&quot;")
    out = Replace(out, "<", "&lt;")
    out = Replace(out, ">", "&gt;")
    XmlAttr = out
End Function

Sub WriteTypeMatrix(typeName, typeCode, result, xmlPath, adtgPath, errorNumber, errorDescription)
    WriteLine fso.BuildPath(root, "type_matrix.csv"), Csv(Array(typeName, typeCode, result, xmlPath, adtgPath, errorNumber, errorDescription))
End Sub

Sub WriteFieldAttributeMatrix(attributeName, fieldTypeName, attributeFlags, result, errorNumber, errorDescription)
    WriteLine fso.BuildPath(root, "field_attribute_matrix.csv"), Csv(Array(attributeName, fieldTypeName, attributeFlags, result, errorNumber, errorDescription))
End Sub

Sub WriteSchemaShapeMatrix(caseName, stage, result, errorNumber, errorDescription)
    WriteLine fso.BuildPath(root, "schema_shape_matrix.csv"), Csv(Array(caseName, stage, result, errorNumber, errorDescription))
End Sub

Sub WriteXmlReaderMatrix(caseName, stage, result, errorNumber, errorDescription)
    WriteLine fso.BuildPath(root, "xml_reader_matrix.csv"), Csv(Array(caseName, stage, result, errorNumber, errorDescription))
End Sub

Sub WriteStreamEncodingMatrix(caseName, charset, result, stage, errorNumber, errorDescription)
    WriteLine fso.BuildPath(root, "stream_encoding_matrix.csv"), Csv(Array(caseName, charset, result, stage, errorNumber, errorDescription))
End Sub

Sub WriteFailure(caseName, stage, errorNumber, errorDescription)
    WriteLine fso.BuildPath(root, "failures.csv"), Csv(Array(caseName, stage, errorNumber, errorDescription))
End Sub

Function Csv(values)
    Dim out, i
    out = ""
    For i = 0 To UBound(values)
        If i > 0 Then out = out & ","
        out = out & """" & Replace(CStr(values(i)), """", """""") & """"
    Next
    Csv = out
End Function

Sub WriteLine(path, text)
    Dim stream
    Set stream = fso.OpenTextFile(path, 8, True)
    stream.WriteLine text
    stream.Close
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
