Option Explicit

Const adUseServer = 2
Const adUseClient = 3
Const adOpenForwardOnly = 0
Const adOpenKeyset = 1
Const adOpenDynamic = 2
Const adOpenStatic = 3
Const adLockReadOnly = 1
Const adLockOptimistic = 3
Const adLockBatchOptimistic = 4
Const adCmdText = 1
Const adCmdFile = 256
Const adPersistADTG = 0
Const adPersistXML = 1
Const adExecuteNoRecords = 128
Const adMarshalModifiedOnly = 1

Dim fso
Set fso = CreateObject("Scripting.FileSystemObject")

Dim server, userName, password, databaseName, root
server = ArgText(0, "SERVER")
userName = ArgText(1, "USER")
password = ArgText(2, "<password>")
databaseName = ArgText(3, "AdoRecordsetSales")
root = ArgText(4, fso.BuildPath(fso.GetParentFolderName(WScript.ScriptFullName), "..\corpus\sqlserver_sales"))

EnsureFolder root

Dim master, db
Set master = OpenConnection("master")
ExecSql master, "IF DB_ID(N'" & SqlString(databaseName) & "') IS NOT NULL BEGIN ALTER DATABASE " & SqlName(databaseName) & " SET SINGLE_USER WITH ROLLBACK IMMEDIATE; DROP DATABASE " & SqlName(databaseName) & "; END", "drop database"
ExecSql master, "CREATE DATABASE " & SqlName(databaseName), "create database"
master.Close

Set db = OpenConnection(databaseName)
ExecSql db, ReadText(fso.BuildPath(fso.GetParentFolderName(WScript.ScriptFullName), "sales_sqlserver_seed.sql")), "seed sales schema"
SaveSalesRecordset db
SaveLegacyLobRecordset db
SaveSqlServerPendingChangesRecordset db
SaveSqlServerMarshalModifiedOnlyRecordset db
SaveSqlServerSortedRecordset db
SaveSqlServerFilteredRecordset db
SaveSqlServerCursorOptimisticRecordset db
SaveSqlServerCursorKeysetReadOnlyRecordset db
SaveSqlServerCursorForwardOnlyReadOnlyRecordset db
SaveSqlServerCursorDynamicReadOnlyRecordset db
SaveSqlServerDuplicateAliasRecordset db
SaveSqlVariantSupportedRecordset db
SaveSqlVariantFailureMatrix db
db.Close

WScript.Echo "Generated SQL Server sales ADO corpus in " & fso.GetAbsolutePathName(root)

Function ArgText(index, defaultValue)
    If WScript.Arguments.Count > index Then
        ArgText = WScript.Arguments(index)
    Else
        ArgText = defaultValue
    End If
End Function

Function OpenConnection(catalog)
    Dim cn
    Set cn = CreateObject("ADODB.Connection")
    cn.ConnectionTimeout = 15
    cn.CommandTimeout = 120
    cn.Open "Provider=SQLOLEDB;Data Source=" & server & ";Initial Catalog=" & catalog & ";User ID=" & userName & ";Password=" & password & ";"
    Set OpenConnection = cn
End Function

Sub SaveSalesRecordset(cn)
    Dim rs, xmlPath, adtgPath, roundtripPath, query
    xmlPath = fso.BuildPath(root, "sales_mixed_join.xml")
    adtgPath = fso.BuildPath(root, "sales_mixed_join.adtg")
    roundtripPath = fso.BuildPath(root, "sales_mixed_join.roundtrip.xml")
    DeleteIfExists xmlPath
    DeleteIfExists adtgPath
    DeleteIfExists roundtripPath

    query = ReadText(fso.BuildPath(fso.GetParentFolderName(WScript.ScriptFullName), "sales_sqlserver_join.sql"))
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

    WriteText fso.BuildPath(root, "manifest.csv"), "case,mode,tables,rows,xml,adtg,roundtrip_xml" & vbCrLf & _
        Csv(Array("sales_mixed_join", "sqlserver_join_mixed_types", "9", "720", xmlPath, adtgPath, roundtripPath)) & vbCrLf
End Sub

Sub SaveSqlVariantSupportedRecordset(cn)
    Dim rs, xmlPath, adtgPath, roundtripPath, query
    xmlPath = fso.BuildPath(root, "sql_variant_supported.xml")
    adtgPath = fso.BuildPath(root, "sql_variant_supported.adtg")
    roundtripPath = fso.BuildPath(root, "sql_variant_supported.roundtrip.xml")
    DeleteIfExists xmlPath
    DeleteIfExists adtgPath
    DeleteIfExists roundtripPath

    query = SqlVariantSupportedQuery()
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

    AppendText fso.BuildPath(root, "manifest.csv"), _
        Csv(Array("sql_variant_supported", "sqlserver_sql_variant_supported_subtypes", "0", "2", xmlPath, adtgPath, roundtripPath)) & vbCrLf
End Sub

Sub SaveLegacyLobRecordset(cn)
    Dim rs, xmlPath, adtgPath, roundtripPath, query
    xmlPath = fso.BuildPath(root, "sales_legacy_lob_join.xml")
    adtgPath = fso.BuildPath(root, "sales_legacy_lob_join.adtg")
    roundtripPath = fso.BuildPath(root, "sales_legacy_lob_join.roundtrip.xml")
    DeleteIfExists xmlPath
    DeleteIfExists adtgPath
    DeleteIfExists roundtripPath

    query = LegacyLobJoinQuery()
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

    AppendText fso.BuildPath(root, "manifest.csv"), _
        Csv(Array("sales_legacy_lob_join", "sqlserver_legacy_lob_rowversion_join", "8", "12", xmlPath, adtgPath, roundtripPath)) & vbCrLf
End Sub

Sub SaveSqlServerPendingChangesRecordset(cn)
    Dim rs, xmlPath, adtgPath, roundtripPath
    xmlPath = fso.BuildPath(root, "sales_customers_pending.xml")
    adtgPath = fso.BuildPath(root, "sales_customers_pending.adtg")
    roundtripPath = fso.BuildPath(root, "sales_customers_pending.roundtrip.xml")
    DeleteIfExists xmlPath
    DeleteIfExists adtgPath
    DeleteIfExists roundtripPath

    Set rs = CreateObject("ADODB.Recordset")
    rs.CursorLocation = adUseClient
    rs.Open "SELECT TOP 6 CustomerId, RegionId, CustomerCode, CustomerName, CustomerNotes, CreditLimit, SignupDate, IsPreferred, CustomerGuid, ProfileHash FROM dbo.SalesCustomers ORDER BY CustomerId", cn, adOpenStatic, adLockBatchOptimistic, adCmdText

    rs.MoveFirst
    rs.Fields("CustomerNotes").Value = "updated provider note & <xml>"
    rs.Fields("CreditLimit").Value = CCur("12345.67")
    rs.Update

    rs.MoveNext
    rs.Delete

    rs.AddNew
    rs.Fields("CustomerId").Value = 900001
    rs.Fields("RegionId").Value = CByte(1)
    rs.Fields("CustomerCode").Value = "CUSTX001"
    rs.Fields("CustomerName").Value = "Inserted Customer"
    rs.Fields("CustomerNotes").Value = "inserted note"
    rs.Fields("CreditLimit").Value = CCur("555.55")
    rs.Fields("SignupDate").Value = DateSerial(2025, 1, 2) + TimeSerial(3, 4, 5)
    rs.Fields("IsPreferred").Value = True
    rs.Fields("CustomerGuid").Value = "{00090001-1111-2222-3333-000000900001}"
    rs.Fields("ProfileHash").Value = Bytes(Array(1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16))
    rs.Update

    rs.Save xmlPath, adPersistXML
    rs.Save adtgPath, adPersistADTG
    rs.Close

    Set rs = CreateObject("ADODB.Recordset")
    rs.CursorLocation = adUseClient
    rs.Open adtgPath, "Provider=MSPersist", adOpenStatic, adLockBatchOptimistic, adCmdFile
    rs.Save roundtripPath, adPersistXML
    rs.Close

    AppendText fso.BuildPath(root, "manifest.csv"), _
        Csv(Array("sales_customers_pending", "sqlserver_provider_pending_changes", "1", "6", xmlPath, adtgPath, roundtripPath)) & vbCrLf
End Sub

Sub SaveSqlServerMarshalModifiedOnlyRecordset(cn)
    Dim rs, xmlPath, adtgPath, roundtripPath
    xmlPath = fso.BuildPath(root, "sales_customers_marshal_modified.xml")
    adtgPath = fso.BuildPath(root, "sales_customers_marshal_modified.adtg")
    roundtripPath = fso.BuildPath(root, "sales_customers_marshal_modified.roundtrip.xml")
    DeleteIfExists xmlPath
    DeleteIfExists adtgPath
    DeleteIfExists roundtripPath

    Set rs = CreateObject("ADODB.Recordset")
    rs.CursorLocation = adUseClient
    rs.Open "SELECT TOP 6 CustomerId, RegionId, CustomerCode, CustomerName, CustomerNotes, CreditLimit, SignupDate, IsPreferred, CustomerGuid, ProfileHash FROM dbo.SalesCustomers ORDER BY CustomerId", cn, adOpenStatic, adLockBatchOptimistic, adCmdText

    rs.MoveFirst
    rs.Fields("CustomerNotes").Value = "marshal modified-only provider note & <xml>"
    rs.Fields("CreditLimit").Value = CCur("22222.22")
    rs.Update

    rs.MoveNext
    rs.Delete

    rs.AddNew
    rs.Fields("CustomerId").Value = 900002
    rs.Fields("RegionId").Value = CByte(1)
    rs.Fields("CustomerCode").Value = "MOPT0002"
    rs.Fields("CustomerName").Value = "Marshal Modified Inserted"
    rs.Fields("CustomerNotes").Value = "marshal inserted note"
    rs.Fields("CreditLimit").Value = CCur("777.77")
    rs.Fields("SignupDate").Value = DateSerial(2026, 6, 1) + TimeSerial(1, 2, 3)
    rs.Fields("IsPreferred").Value = True
    rs.Fields("CustomerGuid").Value = "{00090002-1111-2222-3333-000000900002}"
    rs.Fields("ProfileHash").Value = Bytes(Array(16, 15, 14, 13, 12, 11, 10, 9, 8, 7, 6, 5, 4, 3, 2, 1))
    rs.Update

    rs.MarshalOptions = adMarshalModifiedOnly
    rs.Save xmlPath, adPersistXML
    rs.Save adtgPath, adPersistADTG
    rs.Close

    Set rs = CreateObject("ADODB.Recordset")
    rs.CursorLocation = adUseClient
    rs.Open adtgPath, "Provider=MSPersist", adOpenStatic, adLockBatchOptimistic, adCmdFile
    rs.Save roundtripPath, adPersistXML
    rs.Close

    AppendText fso.BuildPath(root, "manifest.csv"), _
        Csv(Array("sales_customers_marshal_modified", "sqlserver_provider_marshal_modified_only", "1", "2", xmlPath, adtgPath, roundtripPath)) & vbCrLf
End Sub

Sub SaveSqlServerSortedRecordset(cn)
    Dim rs, xmlPath, adtgPath, roundtripPath
    xmlPath = fso.BuildPath(root, "sales_customers_sorted.xml")
    adtgPath = fso.BuildPath(root, "sales_customers_sorted.adtg")
    roundtripPath = fso.BuildPath(root, "sales_customers_sorted.roundtrip.xml")
    DeleteIfExists xmlPath
    DeleteIfExists adtgPath
    DeleteIfExists roundtripPath

    Set rs = CreateObject("ADODB.Recordset")
    rs.CursorLocation = adUseClient
    rs.Open "SELECT TOP 8 CustomerId, RegionId, CustomerCode, CreditLimit FROM dbo.SalesCustomers ORDER BY CustomerId", cn, adOpenStatic, adLockBatchOptimistic, adCmdText
    rs.Sort = "RegionId ASC, CreditLimit DESC"

    rs.Save xmlPath, adPersistXML
    rs.Save adtgPath, adPersistADTG
    rs.Close

    Set rs = CreateObject("ADODB.Recordset")
    rs.CursorLocation = adUseClient
    rs.Open adtgPath, "Provider=MSPersist", adOpenStatic, adLockBatchOptimistic, adCmdFile
    rs.Save roundtripPath, adPersistXML
    rs.Close

    AppendText fso.BuildPath(root, "manifest.csv"), _
        Csv(Array("sales_customers_sorted", "sqlserver_provider_sorted_view", "1", "8", xmlPath, adtgPath, roundtripPath)) & vbCrLf
End Sub

Sub SaveSqlServerFilteredRecordset(cn)
    Dim rs, xmlPath, adtgPath, roundtripPath
    xmlPath = fso.BuildPath(root, "sales_customers_filtered.xml")
    adtgPath = fso.BuildPath(root, "sales_customers_filtered.adtg")
    roundtripPath = fso.BuildPath(root, "sales_customers_filtered.roundtrip.xml")
    DeleteIfExists xmlPath
    DeleteIfExists adtgPath
    DeleteIfExists roundtripPath

    Set rs = CreateObject("ADODB.Recordset")
    rs.CursorLocation = adUseClient
    rs.Open "SELECT TOP 12 CustomerId, RegionId, CustomerCode, CreditLimit FROM dbo.SalesCustomers ORDER BY CustomerId", cn, adOpenStatic, adLockBatchOptimistic, adCmdText
    rs.Filter = "CreditLimit >= 11731 AND CustomerCode <= 'CUST0008'"

    rs.Save xmlPath, adPersistXML
    rs.Save adtgPath, adPersistADTG
    rs.Close

    Set rs = CreateObject("ADODB.Recordset")
    rs.CursorLocation = adUseClient
    rs.Open adtgPath, "Provider=MSPersist", adOpenStatic, adLockBatchOptimistic, adCmdFile
    rs.Save roundtripPath, adPersistXML
    rs.Close

    AppendText fso.BuildPath(root, "manifest.csv"), _
        Csv(Array("sales_customers_filtered", "sqlserver_provider_filtered_view", "1", "5", xmlPath, adtgPath, roundtripPath)) & vbCrLf
End Sub

Sub SaveSqlServerCursorOptimisticRecordset(cn)
    Dim rs, xmlPath, adtgPath, roundtripPath
    xmlPath = fso.BuildPath(root, "sales_customers_server_static_optimistic.xml")
    adtgPath = fso.BuildPath(root, "sales_customers_server_static_optimistic.adtg")
    roundtripPath = fso.BuildPath(root, "sales_customers_server_static_optimistic.roundtrip.xml")
    DeleteIfExists xmlPath
    DeleteIfExists adtgPath
    DeleteIfExists roundtripPath

    Set rs = CreateObject("ADODB.Recordset")
    rs.CursorLocation = adUseServer
    rs.Open "SELECT TOP 6 CustomerId, RegionId, CustomerCode, CreditLimit FROM dbo.SalesCustomers ORDER BY CustomerId", cn, adOpenStatic, adLockOptimistic, adCmdText

    rs.Save xmlPath, adPersistXML
    rs.Save adtgPath, adPersistADTG
    rs.Close

    Set rs = CreateObject("ADODB.Recordset")
    rs.CursorLocation = adUseClient
    rs.Open adtgPath, "Provider=MSPersist", adOpenStatic, adLockBatchOptimistic, adCmdFile
    rs.Save roundtripPath, adPersistXML
    rs.Close

    AppendText fso.BuildPath(root, "manifest.csv"), _
        Csv(Array("sales_customers_server_static_optimistic", "sqlserver_server_cursor_optimistic_extended_descriptors", "1", "6", xmlPath, adtgPath, roundtripPath)) & vbCrLf
End Sub

Sub SaveSqlServerCursorKeysetReadOnlyRecordset(cn)
    Dim rs, xmlPath, adtgPath, roundtripPath
    xmlPath = fso.BuildPath(root, "sales_customers_server_keyset_readonly.xml")
    adtgPath = fso.BuildPath(root, "sales_customers_server_keyset_readonly.adtg")
    roundtripPath = fso.BuildPath(root, "sales_customers_server_keyset_readonly.roundtrip.xml")
    DeleteIfExists xmlPath
    DeleteIfExists adtgPath
    DeleteIfExists roundtripPath

    Set rs = CreateObject("ADODB.Recordset")
    rs.CursorLocation = adUseServer
    rs.Open "SELECT TOP 6 CustomerId, RegionId, CustomerCode, CreditLimit FROM dbo.SalesCustomers ORDER BY CustomerId", cn, adOpenKeyset, adLockReadOnly, adCmdText

    rs.Save xmlPath, adPersistXML
    rs.Save adtgPath, adPersistADTG
    rs.Close

    Set rs = CreateObject("ADODB.Recordset")
    rs.CursorLocation = adUseClient
    rs.Open adtgPath, "Provider=MSPersist", adOpenStatic, adLockBatchOptimistic, adCmdFile
    rs.Save roundtripPath, adPersistXML
    rs.Close

    AppendText fso.BuildPath(root, "manifest.csv"), _
        Csv(Array("sales_customers_server_keyset_readonly", "sqlserver_server_cursor_keyset_readonly_key_descriptor", "1", "6", xmlPath, adtgPath, roundtripPath)) & vbCrLf
End Sub

Sub SaveSqlServerCursorForwardOnlyReadOnlyRecordset(cn)
    Dim rs, xmlPath, adtgPath, roundtripPath
    xmlPath = fso.BuildPath(root, "sales_customers_server_forwardonly_readonly.xml")
    adtgPath = fso.BuildPath(root, "sales_customers_server_forwardonly_readonly.adtg")
    roundtripPath = fso.BuildPath(root, "sales_customers_server_forwardonly_readonly.roundtrip.xml")
    DeleteIfExists xmlPath
    DeleteIfExists adtgPath
    DeleteIfExists roundtripPath

    Set rs = CreateObject("ADODB.Recordset")
    rs.CursorLocation = adUseServer
    rs.Open "SELECT TOP 5 CustomerId, RegionId, CustomerCode, CreditLimit FROM dbo.SalesCustomers ORDER BY CustomerId", cn, adOpenForwardOnly, adLockReadOnly, adCmdText

    rs.Save xmlPath, adPersistXML
    rs.Save adtgPath, adPersistADTG
    rs.Close

    Set rs = CreateObject("ADODB.Recordset")
    rs.CursorLocation = adUseClient
    rs.Open adtgPath, "Provider=MSPersist", adOpenStatic, adLockBatchOptimistic, adCmdFile
    rs.Save roundtripPath, adPersistXML
    rs.Close

    AppendText fso.BuildPath(root, "manifest.csv"), _
        Csv(Array("sales_customers_server_forwardonly_readonly", "sqlserver_server_cursor_forwardonly_readonly_unknown_row_count", "1", "5", xmlPath, adtgPath, roundtripPath)) & vbCrLf
End Sub

Sub SaveSqlServerCursorDynamicReadOnlyRecordset(cn)
    Dim rs, xmlPath, adtgPath, roundtripPath
    xmlPath = fso.BuildPath(root, "sales_customers_server_dynamic_readonly.xml")
    adtgPath = fso.BuildPath(root, "sales_customers_server_dynamic_readonly.adtg")
    roundtripPath = fso.BuildPath(root, "sales_customers_server_dynamic_readonly.roundtrip.xml")
    DeleteIfExists xmlPath
    DeleteIfExists adtgPath
    DeleteIfExists roundtripPath

    Set rs = CreateObject("ADODB.Recordset")
    rs.CursorLocation = adUseServer
    rs.Open "SELECT TOP 5 CustomerId, RegionId, CustomerCode, CreditLimit FROM dbo.SalesCustomers ORDER BY CustomerId", cn, adOpenDynamic, adLockReadOnly, adCmdText

    rs.Save xmlPath, adPersistXML
    rs.Save adtgPath, adPersistADTG
    rs.Close

    Set rs = CreateObject("ADODB.Recordset")
    rs.CursorLocation = adUseClient
    rs.Open adtgPath, "Provider=MSPersist", adOpenStatic, adLockBatchOptimistic, adCmdFile
    rs.Save roundtripPath, adPersistXML
    rs.Close

    AppendText fso.BuildPath(root, "manifest.csv"), _
        Csv(Array("sales_customers_server_dynamic_readonly", "sqlserver_server_cursor_dynamic_readonly_distinct_descriptor_bytes", "1", "5", xmlPath, adtgPath, roundtripPath)) & vbCrLf
End Sub

Sub SaveSqlServerDuplicateAliasRecordset(cn)
    Dim rs, xmlPath, adtgPath, roundtripPath
    xmlPath = fso.BuildPath(root, "sales_customers_duplicate_alias.xml")
    adtgPath = fso.BuildPath(root, "sales_customers_duplicate_alias.adtg")
    roundtripPath = fso.BuildPath(root, "sales_customers_duplicate_alias.roundtrip.xml")
    DeleteIfExists xmlPath
    DeleteIfExists adtgPath
    DeleteIfExists roundtripPath

    Set rs = CreateObject("ADODB.Recordset")
    rs.CursorLocation = adUseClient
    rs.Open "SELECT TOP 4 CustomerId AS [DUP], RegionId AS [DUP], CustomerCode AS [DUP], CreditLimit AS [DUP] FROM dbo.SalesCustomers ORDER BY CustomerId", cn, adOpenStatic, adLockBatchOptimistic, adCmdText

    rs.Save xmlPath, adPersistXML
    rs.Save adtgPath, adPersistADTG
    rs.Close

    Set rs = CreateObject("ADODB.Recordset")
    rs.CursorLocation = adUseClient
    rs.Open adtgPath, "Provider=MSPersist", adOpenStatic, adLockBatchOptimistic, adCmdFile
    rs.Save roundtripPath, adPersistXML
    rs.Close

    AppendText fso.BuildPath(root, "manifest.csv"), _
        Csv(Array("sales_customers_duplicate_alias", "sqlserver_provider_duplicate_aliases", "1", "4", xmlPath, adtgPath, roundtripPath)) & vbCrLf
End Sub

Function SqlVariantSupportedQuery()
    SqlVariantSupportedQuery = _
        "SELECT 1 AS ID, " & _
        "CAST(CAST(123 AS int) AS sql_variant) AS VAR_INT, " & _
        "CAST(CAST(922337203685477580 AS bigint) AS sql_variant) AS VAR_BIGINT, " & _
        "CAST(CAST(123.45 AS decimal(9,2)) AS sql_variant) AS VAR_DECIMAL, " & _
        "CAST(CAST(123.4567 AS money) AS sql_variant) AS VAR_MONEY, " & _
        "CAST(CAST(123.25 AS float) AS sql_variant) AS VAR_FLOAT, " & _
        "CAST(CAST(12.5 AS real) AS sql_variant) AS VAR_REAL, " & _
        "CAST(CAST(1 AS bit) AS sql_variant) AS VAR_BIT, " & _
        "CAST(CAST('2024-02-03T04:05:06' AS datetime) AS sql_variant) AS VAR_DATETIME " & _
        "UNION ALL SELECT 2, " & _
        "CAST(CAST(-5 AS int) AS sql_variant), " & _
        "CAST(CAST(-922337203685477580 AS bigint) AS sql_variant), " & _
        "CAST(CAST(-987.65 AS numeric(9,2)) AS sql_variant), " & _
        "CAST(CAST(-987.6543 AS money) AS sql_variant), " & _
        "CAST(CAST(-987.5 AS float) AS sql_variant), " & _
        "CAST(CAST(-9.25 AS real) AS sql_variant), " & _
        "CAST(CAST(0 AS bit) AS sql_variant), " & _
        "CAST(CAST('1999-12-31T23:59:59' AS datetime) AS sql_variant)"
End Function

Function LegacyLobJoinQuery()
    LegacyLobJoinQuery = _
        "SELECT ld.LegacyDocId AS DOC_ID, o.OrderId AS ORDER_ID, ol.LineId AS LINE_ID, r.RegionCode AS REGION_CODE, c.CustomerCode AS CUSTOMER_CODE, p.ProductName AS PRODUCT_NAME, cat.CategoryName AS CATEGORY_NAME, " & _
        "ld.LegacyCode AS LEGACY_CODE, ld.LegacyText AS LEGACY_TEXT, ld.LegacyNText AS LEGACY_NTEXT, ld.LegacyImage AS LEGACY_IMAGE, ld.LegacyRowVersion AS LEGACY_ROWVERSION, sh.TrackingNumber AS TRACKING_NUMBER, sh.ShipLabel AS SHIP_LABEL " & _
        "FROM dbo.SalesLegacyDocs AS ld INNER JOIN dbo.SalesOrderLines AS ol ON ol.LineId = ld.LineId INNER JOIN dbo.SalesOrders AS o ON o.OrderId = ld.OrderId INNER JOIN dbo.SalesCustomers AS c ON c.CustomerId = o.CustomerId INNER JOIN dbo.SalesRegions AS r ON r.RegionId = c.RegionId " & _
        "INNER JOIN dbo.SalesProducts AS p ON p.ProductId = ol.ProductId INNER JOIN dbo.SalesCategories AS cat ON cat.CategoryId = p.CategoryId LEFT JOIN dbo.SalesShipments AS sh ON sh.OrderId = o.OrderId ORDER BY ld.LegacyDocId"
End Function

Function Bytes(values)
    Dim doc, node, hexText, i
    hexText = ""
    For i = 0 To UBound(values)
        hexText = hexText & HexByte(CInt(values(i)))
    Next

    Set doc = CreateObject("MSXML2.DOMDocument.6.0")
    Set node = doc.createElement("bytes")
    node.dataType = "bin.hex"
    node.Text = hexText
    Bytes = node.nodeTypedValue
End Function

Function HexByte(value)
    HexByte = Right("0" & Hex(value And 255), 2)
End Function

Sub SaveSqlVariantFailureMatrix(cn)
    Dim path, cases, i
    path = fso.BuildPath(root, "sql_variant_failures.csv")
    DeleteIfExists path
    WriteText path, "case,ado_type,result,stage,error_number,error_description" & vbCrLf

    cases = Array( _
        Array("variant_uniqueidentifier", "CAST(CAST('00000001-1111-2222-3333-000000000001' AS uniqueidentifier) AS sql_variant)", "CAST(CAST('00000002-1111-2222-3333-000000000002' AS uniqueidentifier) AS sql_variant)"), _
        Array("variant_varchar", "CAST(CAST('hello' AS varchar(20)) AS sql_variant)", "CAST(CAST('world' AS varchar(20)) AS sql_variant)"), _
        Array("variant_nvarchar", "CAST(CAST(NCHAR(50504) + NCHAR(45397) AS nvarchar(20)) AS sql_variant)", "CAST(CAST(NCHAR(54844) + NCHAR(54633) AS nvarchar(20)) AS sql_variant)"), _
        Array("variant_varbinary", "CAST(CAST(0x010203 AS varbinary(3)) AS sql_variant)", "CAST(CAST(0xA0B0C0 AS varbinary(3)) AS sql_variant)") _
    )

    For i = 0 To UBound(cases)
        ProbeSqlVariantFailure cn, path, cases(i)
    Next
End Sub

Sub ProbeSqlVariantFailure(cn, csvPath, def)
    Dim rs, query, adoType, result, stage, errorNumber, errorDescription
    adoType = ""
    result = "ok"
    stage = "open"
    errorNumber = ""
    errorDescription = ""

    On Error Resume Next
    Set rs = CreateObject("ADODB.Recordset")
    rs.CursorLocation = adUseClient
    query = "SELECT 1 AS ID, " & def(1) & " AS VALUE_FIELD UNION ALL SELECT 2, " & def(2)
    rs.Open query, cn, adOpenStatic, adLockBatchOptimistic, adCmdText
    If Err.Number = 0 Then
        adoType = CStr(rs.Fields("VALUE_FIELD").Type)
        stage = "save_adtg"
        rs.Save fso.BuildPath(root, def(0) & ".adtg"), adPersistADTG
    End If
    If Err.Number <> 0 Then
        result = "error"
        errorNumber = CStr(Err.Number)
        errorDescription = Err.Description
        Err.Clear
    End If
    If Not rs Is Nothing Then
        If rs.State <> 0 Then rs.Close
    End If
    On Error GoTo 0

    DeleteIfExists fso.BuildPath(root, def(0) & ".adtg")
    AppendText csvPath, Csv(Array(def(0), adoType, result, stage, errorNumber, errorDescription)) & vbCrLf
End Sub

Sub ExecSql(cn, sql, label)
    On Error Resume Next
    cn.Execute sql, , adExecuteNoRecords
    If Err.Number <> 0 Then
        WScript.Echo "SQL failed during " & label & ": " & Err.Number & " " & Err.Description
        WScript.Quit 1
    End If
    On Error GoTo 0
End Sub

Function ReadText(path)
    Dim stream
    Set stream = CreateObject("ADODB.Stream")
    stream.Type = 2
    stream.Charset = "utf-8"
    stream.Open
    stream.LoadFromFile path
    ReadText = stream.ReadText
    stream.Close
End Function

Sub WriteText(path, text)
    Dim stream
    Set stream = CreateObject("ADODB.Stream")
    stream.Type = 2
    stream.Charset = "utf-8"
    stream.Open
    stream.WriteText text
    stream.SaveToFile path, 2
    stream.Close
End Sub

Sub AppendText(path, text)
    Dim stream
    Set stream = CreateObject("ADODB.Stream")
    stream.Type = 2
    stream.Charset = "utf-8"
    stream.Open
    If fso.FileExists(path) Then
        stream.LoadFromFile path
        stream.Position = stream.Size
    End If
    stream.WriteText text
    stream.SaveToFile path, 2
    stream.Close
End Sub

Function Csv(values)
    Dim parts(), i
    ReDim parts(UBound(values))
    For i = 0 To UBound(values)
        parts(i) = CsvValue(CStr(values(i)))
    Next
    Csv = Join(parts, ",")
End Function

Function CsvValue(value)
    CsvValue = """" & Replace(value, """", """""") & """"
End Function

Function SqlString(value)
    SqlString = Replace(value, "'", "''")
End Function

Function SqlName(value)
    SqlName = "[" & Replace(value, "]", "]]") & "]"
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
