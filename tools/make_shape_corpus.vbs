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

Dim server, userName, password, databaseName, root
server = ArgText(0, "SERVER")
userName = ArgText(1, "USER")
password = ArgText(2, "<password>")
databaseName = ArgText(3, "AdoRecordsetSales")
root = ArgText(4, fso.BuildPath(fso.GetParentFolderName(WScript.ScriptFullName), "..\corpus\shape"))

EnsureFolder root
DeleteIfExists fso.BuildPath(root, "manifest.csv")

Dim cn
Set cn = OpenShapeConnection(databaseName)
SaveSingleChapterShape cn
SaveMultiChapterShape cn
SaveAggregateChapterShape cn
SaveStatisticsAggregateShape cn
SaveComputeGroupShape cn
SaveCalcNewShape cn
SaveCalcNewPendingShape cn
SaveGrandchildAggregateShape cn
SaveNestedChapterShape cn
SaveNestedSiblingGrandchildShape cn
SaveNestedSiblingGrandchildPendingShape cn
SaveDeepNestedChapterShape cn
SaveCompositeRelationShape cn
SaveDateCurrencyRelationShape cn
SaveSmallDateTimeSmallMoneyRelationShape cn
SaveTinyIntSmallIntRelationShape cn
SaveBigIntRelationShape cn
SaveDecimalNumericRelationShape cn
SaveRealFloatRelationShape cn
SaveNullableChapterShape cn
SaveWideNullableChapterShape cn
SaveGuidRelationShape cn
SaveTextRelationShape cn
SaveUnicodeRelationShape cn
SaveBinaryRelationShape cn
SaveRowVersionRelationShape cn
SaveBooleanRelationShape cn
SaveNullableRelationShape cn
SaveDuplicateParentRelationShape cn
SaveSparseAggregateShape cn
SaveSparseChildShape cn
SaveEmptyChildShape cn
SaveEmptyParentShape cn
SavePendingChangesShape cn
SaveParentInsertDeleteShape cn
SaveParentRelationKeyUpdateShape cn
SaveChildRelationKeyUpdateShape cn
SaveCompositeParentRelationKeyUpdateShape cn
SaveCompositeChildRelationKeyUpdateShape cn
SaveNestedPendingChangesShape cn
cn.Close

WScript.Echo "Generated shaped ADO corpus in " & fso.GetAbsolutePathName(root)

Function ArgText(index, defaultValue)
    If WScript.Arguments.Count > index Then
        ArgText = WScript.Arguments(index)
    Else
        ArgText = defaultValue
    End If
End Function

Function OpenShapeConnection(catalog)
    Dim cn
    Set cn = CreateObject("ADODB.Connection")
    cn.ConnectionTimeout = 15
    cn.CommandTimeout = 120
    cn.Open "Provider=MSDataShape;Data Provider=SQLOLEDB;Data Source=" & server & ";Initial Catalog=" & catalog & ";User ID=" & userName & ";Password=" & password & ";"
    Set OpenShapeConnection = cn
End Function

Sub SaveSingleChapterShape(cn)
    Dim rs, xmlPath, adtgPath, roundtripPath
    xmlPath = fso.BuildPath(root, "orders_lines_shape.xml")
    adtgPath = fso.BuildPath(root, "orders_lines_shape.adtg")
    roundtripPath = fso.BuildPath(root, "orders_lines_shape.roundtrip.xml")
    DeleteIfExists xmlPath
    DeleteIfExists adtgPath
    DeleteIfExists roundtripPath

    Set rs = CreateObject("ADODB.Recordset")
    rs.CursorLocation = adUseClient
    rs.Open SingleChapterShapeQuery(), cn, adOpenStatic, adLockBatchOptimistic, adCmdText

    rs.Save xmlPath, adPersistXML
    rs.Save adtgPath, adPersistADTG
    rs.Close

    Set rs = CreateObject("ADODB.Recordset")
    rs.CursorLocation = adUseClient
    rs.Open adtgPath, "Provider=MSPersist", adOpenStatic, adLockBatchOptimistic, adCmdFile
    rs.Save roundtripPath, adPersistXML
    rs.Close

    AppendManifest Csv(Array("orders_lines_shape", "sqlserver_msdatashape_single_chapter", "3", "1", xmlPath, adtgPath, roundtripPath))
End Sub

Function SingleChapterShapeQuery()
    SingleChapterShapeQuery = _
        "SHAPE {SELECT TOP 3 OrderId, CustomerId, Freight FROM dbo.SalesOrders ORDER BY OrderId} AS Orders " & _
        "APPEND ({SELECT OrderId, LineId, Quantity FROM dbo.SalesOrderLines WHERE OrderId BETWEEN 100001 AND 100003 ORDER BY OrderId, LineNumber} AS Lines RELATE OrderId TO OrderId) AS Lines"
End Function

Sub SaveMultiChapterShape(cn)
    Dim rs, xmlPath, adtgPath, roundtripPath
    xmlPath = fso.BuildPath(root, "orders_lines_payments_shape.xml")
    adtgPath = fso.BuildPath(root, "orders_lines_payments_shape.adtg")
    roundtripPath = fso.BuildPath(root, "orders_lines_payments_shape.roundtrip.xml")
    DeleteIfExists xmlPath
    DeleteIfExists adtgPath
    DeleteIfExists roundtripPath

    Set rs = CreateObject("ADODB.Recordset")
    rs.CursorLocation = adUseClient
    rs.Open MultiChapterShapeQuery(), cn, adOpenStatic, adLockBatchOptimistic, adCmdText

    rs.Save xmlPath, adPersistXML
    rs.Save adtgPath, adPersistADTG
    rs.Close

    Set rs = CreateObject("ADODB.Recordset")
    rs.CursorLocation = adUseClient
    rs.Open adtgPath, "Provider=MSPersist", adOpenStatic, adLockBatchOptimistic, adCmdFile
    rs.Save roundtripPath, adPersistXML
    rs.Close

    AppendManifest Csv(Array("orders_lines_payments_shape", "sqlserver_msdatashape_two_chapters", "5", "2", xmlPath, adtgPath, roundtripPath))
End Sub

Function MultiChapterShapeQuery()
    MultiChapterShapeQuery = _
        "SHAPE {SELECT TOP 5 OrderId, CustomerId, Freight, OrderDate FROM dbo.SalesOrders ORDER BY OrderId} AS Orders " & _
        "APPEND ({SELECT OrderId, LineId, LineNumber, Quantity, UnitPrice FROM dbo.SalesOrderLines WHERE OrderId BETWEEN 100001 AND 100005 ORDER BY OrderId, LineNumber} AS Lines RELATE OrderId TO OrderId) AS Lines, " & _
        "({SELECT OrderId, PaymentId, PaymentMethod, PaymentAmount, Approved FROM dbo.SalesPayments WHERE OrderId BETWEEN 100001 AND 100005 ORDER BY OrderId} AS Payments RELATE OrderId TO OrderId) AS Payments"
End Function

Sub SaveAggregateChapterShape(cn)
    Dim rs, xmlPath, adtgPath, roundtripPath
    xmlPath = fso.BuildPath(root, "orders_lines_aggregate_shape.xml")
    adtgPath = fso.BuildPath(root, "orders_lines_aggregate_shape.adtg")
    roundtripPath = fso.BuildPath(root, "orders_lines_aggregate_shape.roundtrip.xml")
    DeleteIfExists xmlPath
    DeleteIfExists adtgPath
    DeleteIfExists roundtripPath

    Set rs = CreateObject("ADODB.Recordset")
    rs.CursorLocation = adUseClient
    rs.Open AggregateChapterShapeQuery(), cn, adOpenStatic, adLockBatchOptimistic, adCmdText

    rs.Save xmlPath, adPersistXML
    rs.Save adtgPath, adPersistADTG
    rs.Close

    Set rs = CreateObject("ADODB.Recordset")
    rs.CursorLocation = adUseClient
    rs.Open adtgPath, "Provider=MSPersist", adOpenStatic, adLockBatchOptimistic, adCmdFile
    rs.Save roundtripPath, adPersistXML
    rs.Close

    AppendManifest Csv(Array("orders_lines_aggregate_shape", "sqlserver_msdatashape_append_aggregates", "3", "1", xmlPath, adtgPath, roundtripPath))
End Sub

Function AggregateChapterShapeQuery()
    AggregateChapterShapeQuery = _
        "SHAPE {SELECT TOP 3 OrderId, CustomerId, Freight FROM dbo.SalesOrders ORDER BY OrderId} AS Orders " & _
        "APPEND ({SELECT OrderId, LineId, LineNumber, Quantity, UnitPrice, CAST(Quantity * UnitPrice AS money) AS LineTotal FROM dbo.SalesOrderLines WHERE OrderId BETWEEN 100001 AND 100003 ORDER BY OrderId, LineNumber} AS Lines RELATE OrderId TO OrderId) AS Lines, " & _
        "SUM(Lines.LineTotal) AS LineTotalSum, COUNT(Lines.LineId) AS LineCount, MIN(Lines.Quantity) AS MinQuantity, MAX(Lines.Quantity) AS MaxQuantity, ANY(Lines.LineNumber) AS AnyLineNumber"
End Function

Sub SaveStatisticsAggregateShape(cn)
    Dim rs, xmlPath, adtgPath, roundtripPath
    xmlPath = fso.BuildPath(root, "orders_lines_statistics_shape.xml")
    adtgPath = fso.BuildPath(root, "orders_lines_statistics_shape.adtg")
    roundtripPath = fso.BuildPath(root, "orders_lines_statistics_shape.roundtrip.xml")
    DeleteIfExists xmlPath
    DeleteIfExists adtgPath
    DeleteIfExists roundtripPath

    Set rs = CreateObject("ADODB.Recordset")
    rs.CursorLocation = adUseClient
    rs.Open StatisticsAggregateShapeQuery(), cn, adOpenStatic, adLockBatchOptimistic, adCmdText

    rs.Save xmlPath, adPersistXML
    rs.Save adtgPath, adPersistADTG
    rs.Close

    Set rs = CreateObject("ADODB.Recordset")
    rs.CursorLocation = adUseClient
    rs.Open adtgPath, "Provider=MSPersist", adOpenStatic, adLockBatchOptimistic, adCmdFile
    rs.Save roundtripPath, adPersistXML
    rs.Close

    AppendManifest Csv(Array("orders_lines_statistics_shape", "sqlserver_msdatashape_statistics_aggregates", "3", "1", xmlPath, adtgPath, roundtripPath))
End Sub

Function StatisticsAggregateShapeQuery()
    StatisticsAggregateShapeQuery = _
        "SHAPE {SELECT TOP 3 OrderId, CustomerId, Freight FROM dbo.SalesOrders ORDER BY OrderId} AS Orders " & _
        "APPEND ({SELECT OrderId, LineId, LineNumber, Quantity, UnitPrice, CAST(Quantity * UnitPrice AS money) AS LineTotal FROM dbo.SalesOrderLines WHERE OrderId BETWEEN 100001 AND 100003 ORDER BY OrderId, LineNumber} AS Lines RELATE OrderId TO OrderId) AS Lines, " & _
        "AVG(Lines.Quantity) AS AvgQuantity, STDEV(Lines.Quantity) AS QuantityStdev, COUNT(Lines) AS LineRows, COUNT(Lines.LineTotal) AS LineTotalCount"
End Function

Sub SaveComputeGroupShape(cn)
    Dim rs, xmlPath, adtgPath, roundtripPath
    xmlPath = fso.BuildPath(root, "customer_lines_compute_shape.xml")
    adtgPath = fso.BuildPath(root, "customer_lines_compute_shape.adtg")
    roundtripPath = fso.BuildPath(root, "customer_lines_compute_shape.roundtrip.xml")
    DeleteIfExists xmlPath
    DeleteIfExists adtgPath
    DeleteIfExists roundtripPath

    Set rs = CreateObject("ADODB.Recordset")
    rs.CursorLocation = adUseClient
    rs.Open ComputeGroupShapeQuery(), cn, adOpenStatic, adLockBatchOptimistic, adCmdText

    rs.Save xmlPath, adPersistXML
    rs.Save adtgPath, adPersistADTG
    rs.Close

    Set rs = CreateObject("ADODB.Recordset")
    rs.CursorLocation = adUseClient
    rs.Open adtgPath, "Provider=MSPersist", adOpenStatic, adLockBatchOptimistic, adCmdFile
    rs.Save roundtripPath, adPersistXML
    rs.Close

    AppendManifest Csv(Array("customer_lines_compute_shape", "sqlserver_msdatashape_compute_group", "3", "1", xmlPath, adtgPath, roundtripPath))
End Sub

Function ComputeGroupShapeQuery()
    ComputeGroupShapeQuery = _
        "SHAPE {SELECT o.CustomerId, o.OrderId, ol.LineId, ol.Quantity, ol.UnitPrice, CAST(ol.Quantity * ol.UnitPrice AS money) AS LineTotal " & _
        "FROM dbo.SalesOrders AS o INNER JOIN dbo.SalesOrderLines AS ol ON ol.OrderId = o.OrderId " & _
        "WHERE o.CustomerId BETWEEN 1 AND 3 AND o.OrderId BETWEEN 100001 AND 100099 ORDER BY o.CustomerId, o.OrderId, ol.LineNumber} AS Lines " & _
        "COMPUTE Lines AS Lines, SUM(Lines.LineTotal) AS LineTotalSum, COUNT(Lines.LineId) AS LineCount BY CustomerId"
End Function

Sub SaveCalcNewShape(cn)
    Dim rs, xmlPath, adtgPath, roundtripPath
    xmlPath = fso.BuildPath(root, "orders_child_calc_new_shape.xml")
    adtgPath = fso.BuildPath(root, "orders_child_calc_new_shape.adtg")
    roundtripPath = fso.BuildPath(root, "orders_child_calc_new_shape.roundtrip.xml")
    DeleteIfExists xmlPath
    DeleteIfExists adtgPath
    DeleteIfExists roundtripPath

    Set rs = CreateObject("ADODB.Recordset")
    rs.CursorLocation = adUseClient
    rs.Open CalcNewShapeQuery(), cn, adOpenStatic, adLockBatchOptimistic, adCmdText

    rs.Save xmlPath, adPersistXML
    rs.Save adtgPath, adPersistADTG
    rs.Close

    Set rs = CreateObject("ADODB.Recordset")
    rs.CursorLocation = adUseClient
    rs.Open adtgPath, "Provider=MSPersist", adOpenStatic, adLockBatchOptimistic, adCmdFile
    rs.Save roundtripPath, adPersistXML
    rs.Close

    AppendManifest Csv(Array("orders_child_calc_new_shape", "sqlserver_msdatashape_calc_new_columns", "3", "1", xmlPath, adtgPath, roundtripPath))
End Sub

Function CalcNewShapeQuery()
    CalcNewShapeQuery = _
        "SHAPE {SELECT TOP 3 OrderId, CustomerId, Freight FROM dbo.SalesOrders ORDER BY OrderId} AS Orders " & _
        "APPEND CALC(OrderId + CustomerId) AS OrderCustomerCalc, NEW adVarWChar(40) AS ReviewNote, " & _
        "((SHAPE {SELECT OrderId, LineId, LineNumber, Quantity, UnitPrice FROM dbo.SalesOrderLines WHERE OrderId BETWEEN 100001 AND 100003 ORDER BY OrderId, LineNumber} AS Lines " & _
        "APPEND CALC(Quantity + LineNumber) AS QuantityLineCalc, NEW adInteger AS LineScore) " & _
        "RELATE OrderId TO OrderId) AS Lines"
End Function

Sub SaveCalcNewPendingShape(cn)
    Dim rs, child, xmlPath, adtgPath, roundtripPath
    xmlPath = fso.BuildPath(root, "orders_calc_new_pending_shape.xml")
    adtgPath = fso.BuildPath(root, "orders_calc_new_pending_shape.adtg")
    roundtripPath = fso.BuildPath(root, "orders_calc_new_pending_shape.roundtrip.xml")
    DeleteIfExists xmlPath
    DeleteIfExists adtgPath
    DeleteIfExists roundtripPath

    Set rs = CreateObject("ADODB.Recordset")
    rs.CursorLocation = adUseClient
    rs.Open CalcNewShapeQuery(), cn, adOpenStatic, adLockBatchOptimistic, adCmdText

    rs.MoveFirst
    rs.Fields("ReviewNote").Value = "parent-note-100001"
    rs.Update
    Set child = rs.Fields("Lines").Value
    child.MoveFirst
    child.Fields("LineScore").Value = 501
    child.Update

    rs.MoveNext
    rs.Fields("ReviewNote").Value = "parent-note-100002"
    rs.Update
    Set child = rs.Fields("Lines").Value
    child.MoveFirst
    child.Fields("LineScore").Value = 502
    child.Update

    ' MDAC refuses XML persistence for updated hierarchical Recordsets.
    rs.Save adtgPath, adPersistADTG
    rs.Close

    AppendManifest Csv(Array("orders_calc_new_pending_shape", "sqlserver_msdatashape_calc_new_pending_adtg_only", "3", "1", "", adtgPath, ""))
End Sub

Sub SaveGrandchildAggregateShape(cn)
    Dim rs, xmlPath, adtgPath, roundtripPath
    xmlPath = fso.BuildPath(root, "orders_lines_product_aggregate_shape.xml")
    adtgPath = fso.BuildPath(root, "orders_lines_product_aggregate_shape.adtg")
    roundtripPath = fso.BuildPath(root, "orders_lines_product_aggregate_shape.roundtrip.xml")
    DeleteIfExists xmlPath
    DeleteIfExists adtgPath
    DeleteIfExists roundtripPath

    Set rs = CreateObject("ADODB.Recordset")
    rs.CursorLocation = adUseClient
    rs.Open GrandchildAggregateShapeQuery(), cn, adOpenStatic, adLockBatchOptimistic, adCmdText

    rs.Save xmlPath, adPersistXML
    rs.Save adtgPath, adPersistADTG
    rs.Close

    Set rs = CreateObject("ADODB.Recordset")
    rs.CursorLocation = adUseClient
    rs.Open adtgPath, "Provider=MSPersist", adOpenStatic, adLockBatchOptimistic, adCmdFile
    rs.Save roundtripPath, adPersistXML
    rs.Close

    AppendManifest Csv(Array("orders_lines_product_aggregate_shape", "sqlserver_msdatashape_grandchild_aggregates", "2", "2", xmlPath, adtgPath, roundtripPath))
End Sub

Function GrandchildAggregateShapeQuery()
    GrandchildAggregateShapeQuery = _
        "SHAPE {SELECT TOP 2 OrderId, CustomerId, Freight FROM dbo.SalesOrders ORDER BY OrderId} AS Orders " & _
        "APPEND ((SHAPE {SELECT OrderId, LineId, LineNumber, ProductId, Quantity FROM dbo.SalesOrderLines WHERE OrderId BETWEEN 100001 AND 100002 ORDER BY OrderId, LineNumber} AS Lines " & _
        "APPEND ({SELECT ProductId, ProductName, UnitCost FROM dbo.SalesProducts WHERE ProductId IN (SELECT ProductId FROM dbo.SalesOrderLines WHERE OrderId BETWEEN 100001 AND 100002)} AS Product RELATE ProductId TO ProductId) AS Product) " & _
        "RELATE OrderId TO OrderId) AS Lines, SUM(Lines.Product.UnitCost) AS ProductCostSum, COUNT(Lines.Product) AS ProductRows"
End Function

Sub SaveNestedChapterShape(cn)
    Dim rs, xmlPath, adtgPath, roundtripPath
    xmlPath = fso.BuildPath(root, "orders_lines_product_shape.xml")
    adtgPath = fso.BuildPath(root, "orders_lines_product_shape.adtg")
    roundtripPath = fso.BuildPath(root, "orders_lines_product_shape.roundtrip.xml")
    DeleteIfExists xmlPath
    DeleteIfExists adtgPath
    DeleteIfExists roundtripPath

    Set rs = CreateObject("ADODB.Recordset")
    rs.CursorLocation = adUseClient
    rs.Open NestedChapterShapeQuery(), cn, adOpenStatic, adLockBatchOptimistic, adCmdText

    rs.Save xmlPath, adPersistXML
    rs.Save adtgPath, adPersistADTG
    rs.Close

    Set rs = CreateObject("ADODB.Recordset")
    rs.CursorLocation = adUseClient
    rs.Open adtgPath, "Provider=MSPersist", adOpenStatic, adLockBatchOptimistic, adCmdFile
    rs.Save roundtripPath, adPersistXML
    rs.Close

    AppendManifest Csv(Array("orders_lines_product_shape", "sqlserver_msdatashape_nested_chapter", "3", "2", xmlPath, adtgPath, roundtripPath))
End Sub

Function NestedChapterShapeQuery()
    NestedChapterShapeQuery = _
        "SHAPE {SELECT TOP 3 OrderId, CustomerId, Freight FROM dbo.SalesOrders ORDER BY OrderId} AS Orders " & _
        "APPEND ((SHAPE {SELECT OrderId, LineId, LineNumber, ProductId, Quantity FROM dbo.SalesOrderLines WHERE OrderId BETWEEN 100001 AND 100003 ORDER BY OrderId, LineNumber} AS Lines " & _
        "APPEND ({SELECT ProductId, ProductName, UnitCost FROM dbo.SalesProducts WHERE ProductId IN (SELECT ProductId FROM dbo.SalesOrderLines WHERE OrderId BETWEEN 100001 AND 100003)} AS Product RELATE ProductId TO ProductId) AS Product) " & _
        "RELATE OrderId TO OrderId) AS Lines"
End Function

Sub SaveNestedSiblingGrandchildShape(cn)
    Dim rs, xmlPath, adtgPath, roundtripPath
    xmlPath = fso.BuildPath(root, "orders_lines_product_legacy_shape.xml")
    adtgPath = fso.BuildPath(root, "orders_lines_product_legacy_shape.adtg")
    roundtripPath = fso.BuildPath(root, "orders_lines_product_legacy_shape.roundtrip.xml")
    DeleteIfExists xmlPath
    DeleteIfExists adtgPath
    DeleteIfExists roundtripPath

    Set rs = CreateObject("ADODB.Recordset")
    rs.CursorLocation = adUseClient
    rs.Open NestedSiblingGrandchildShapeQuery(), cn, adOpenStatic, adLockBatchOptimistic, adCmdText

    rs.Save xmlPath, adPersistXML
    rs.Save adtgPath, adPersistADTG
    rs.Close

    Set rs = CreateObject("ADODB.Recordset")
    rs.CursorLocation = adUseClient
    rs.Open adtgPath, "Provider=MSPersist", adOpenStatic, adLockBatchOptimistic, adCmdFile
    rs.Save roundtripPath, adPersistXML
    rs.Close

    AppendManifest Csv(Array("orders_lines_product_legacy_shape", "sqlserver_msdatashape_nested_sibling_grandchildren", "2", "3", xmlPath, adtgPath, roundtripPath))
End Sub

Function NestedSiblingGrandchildShapeQuery()
    NestedSiblingGrandchildShapeQuery = _
        "SHAPE {SELECT TOP 2 OrderId, CustomerId, Freight FROM dbo.SalesOrders ORDER BY OrderId} AS Orders " & _
        "APPEND ((SHAPE {SELECT OrderId, LineId, LineNumber, ProductId, Quantity, UnitPrice FROM dbo.SalesOrderLines WHERE OrderId BETWEEN 100001 AND 100002 ORDER BY OrderId, LineNumber} AS Lines " & _
        "APPEND ({SELECT ProductId, ProductName, ProductSku, UnitCost FROM dbo.SalesProducts ORDER BY ProductId} AS Product RELATE ProductId TO ProductId) AS Product, " & _
        "({SELECT LineId, LegacyDocId, LegacyCode, LegacyRowVersion FROM dbo.SalesLegacyDocs WHERE OrderId BETWEEN 100001 AND 100002 ORDER BY LineId} AS Legacy RELATE LineId TO LineId) AS Legacy) " & _
        "RELATE OrderId TO OrderId) AS Lines"
End Function

Sub SaveNestedSiblingGrandchildPendingShape(cn)
    Dim rs, lines, product, legacy, xmlPath, adtgPath, roundtripPath
    xmlPath = fso.BuildPath(root, "orders_lines_product_legacy_pending_shape.xml")
    adtgPath = fso.BuildPath(root, "orders_lines_product_legacy_pending_shape.adtg")
    roundtripPath = fso.BuildPath(root, "orders_lines_product_legacy_pending_shape.roundtrip.xml")
    DeleteIfExists xmlPath
    DeleteIfExists adtgPath
    DeleteIfExists roundtripPath

    Set rs = CreateObject("ADODB.Recordset")
    rs.CursorLocation = adUseClient
    rs.Open NestedSiblingGrandchildShapeQuery(), cn, adOpenStatic, adLockBatchOptimistic, adCmdText

    rs.MoveFirst
    Set lines = rs.Fields("Lines").Value
    lines.MoveFirst
    lines.Fields("Quantity").Value = 999
    lines.Update

    Set product = lines.Fields("Product").Value
    product.MoveFirst
    product.Fields("UnitCost").Value = 456.78
    product.Update

    Set legacy = lines.Fields("Legacy").Value
    legacy.MoveFirst
    legacy.Fields("LegacyCode").Value = "PX0001"
    legacy.Update

    lines.MoveNext
    Set product = lines.Fields("Product").Value
    product.MoveFirst
    product.Delete

    Set legacy = lines.Fields("Legacy").Value
    legacy.MoveFirst
    legacy.Delete

    ' MDAC refuses XML persistence for updated hierarchical Recordsets.
    rs.Save adtgPath, adPersistADTG
    rs.Close

    AppendManifest Csv(Array("orders_lines_product_legacy_pending_shape", "sqlserver_msdatashape_nested_sibling_grandchildren_pending_adtg_only", "2", "3", "", adtgPath, ""))
End Sub

Sub SaveDeepNestedChapterShape(cn)
    Dim rs, xmlPath, adtgPath, roundtripPath
    xmlPath = fso.BuildPath(root, "orders_lines_product_category_shape.xml")
    adtgPath = fso.BuildPath(root, "orders_lines_product_category_shape.adtg")
    roundtripPath = fso.BuildPath(root, "orders_lines_product_category_shape.roundtrip.xml")
    DeleteIfExists xmlPath
    DeleteIfExists adtgPath
    DeleteIfExists roundtripPath

    Set rs = CreateObject("ADODB.Recordset")
    rs.CursorLocation = adUseClient
    rs.Open DeepNestedChapterShapeQuery(), cn, adOpenStatic, adLockBatchOptimistic, adCmdText

    rs.Save xmlPath, adPersistXML
    rs.Save adtgPath, adPersistADTG
    rs.Close

    Set rs = CreateObject("ADODB.Recordset")
    rs.CursorLocation = adUseClient
    rs.Open adtgPath, "Provider=MSPersist", adOpenStatic, adLockBatchOptimistic, adCmdFile
    rs.Save roundtripPath, adPersistXML
    rs.Close

    AppendManifest Csv(Array("orders_lines_product_category_shape", "sqlserver_msdatashape_deep_nested_chapter", "2", "3", xmlPath, adtgPath, roundtripPath))
End Sub

Function DeepNestedChapterShapeQuery()
    DeepNestedChapterShapeQuery = _
        "SHAPE {SELECT TOP 2 OrderId, CustomerId, Freight FROM dbo.SalesOrders ORDER BY OrderId} AS Orders " & _
        "APPEND ((SHAPE {SELECT OrderId, LineId, LineNumber, ProductId, Quantity FROM dbo.SalesOrderLines WHERE OrderId BETWEEN 100001 AND 100002 ORDER BY OrderId, LineNumber} AS Lines " & _
        "APPEND ((SHAPE {SELECT ProductId, CategoryId, ProductName, UnitCost FROM dbo.SalesProducts WHERE ProductId IN (SELECT ProductId FROM dbo.SalesOrderLines WHERE OrderId BETWEEN 100001 AND 100002)} AS Product " & _
        "APPEND ({SELECT CategoryId, CategoryName, MarginTarget FROM dbo.SalesCategories} AS Category RELATE CategoryId TO CategoryId) AS Category) " & _
        "RELATE ProductId TO ProductId) AS Product) " & _
        "RELATE OrderId TO OrderId) AS Lines"
End Function

Sub SaveCompositeRelationShape(cn)
    Dim rs, xmlPath, adtgPath, roundtripPath
    xmlPath = fso.BuildPath(root, "orders_lines_composite_shape.xml")
    adtgPath = fso.BuildPath(root, "orders_lines_composite_shape.adtg")
    roundtripPath = fso.BuildPath(root, "orders_lines_composite_shape.roundtrip.xml")
    DeleteIfExists xmlPath
    DeleteIfExists adtgPath
    DeleteIfExists roundtripPath

    Set rs = CreateObject("ADODB.Recordset")
    rs.CursorLocation = adUseClient
    rs.Open CompositeRelationShapeQuery(), cn, adOpenStatic, adLockBatchOptimistic, adCmdText

    rs.Save xmlPath, adPersistXML
    rs.Save adtgPath, adPersistADTG
    rs.Close

    Set rs = CreateObject("ADODB.Recordset")
    rs.CursorLocation = adUseClient
    rs.Open adtgPath, "Provider=MSPersist", adOpenStatic, adLockBatchOptimistic, adCmdFile
    rs.Save roundtripPath, adPersistXML
    rs.Close

    AppendManifest Csv(Array("orders_lines_composite_shape", "sqlserver_msdatashape_composite_relation", "3", "1", xmlPath, adtgPath, roundtripPath))
End Sub

Function CompositeRelationShapeQuery()
    CompositeRelationShapeQuery = _
        "SHAPE {SELECT TOP 3 OrderId, CustomerId, Freight FROM dbo.SalesOrders ORDER BY OrderId} AS Orders " & _
        "APPEND ({SELECT o.OrderId, o.CustomerId, ol.LineId, ol.LineNumber, ol.Quantity FROM dbo.SalesOrderLines AS ol INNER JOIN dbo.SalesOrders AS o ON o.OrderId = ol.OrderId WHERE o.OrderId BETWEEN 100001 AND 100003 " & _
        "UNION ALL SELECT o.OrderId, o.CustomerId + 9000 AS CustomerId, ol.LineId + 900000000 AS LineId, ol.LineNumber, ol.Quantity FROM dbo.SalesOrderLines AS ol INNER JOIN dbo.SalesOrders AS o ON o.OrderId = ol.OrderId WHERE o.OrderId BETWEEN 100001 AND 100003 AND ol.LineNumber = 1 " & _
        "ORDER BY OrderId, CustomerId, LineNumber} AS Lines RELATE OrderId TO OrderId, CustomerId TO CustomerId) AS Lines"
End Function

Sub SaveDateCurrencyRelationShape(cn)
    Dim rs, xmlPath, adtgPath, roundtripPath
    xmlPath = fso.BuildPath(root, "orders_date_currency_relation_shape.xml")
    adtgPath = fso.BuildPath(root, "orders_date_currency_relation_shape.adtg")
    roundtripPath = fso.BuildPath(root, "orders_date_currency_relation_shape.roundtrip.xml")
    DeleteIfExists xmlPath
    DeleteIfExists adtgPath
    DeleteIfExists roundtripPath

    Set rs = CreateObject("ADODB.Recordset")
    rs.CursorLocation = adUseClient
    rs.Open DateCurrencyRelationShapeQuery(), cn, adOpenStatic, adLockBatchOptimistic, adCmdText

    rs.Save xmlPath, adPersistXML
    rs.Save adtgPath, adPersistADTG
    rs.Close

    Set rs = CreateObject("ADODB.Recordset")
    rs.CursorLocation = adUseClient
    rs.Open adtgPath, "Provider=MSPersist", adOpenStatic, adLockBatchOptimistic, adCmdFile
    rs.Save roundtripPath, adPersistXML
    rs.Close

    AppendManifest Csv(Array("orders_date_currency_relation_shape", "sqlserver_msdatashape_date_currency_relation", "3", "2", xmlPath, adtgPath, roundtripPath))
End Sub

Function DateCurrencyRelationShapeQuery()
    DateCurrencyRelationShapeQuery = _
        "SHAPE {SELECT TOP 3 OrderId, OrderDate, Freight FROM dbo.SalesOrders ORDER BY OrderId} AS Orders " & _
        "APPEND ({SELECT o.OrderDate, ol.LineId, ol.OrderId, ol.Quantity FROM dbo.SalesOrderLines AS ol INNER JOIN dbo.SalesOrders AS o ON o.OrderId = ol.OrderId WHERE ol.OrderId BETWEEN 100001 AND 100003 ORDER BY o.OrderDate, ol.LineNumber} AS DateLines RELATE OrderDate TO OrderDate) AS DateLines, " & _
        "({SELECT o.Freight, ol.LineId, ol.OrderId, ol.Quantity FROM dbo.SalesOrderLines AS ol INNER JOIN dbo.SalesOrders AS o ON o.OrderId = ol.OrderId WHERE ol.OrderId BETWEEN 100001 AND 100003 ORDER BY o.Freight, ol.LineNumber} AS FreightLines RELATE Freight TO Freight) AS FreightLines"
End Function

Sub SaveSmallDateTimeSmallMoneyRelationShape(cn)
    Dim rs, xmlPath, adtgPath, roundtripPath
    xmlPath = fso.BuildPath(root, "orders_products_smalldatetime_smallmoney_relation_shape.xml")
    adtgPath = fso.BuildPath(root, "orders_products_smalldatetime_smallmoney_relation_shape.adtg")
    roundtripPath = fso.BuildPath(root, "orders_products_smalldatetime_smallmoney_relation_shape.roundtrip.xml")
    DeleteIfExists xmlPath
    DeleteIfExists adtgPath
    DeleteIfExists roundtripPath

    Set rs = CreateObject("ADODB.Recordset")
    rs.CursorLocation = adUseClient
    rs.Open SmallDateTimeSmallMoneyRelationShapeQuery(), cn, adOpenStatic, adLockBatchOptimistic, adCmdText

    rs.Save xmlPath, adPersistXML
    rs.Save adtgPath, adPersistADTG
    rs.Close

    Set rs = CreateObject("ADODB.Recordset")
    rs.CursorLocation = adUseClient
    rs.Open adtgPath, "Provider=MSPersist", adOpenStatic, adLockBatchOptimistic, adCmdFile
    rs.Save roundtripPath, adPersistXML
    rs.Close

    AppendManifest Csv(Array("orders_products_smalldatetime_smallmoney_relation_shape", "sqlserver_msdatashape_smalldatetime_smallmoney_relation", "3", "2", xmlPath, adtgPath, roundtripPath))
End Sub

Function SmallDateTimeSmallMoneyRelationShapeQuery()
    SmallDateTimeSmallMoneyRelationShapeQuery = _
        "SHAPE {SELECT TOP 3 o.RequiredDate, p.UnitCost, p.ProductId FROM dbo.SalesOrders AS o INNER JOIN dbo.SalesProducts AS p ON p.ProductId = o.OrderId - 100000 ORDER BY o.OrderId} AS SmallKeys " & _
        "APPEND ({SELECT RequiredDate, OrderId, CustomerId, Priority FROM dbo.SalesOrders WHERE OrderId BETWEEN 100001 AND 100003 ORDER BY RequiredDate, OrderId} AS RequiredOrders RELATE RequiredDate TO RequiredDate) AS RequiredOrders, " & _
        "({SELECT UnitCost, ProductId, CategoryId, ProductName FROM dbo.SalesProducts WHERE ProductId BETWEEN 1 AND 3 ORDER BY UnitCost, ProductId} AS UnitProducts RELATE UnitCost TO UnitCost) AS UnitProducts"
End Function

Sub SaveTinyIntSmallIntRelationShape(cn)
    Dim rs, xmlPath, adtgPath, roundtripPath
    xmlPath = fso.BuildPath(root, "customers_orders_tinyint_smallint_relation_shape.xml")
    adtgPath = fso.BuildPath(root, "customers_orders_tinyint_smallint_relation_shape.adtg")
    roundtripPath = fso.BuildPath(root, "customers_orders_tinyint_smallint_relation_shape.roundtrip.xml")
    DeleteIfExists xmlPath
    DeleteIfExists adtgPath
    DeleteIfExists roundtripPath

    Set rs = CreateObject("ADODB.Recordset")
    rs.CursorLocation = adUseClient
    rs.Open TinyIntSmallIntRelationShapeQuery(), cn, adOpenStatic, adLockBatchOptimistic, adCmdText

    rs.Save xmlPath, adPersistXML
    rs.Save adtgPath, adPersistADTG
    rs.Close

    Set rs = CreateObject("ADODB.Recordset")
    rs.CursorLocation = adUseClient
    rs.Open adtgPath, "Provider=MSPersist", adOpenStatic, adLockBatchOptimistic, adCmdFile
    rs.Save roundtripPath, adPersistXML
    rs.Close

    AppendManifest Csv(Array("customers_orders_tinyint_smallint_relation_shape", "sqlserver_msdatashape_tinyint_smallint_relation", "3", "2", xmlPath, adtgPath, roundtripPath))
End Sub

Function TinyIntSmallIntRelationShapeQuery()
    TinyIntSmallIntRelationShapeQuery = _
        "SHAPE {SELECT RegionId, EmployeeId FROM (SELECT CONVERT(tinyint, 1) AS RegionId, CONVERT(smallint, 1) AS EmployeeId UNION ALL SELECT CONVERT(tinyint, 2), CONVERT(smallint, 2) UNION ALL SELECT CONVERT(tinyint, 3), CONVERT(smallint, 3)) AS Keys ORDER BY RegionId} AS IntegerKeys " & _
        "APPEND ({SELECT RegionId, CustomerId, CustomerCode FROM dbo.SalesCustomers WHERE RegionId IN (1, 2, 3) AND CustomerId <= 9 ORDER BY RegionId, CustomerId} AS RegionCustomers RELATE RegionId TO RegionId) AS RegionCustomers, " & _
        "({SELECT EmployeeId, OrderId, CustomerId, Priority FROM dbo.SalesOrders WHERE EmployeeId IN (1, 2, 3) AND OrderId <= 100015 ORDER BY EmployeeId, OrderId} AS EmployeeOrders RELATE EmployeeId TO EmployeeId) AS EmployeeOrders"
End Function

Sub SaveBigIntRelationShape(cn)
    Dim rs, xmlPath, adtgPath, roundtripPath
    xmlPath = fso.BuildPath(root, "lines_legacy_bigint_relation_shape.xml")
    adtgPath = fso.BuildPath(root, "lines_legacy_bigint_relation_shape.adtg")
    roundtripPath = fso.BuildPath(root, "lines_legacy_bigint_relation_shape.roundtrip.xml")
    DeleteIfExists xmlPath
    DeleteIfExists adtgPath
    DeleteIfExists roundtripPath

    Set rs = CreateObject("ADODB.Recordset")
    rs.CursorLocation = adUseClient
    rs.Open BigIntRelationShapeQuery(), cn, adOpenStatic, adLockBatchOptimistic, adCmdText

    rs.Save xmlPath, adPersistXML
    rs.Save adtgPath, adPersistADTG
    rs.Close

    Set rs = CreateObject("ADODB.Recordset")
    rs.CursorLocation = adUseClient
    rs.Open adtgPath, "Provider=MSPersist", adOpenStatic, adLockBatchOptimistic, adCmdFile
    rs.Save roundtripPath, adPersistXML
    rs.Close

    AppendManifest Csv(Array("lines_legacy_bigint_relation_shape", "sqlserver_msdatashape_bigint_relation_hidden_rowversion_suffix", "3", "1", xmlPath, adtgPath, roundtripPath))
End Sub

Function BigIntRelationShapeQuery()
    BigIntRelationShapeQuery = _
        "SHAPE {SELECT TOP 3 LineId, OrderId, LineNumber FROM dbo.SalesOrderLines WHERE OrderId = 100001 ORDER BY LineId} AS Lines " & _
        "APPEND ({SELECT LineId, LegacyDocId, LegacyCode FROM dbo.SalesLegacyDocs WHERE OrderId = 100001 ORDER BY LineId} AS Legacy RELATE LineId TO LineId) AS Legacy"
End Function

Sub SaveDecimalNumericRelationShape(cn)
    Dim rs, xmlPath, adtgPath, roundtripPath
    xmlPath = fso.BuildPath(root, "lines_decimal_numeric_relation_shape.xml")
    adtgPath = fso.BuildPath(root, "lines_decimal_numeric_relation_shape.adtg")
    roundtripPath = fso.BuildPath(root, "lines_decimal_numeric_relation_shape.roundtrip.xml")
    DeleteIfExists xmlPath
    DeleteIfExists adtgPath
    DeleteIfExists roundtripPath

    Set rs = CreateObject("ADODB.Recordset")
    rs.CursorLocation = adUseClient
    rs.Open DecimalNumericRelationShapeQuery(), cn, adOpenStatic, adLockBatchOptimistic, adCmdText

    rs.Save xmlPath, adPersistXML
    rs.Save adtgPath, adPersistADTG
    rs.Close

    Set rs = CreateObject("ADODB.Recordset")
    rs.CursorLocation = adUseClient
    rs.Open adtgPath, "Provider=MSPersist", adOpenStatic, adLockBatchOptimistic, adCmdFile
    rs.Save roundtripPath, adPersistXML
    rs.Close

    AppendManifest Csv(Array("lines_decimal_numeric_relation_shape", "sqlserver_msdatashape_decimal_numeric_relation", "3", "2", xmlPath, adtgPath, roundtripPath))
End Sub

Function DecimalNumericRelationShapeQuery()
    DecimalNumericRelationShapeQuery = _
        "SHAPE {SELECT CAST(0.0000 AS decimal(9,4)) AS DiscountRate, CAST(0.0000 AS numeric(9,4)) AS TaxRate UNION ALL SELECT CAST(0.0100 AS decimal(9,4)), CAST(0.0100 AS numeric(9,4)) UNION ALL SELECT CAST(0.0200 AS decimal(9,4)), CAST(0.0200 AS numeric(9,4))} AS Rates " & _
        "APPEND ({SELECT DiscountRate, LineId, OrderId, LineNumber, Quantity FROM (SELECT DiscountRate, LineId, OrderId, LineNumber, Quantity, ROW_NUMBER() OVER (PARTITION BY DiscountRate ORDER BY LineId) AS rn FROM dbo.SalesOrderLines WHERE DiscountRate IN (0.0000, 0.0100, 0.0200)) AS d WHERE rn <= 3 ORDER BY DiscountRate, LineId} AS DiscountLines RELATE DiscountRate TO DiscountRate) AS DiscountLines, " & _
        "({SELECT TaxRate, LineId, OrderId, LineNumber, Quantity FROM (SELECT TaxRate, LineId, OrderId, LineNumber, Quantity, ROW_NUMBER() OVER (PARTITION BY TaxRate ORDER BY LineId) AS rn FROM dbo.SalesOrderLines WHERE TaxRate IN (0.0000, 0.0100, 0.0200)) AS t WHERE rn <= 3 ORDER BY TaxRate, LineId} AS TaxLines RELATE TaxRate TO TaxRate) AS TaxLines"
End Function

Sub SaveRealFloatRelationShape(cn)
    Dim rs, xmlPath, adtgPath, roundtripPath
    xmlPath = fso.BuildPath(root, "products_real_float_relation_shape.xml")
    adtgPath = fso.BuildPath(root, "products_real_float_relation_shape.adtg")
    roundtripPath = fso.BuildPath(root, "products_real_float_relation_shape.roundtrip.xml")
    DeleteIfExists xmlPath
    DeleteIfExists adtgPath
    DeleteIfExists roundtripPath

    Set rs = CreateObject("ADODB.Recordset")
    rs.CursorLocation = adUseClient
    rs.Open RealFloatRelationShapeQuery(), cn, adOpenStatic, adLockBatchOptimistic, adCmdText

    rs.Save xmlPath, adPersistXML
    rs.Save adtgPath, adPersistADTG
    rs.Close

    Set rs = CreateObject("ADODB.Recordset")
    rs.CursorLocation = adUseClient
    rs.Open adtgPath, "Provider=MSPersist", adOpenStatic, adLockBatchOptimistic, adCmdFile
    rs.Save roundtripPath, adPersistXML
    rs.Close

    AppendManifest Csv(Array("products_real_float_relation_shape", "sqlserver_msdatashape_real_float_relation", "3", "2", xmlPath, adtgPath, roundtripPath))
End Sub

Function RealFloatRelationShapeQuery()
    RealFloatRelationShapeQuery = _
        "SHAPE {SELECT TOP 3 WeightReal, RatingFloat FROM dbo.SalesProducts ORDER BY ProductId} AS ProductKeys " & _
        "APPEND ({SELECT WeightReal, ProductId, CategoryId, ProductName FROM dbo.SalesProducts WHERE ProductId BETWEEN 1 AND 3 ORDER BY WeightReal, ProductId} AS RealProducts RELATE WeightReal TO WeightReal) AS RealProducts, " & _
        "({SELECT RatingFloat, ProductId, CategoryId, ProductName FROM dbo.SalesProducts WHERE ProductId BETWEEN 1 AND 3 ORDER BY RatingFloat, ProductId} AS FloatProducts RELATE RatingFloat TO RatingFloat) AS FloatProducts"
End Function

Sub SaveNullableChapterShape(cn)
    Dim rs, xmlPath, adtgPath, roundtripPath
    xmlPath = fso.BuildPath(root, "orders_nullable_lines_shape.xml")
    adtgPath = fso.BuildPath(root, "orders_nullable_lines_shape.adtg")
    roundtripPath = fso.BuildPath(root, "orders_nullable_lines_shape.roundtrip.xml")
    DeleteIfExists xmlPath
    DeleteIfExists adtgPath
    DeleteIfExists roundtripPath

    Set rs = CreateObject("ADODB.Recordset")
    rs.CursorLocation = adUseClient
    rs.Open NullableChapterShapeQuery(), cn, adOpenStatic, adLockBatchOptimistic, adCmdText

    rs.Save xmlPath, adPersistXML
    rs.Save adtgPath, adPersistADTG
    rs.Close

    Set rs = CreateObject("ADODB.Recordset")
    rs.CursorLocation = adUseClient
    rs.Open adtgPath, "Provider=MSPersist", adOpenStatic, adLockBatchOptimistic, adCmdFile
    rs.Save roundtripPath, adPersistXML
    rs.Close

    AppendManifest Csv(Array("orders_nullable_lines_shape", "sqlserver_msdatashape_nullable_parent_child", "5", "1", xmlPath, adtgPath, roundtripPath))
End Sub

Function NullableChapterShapeQuery()
    NullableChapterShapeQuery = _
        "SHAPE {SELECT TOP 5 o.OrderId, o.CustomerId, c.CustomerNotes FROM dbo.SalesOrders AS o INNER JOIN dbo.SalesCustomers AS c ON c.CustomerId = o.CustomerId ORDER BY o.OrderId} AS Orders " & _
        "APPEND ({SELECT OrderId, LineId, LineNumber, Quantity, LineComment FROM dbo.SalesOrderLines WHERE OrderId BETWEEN 100001 AND 100005 ORDER BY OrderId, LineNumber} AS Lines RELATE OrderId TO OrderId) AS Lines"
End Function

Sub SaveWideNullableChapterShape(cn)
    Dim rs, xmlPath, adtgPath, roundtripPath
    xmlPath = fso.BuildPath(root, "orders_wide_nullable_lines_shape.xml")
    adtgPath = fso.BuildPath(root, "orders_wide_nullable_lines_shape.adtg")
    roundtripPath = fso.BuildPath(root, "orders_wide_nullable_lines_shape.roundtrip.xml")
    DeleteIfExists xmlPath
    DeleteIfExists adtgPath
    DeleteIfExists roundtripPath

    Set rs = CreateObject("ADODB.Recordset")
    rs.CursorLocation = adUseClient
    rs.Open WideNullableChapterShapeQuery(), cn, adOpenStatic, adLockBatchOptimistic, adCmdText

    rs.Save xmlPath, adPersistXML
    rs.Save adtgPath, adPersistADTG
    rs.Close

    Set rs = CreateObject("ADODB.Recordset")
    rs.CursorLocation = adUseClient
    rs.Open adtgPath, "Provider=MSPersist", adOpenStatic, adLockBatchOptimistic, adCmdFile
    rs.Save roundtripPath, adPersistXML
    rs.Close

    AppendManifest Csv(Array("orders_wide_nullable_lines_shape", "sqlserver_msdatashape_wide_nullable_child_mask", "2", "1", xmlPath, adtgPath, roundtripPath))
End Sub

Function WideNullableChapterShapeQuery()
    WideNullableChapterShapeQuery = _
        "SHAPE {SELECT TOP 2 OrderId, CustomerId, Freight FROM dbo.SalesOrders ORDER BY OrderId} AS Orders " & _
        "APPEND ({SELECT ol.OrderId, ol.LineId, ol.LineNumber, ol.LineComment, CASE WHEN ol.LineNumber = 1 THEN NULL ELSE ol.LineComment END AS MaybeComment2, c.CustomerNotes, p.ProductDescription, pay.ReceivedAt, s.TrackingNumber, s.ShipLabel, " & _
        "CASE WHEN ol.LineNumber = 2 THEN NULL ELSE ol.UnitPrice END AS MaybeUnitPrice, CASE WHEN ol.LineNumber = 3 THEN NULL ELSE ol.DiscountRate END AS MaybeDiscount, CASE WHEN ol.LineNumber = 1 THEN NULL ELSE p.ProductGuid END AS MaybeProductGuid " & _
        "FROM dbo.SalesOrderLines AS ol INNER JOIN dbo.SalesOrders AS o ON o.OrderId = ol.OrderId INNER JOIN dbo.SalesCustomers AS c ON c.CustomerId = o.CustomerId INNER JOIN dbo.SalesProducts AS p ON p.ProductId = ol.ProductId " & _
        "INNER JOIN dbo.SalesPayments AS pay ON pay.OrderId = o.OrderId INNER JOIN dbo.SalesShipments AS s ON s.OrderId = o.OrderId WHERE ol.OrderId BETWEEN 100001 AND 100002 ORDER BY ol.OrderId, ol.LineNumber} AS Lines RELATE OrderId TO OrderId) AS Lines"
End Function

Sub SaveGuidRelationShape(cn)
    Dim rs, xmlPath, adtgPath, roundtripPath
    xmlPath = fso.BuildPath(root, "customers_orders_guid_shape.xml")
    adtgPath = fso.BuildPath(root, "customers_orders_guid_shape.adtg")
    roundtripPath = fso.BuildPath(root, "customers_orders_guid_shape.roundtrip.xml")
    DeleteIfExists xmlPath
    DeleteIfExists adtgPath
    DeleteIfExists roundtripPath

    Set rs = CreateObject("ADODB.Recordset")
    rs.CursorLocation = adUseClient
    rs.Open GuidRelationShapeQuery(), cn, adOpenStatic, adLockBatchOptimistic, adCmdText

    rs.Save xmlPath, adPersistXML
    rs.Save adtgPath, adPersistADTG
    rs.Close

    Set rs = CreateObject("ADODB.Recordset")
    rs.CursorLocation = adUseClient
    rs.Open adtgPath, "Provider=MSPersist", adOpenStatic, adLockBatchOptimistic, adCmdFile
    rs.Save roundtripPath, adPersistXML
    rs.Close

    AppendManifest Csv(Array("customers_orders_guid_shape", "sqlserver_msdatashape_guid_relation", "4", "1", xmlPath, adtgPath, roundtripPath))
End Sub

Function GuidRelationShapeQuery()
    GuidRelationShapeQuery = _
        "SHAPE {SELECT TOP 4 CustomerId, CustomerGuid, CustomerName FROM dbo.SalesCustomers ORDER BY CustomerId} AS Customers " & _
        "APPEND ({SELECT c.CustomerGuid, o.OrderId, o.Freight FROM dbo.SalesOrders AS o INNER JOIN dbo.SalesCustomers AS c ON c.CustomerId = o.CustomerId WHERE c.CustomerId BETWEEN 1 AND 4 ORDER BY c.CustomerId, o.OrderId} AS Orders RELATE CustomerGuid TO CustomerGuid) AS Orders"
End Function

Sub SaveTextRelationShape(cn)
    Dim rs, xmlPath, adtgPath, roundtripPath
    xmlPath = fso.BuildPath(root, "regions_customers_text_shape.xml")
    adtgPath = fso.BuildPath(root, "regions_customers_text_shape.adtg")
    roundtripPath = fso.BuildPath(root, "regions_customers_text_shape.roundtrip.xml")
    DeleteIfExists xmlPath
    DeleteIfExists adtgPath
    DeleteIfExists roundtripPath

    Set rs = CreateObject("ADODB.Recordset")
    rs.CursorLocation = adUseClient
    rs.Open TextRelationShapeQuery(), cn, adOpenStatic, adLockBatchOptimistic, adCmdText

    rs.Save xmlPath, adPersistXML
    rs.Save adtgPath, adPersistADTG
    rs.Close

    Set rs = CreateObject("ADODB.Recordset")
    rs.CursorLocation = adUseClient
    rs.Open adtgPath, "Provider=MSPersist", adOpenStatic, adLockBatchOptimistic, adCmdFile
    rs.Save roundtripPath, adPersistXML
    rs.Close

    AppendManifest Csv(Array("regions_customers_text_shape", "sqlserver_msdatashape_text_relation", "6", "1", xmlPath, adtgPath, roundtripPath))
End Sub

Function TextRelationShapeQuery()
    TextRelationShapeQuery = _
        "SHAPE {SELECT RegionCode, RegionName, IsDomestic FROM dbo.SalesRegions ORDER BY RegionId} AS Regions " & _
        "APPEND ({SELECT r.RegionCode, c.CustomerId, c.CustomerCode, c.CustomerName FROM dbo.SalesCustomers AS c INNER JOIN dbo.SalesRegions AS r ON r.RegionId = c.RegionId WHERE c.CustomerId <= 18 ORDER BY r.RegionId, c.CustomerId} AS Customers RELATE RegionCode TO RegionCode) AS Customers"
End Function

Sub SaveUnicodeRelationShape(cn)
    Dim rs, xmlPath, adtgPath, roundtripPath
    xmlPath = fso.BuildPath(root, "customers_orders_unicode_shape.xml")
    adtgPath = fso.BuildPath(root, "customers_orders_unicode_shape.adtg")
    roundtripPath = fso.BuildPath(root, "customers_orders_unicode_shape.roundtrip.xml")
    DeleteIfExists xmlPath
    DeleteIfExists adtgPath
    DeleteIfExists roundtripPath

    Set rs = CreateObject("ADODB.Recordset")
    rs.CursorLocation = adUseClient
    rs.Open UnicodeRelationShapeQuery(), cn, adOpenStatic, adLockBatchOptimistic, adCmdText

    rs.Save xmlPath, adPersistXML
    rs.Save adtgPath, adPersistADTG
    rs.Close

    Set rs = CreateObject("ADODB.Recordset")
    rs.CursorLocation = adUseClient
    rs.Open adtgPath, "Provider=MSPersist", adOpenStatic, adLockBatchOptimistic, adCmdFile
    rs.Save roundtripPath, adPersistXML
    rs.Close

    AppendManifest Csv(Array("customers_orders_unicode_shape", "sqlserver_msdatashape_unicode_relation", "3", "1", xmlPath, adtgPath, roundtripPath))
End Sub

Function UnicodeRelationShapeQuery()
    UnicodeRelationShapeQuery = _
        "SHAPE {SELECT TOP 3 CustomerId, CustomerName FROM dbo.SalesCustomers ORDER BY CustomerId} AS Customers " & _
        "APPEND ({SELECT c.CustomerName, o.OrderId, o.Freight FROM dbo.SalesOrders AS o INNER JOIN dbo.SalesCustomers AS c ON c.CustomerId = o.CustomerId WHERE c.CustomerId BETWEEN 1 AND 3 ORDER BY c.CustomerId, o.OrderId} AS Orders RELATE CustomerName TO CustomerName) AS Orders"
End Function

Sub SaveBinaryRelationShape(cn)
    Dim rs, xmlPath, adtgPath, roundtripPath
    xmlPath = fso.BuildPath(root, "products_lines_binary_shape.xml")
    adtgPath = fso.BuildPath(root, "products_lines_binary_shape.adtg")
    roundtripPath = fso.BuildPath(root, "products_lines_binary_shape.roundtrip.xml")
    DeleteIfExists xmlPath
    DeleteIfExists adtgPath
    DeleteIfExists roundtripPath

    Set rs = CreateObject("ADODB.Recordset")
    rs.CursorLocation = adUseClient
    rs.Open BinaryRelationShapeQuery(), cn, adOpenStatic, adLockBatchOptimistic, adCmdText

    rs.Save xmlPath, adPersistXML
    rs.Save adtgPath, adPersistADTG
    rs.Close

    Set rs = CreateObject("ADODB.Recordset")
    rs.CursorLocation = adUseClient
    rs.Open adtgPath, "Provider=MSPersist", adOpenStatic, adLockBatchOptimistic, adCmdFile
    rs.Save roundtripPath, adPersistXML
    rs.Close

    AppendManifest Csv(Array("products_lines_binary_shape", "sqlserver_msdatashape_binary_relation", "4", "1", xmlPath, adtgPath, roundtripPath))
End Sub

Function BinaryRelationShapeQuery()
    BinaryRelationShapeQuery = _
        "SHAPE {SELECT TOP 4 ProductId, ProductSku, ProductName FROM dbo.SalesProducts ORDER BY ProductId} AS Products " & _
        "APPEND ({SELECT p.ProductSku, ol.LineId, ol.OrderId, ol.Quantity FROM dbo.SalesOrderLines AS ol INNER JOIN dbo.SalesProducts AS p ON p.ProductId = ol.ProductId WHERE p.ProductId BETWEEN 1 AND 4 ORDER BY p.ProductId, ol.LineId} AS Lines RELATE ProductSku TO ProductSku) AS Lines"
End Function

Sub SaveRowVersionRelationShape(cn)
    Dim rs, xmlPath, adtgPath, roundtripPath
    xmlPath = fso.BuildPath(root, "legacy_rowversion_relation_shape.xml")
    adtgPath = fso.BuildPath(root, "legacy_rowversion_relation_shape.adtg")
    roundtripPath = fso.BuildPath(root, "legacy_rowversion_relation_shape.roundtrip.xml")
    DeleteIfExists xmlPath
    DeleteIfExists adtgPath
    DeleteIfExists roundtripPath

    Set rs = CreateObject("ADODB.Recordset")
    rs.CursorLocation = adUseClient
    rs.Open RowVersionRelationShapeQuery(), cn, adOpenStatic, adLockBatchOptimistic, adCmdText

    rs.Save xmlPath, adPersistXML
    rs.Save adtgPath, adPersistADTG
    rs.Close

    Set rs = CreateObject("ADODB.Recordset")
    rs.CursorLocation = adUseClient
    rs.Open adtgPath, "Provider=MSPersist", adOpenStatic, adLockBatchOptimistic, adCmdFile
    rs.Save roundtripPath, adPersistXML
    rs.Close

    AppendManifest Csv(Array("legacy_rowversion_relation_shape", "sqlserver_msdatashape_rowversion_relation", "3", "1", xmlPath, adtgPath, roundtripPath))
End Sub

Function RowVersionRelationShapeQuery()
    RowVersionRelationShapeQuery = _
        "SHAPE {SELECT TOP 3 LegacyRowVersion, LegacyDocId FROM dbo.SalesLegacyDocs ORDER BY LegacyDocId} AS Versions " & _
        "APPEND ({SELECT LegacyRowVersion, LegacyDocId, LineId, LegacyCode FROM dbo.SalesLegacyDocs WHERE LegacyDocId BETWEEN 1 AND 3 ORDER BY LegacyDocId} AS LegacyRows RELATE LegacyRowVersion TO LegacyRowVersion) AS LegacyRows"
End Function

Sub SaveBooleanRelationShape(cn)
    Dim rs, xmlPath, adtgPath, roundtripPath
    xmlPath = fso.BuildPath(root, "regions_customers_boolean_shape.xml")
    adtgPath = fso.BuildPath(root, "regions_customers_boolean_shape.adtg")
    roundtripPath = fso.BuildPath(root, "regions_customers_boolean_shape.roundtrip.xml")
    DeleteIfExists xmlPath
    DeleteIfExists adtgPath
    DeleteIfExists roundtripPath

    Set rs = CreateObject("ADODB.Recordset")
    rs.CursorLocation = adUseClient
    rs.Open BooleanRelationShapeQuery(), cn, adOpenStatic, adLockBatchOptimistic, adCmdText

    rs.Save xmlPath, adPersistXML
    rs.Save adtgPath, adPersistADTG
    rs.Close

    Set rs = CreateObject("ADODB.Recordset")
    rs.CursorLocation = adUseClient
    rs.Open adtgPath, "Provider=MSPersist", adOpenStatic, adLockBatchOptimistic, adCmdFile
    rs.Save roundtripPath, adPersistXML
    rs.Close

    AppendManifest Csv(Array("regions_customers_boolean_shape", "sqlserver_msdatashape_boolean_relation", "2", "1", xmlPath, adtgPath, roundtripPath))
End Sub

Function BooleanRelationShapeQuery()
    BooleanRelationShapeQuery = _
        "SHAPE {SELECT IsDomestic, Bucket FROM (SELECT CAST(1 AS bit) AS IsDomestic, CAST('Domestic' AS varchar(20)) AS Bucket UNION ALL SELECT CAST(0 AS bit), CAST('International' AS varchar(20))) AS Buckets ORDER BY IsDomestic DESC} AS Buckets " & _
        "APPEND ({SELECT r.IsDomestic, c.CustomerId, c.CustomerCode, r.RegionCode FROM dbo.SalesCustomers AS c INNER JOIN dbo.SalesRegions AS r ON r.RegionId = c.RegionId WHERE c.CustomerId <= 18 ORDER BY r.IsDomestic DESC, c.CustomerId} AS Customers RELATE IsDomestic TO IsDomestic) AS Customers"
End Function

Sub SaveNullableRelationShape(cn)
    Dim rs, xmlPath, adtgPath, roundtripPath
    xmlPath = fso.BuildPath(root, "customers_orders_nullable_key_shape.xml")
    adtgPath = fso.BuildPath(root, "customers_orders_nullable_key_shape.adtg")
    roundtripPath = fso.BuildPath(root, "customers_orders_nullable_key_shape.roundtrip.xml")
    DeleteIfExists xmlPath
    DeleteIfExists adtgPath
    DeleteIfExists roundtripPath

    Set rs = CreateObject("ADODB.Recordset")
    rs.CursorLocation = adUseClient
    rs.Open NullableRelationShapeQuery(), cn, adOpenStatic, adLockBatchOptimistic, adCmdText

    rs.Save xmlPath, adPersistXML
    rs.Save adtgPath, adPersistADTG
    rs.Close

    Set rs = CreateObject("ADODB.Recordset")
    rs.CursorLocation = adUseClient
    rs.Open adtgPath, "Provider=MSPersist", adOpenStatic, adLockBatchOptimistic, adCmdFile
    rs.Save roundtripPath, adPersistXML
    rs.Close

    AppendManifest Csv(Array("customers_orders_nullable_key_shape", "sqlserver_msdatashape_nullable_relation_key", "6", "1", xmlPath, adtgPath, roundtripPath))
End Sub

Function NullableRelationShapeQuery()
    NullableRelationShapeQuery = _
        "SHAPE {SELECT CustomerId, CASE WHEN CustomerId IN (1,3,5) THEN CustomerId ELSE NULL END AS NullableCustomerId, CustomerName FROM dbo.SalesCustomers WHERE CustomerId <= 6 ORDER BY CustomerId} AS Customers " & _
        "APPEND ({SELECT CASE WHEN c.CustomerId IN (1,3,5) THEN c.CustomerId ELSE NULL END AS NullableCustomerId, o.OrderId, o.Freight FROM dbo.SalesOrders AS o INNER JOIN dbo.SalesCustomers AS c ON c.CustomerId = o.CustomerId WHERE c.CustomerId <= 6 ORDER BY c.CustomerId, o.OrderId} AS Orders RELATE NullableCustomerId TO NullableCustomerId) AS Orders"
End Function

Sub SaveDuplicateParentRelationShape(cn)
    Dim rs, xmlPath, adtgPath, roundtripPath
    xmlPath = fso.BuildPath(root, "customers_orders_duplicate_parent_shape.xml")
    adtgPath = fso.BuildPath(root, "customers_orders_duplicate_parent_shape.adtg")
    roundtripPath = fso.BuildPath(root, "customers_orders_duplicate_parent_shape.roundtrip.xml")
    DeleteIfExists xmlPath
    DeleteIfExists adtgPath
    DeleteIfExists roundtripPath

    Set rs = CreateObject("ADODB.Recordset")
    rs.CursorLocation = adUseClient
    rs.Open DuplicateParentRelationShapeQuery(), cn, adOpenStatic, adLockBatchOptimistic, adCmdText

    rs.Save xmlPath, adPersistXML
    rs.Save adtgPath, adPersistADTG
    rs.Close

    Set rs = CreateObject("ADODB.Recordset")
    rs.CursorLocation = adUseClient
    rs.Open adtgPath, "Provider=MSPersist", adOpenStatic, adLockBatchOptimistic, adCmdFile
    rs.Save roundtripPath, adPersistXML
    rs.Close

    AppendManifest Csv(Array("customers_orders_duplicate_parent_shape", "sqlserver_msdatashape_duplicate_parent_relation_key", "6", "1", xmlPath, adtgPath, roundtripPath))
End Sub

Function DuplicateParentRelationShapeQuery()
    DuplicateParentRelationShapeQuery = _
        "SHAPE {SELECT TOP 6 r.RegionCode, c.CustomerId, c.CustomerCode FROM dbo.SalesCustomers AS c INNER JOIN dbo.SalesRegions AS r ON r.RegionId = c.RegionId ORDER BY r.RegionId, c.CustomerId} AS Customers " & _
        "APPEND ({SELECT r.RegionCode, o.OrderId, o.CustomerId, o.Freight FROM dbo.SalesOrders AS o INNER JOIN dbo.SalesCustomers AS c ON c.CustomerId = o.CustomerId INNER JOIN dbo.SalesRegions AS r ON r.RegionId = c.RegionId WHERE c.CustomerId <= 12 ORDER BY r.RegionId, o.OrderId} AS Orders RELATE RegionCode TO RegionCode) AS Orders"
End Function

Sub SaveSparseAggregateShape(cn)
    Dim rs, xmlPath, adtgPath, roundtripPath
    xmlPath = fso.BuildPath(root, "orders_sparse_aggregate_shape.xml")
    adtgPath = fso.BuildPath(root, "orders_sparse_aggregate_shape.adtg")
    roundtripPath = fso.BuildPath(root, "orders_sparse_aggregate_shape.roundtrip.xml")
    DeleteIfExists xmlPath
    DeleteIfExists adtgPath
    DeleteIfExists roundtripPath

    Set rs = CreateObject("ADODB.Recordset")
    rs.CursorLocation = adUseClient
    rs.Open SparseAggregateShapeQuery(), cn, adOpenStatic, adLockBatchOptimistic, adCmdText

    rs.Save xmlPath, adPersistXML
    rs.Save adtgPath, adPersistADTG
    rs.Close

    Set rs = CreateObject("ADODB.Recordset")
    rs.CursorLocation = adUseClient
    rs.Open adtgPath, "Provider=MSPersist", adOpenStatic, adLockBatchOptimistic, adCmdFile
    rs.Save roundtripPath, adPersistXML
    rs.Close

    AppendManifest Csv(Array("orders_sparse_aggregate_shape", "sqlserver_msdatashape_sparse_aggregates", "5", "1", xmlPath, adtgPath, roundtripPath))
End Sub

Function SparseAggregateShapeQuery()
    SparseAggregateShapeQuery = _
        "SHAPE {SELECT TOP 5 OrderId, CustomerId, Freight FROM dbo.SalesOrders ORDER BY OrderId} AS Orders " & _
        "APPEND ({SELECT OrderId, LineId, LineNumber, Quantity, CAST(Quantity AS money) AS QuantityMoney FROM dbo.SalesOrderLines WHERE OrderId IN (100001, 100003) ORDER BY OrderId, LineNumber} AS Lines RELATE OrderId TO OrderId) AS Lines, " & _
        "SUM(Lines.QuantityMoney) AS QuantitySum, AVG(Lines.Quantity) AS AvgQuantity, COUNT(Lines) AS LineRows, COUNT(Lines.Quantity) AS QuantityCount"
End Function

Sub SaveSparseChildShape(cn)
    Dim rs, xmlPath, adtgPath, roundtripPath
    xmlPath = fso.BuildPath(root, "orders_sparse_lines_shape.xml")
    adtgPath = fso.BuildPath(root, "orders_sparse_lines_shape.adtg")
    roundtripPath = fso.BuildPath(root, "orders_sparse_lines_shape.roundtrip.xml")
    DeleteIfExists xmlPath
    DeleteIfExists adtgPath
    DeleteIfExists roundtripPath

    Set rs = CreateObject("ADODB.Recordset")
    rs.CursorLocation = adUseClient
    rs.Open SparseChildShapeQuery(), cn, adOpenStatic, adLockBatchOptimistic, adCmdText

    rs.Save xmlPath, adPersistXML
    rs.Save adtgPath, adPersistADTG
    rs.Close

    Set rs = CreateObject("ADODB.Recordset")
    rs.CursorLocation = adUseClient
    rs.Open adtgPath, "Provider=MSPersist", adOpenStatic, adLockBatchOptimistic, adCmdFile
    rs.Save roundtripPath, adPersistXML
    rs.Close

    AppendManifest Csv(Array("orders_sparse_lines_shape", "sqlserver_msdatashape_sparse_child_rows", "5", "1", xmlPath, adtgPath, roundtripPath))
End Sub

Function SparseChildShapeQuery()
    SparseChildShapeQuery = _
        "SHAPE {SELECT TOP 5 OrderId, CustomerId, Freight FROM dbo.SalesOrders ORDER BY OrderId} AS Orders " & _
        "APPEND ({SELECT OrderId, LineId, Quantity FROM dbo.SalesOrderLines WHERE OrderId IN (100001, 100003) ORDER BY OrderId, LineNumber} AS Lines RELATE OrderId TO OrderId) AS Lines"
End Function

Sub SaveEmptyChildShape(cn)
    Dim rs, xmlPath, adtgPath, roundtripPath
    xmlPath = fso.BuildPath(root, "orders_empty_lines_shape.xml")
    adtgPath = fso.BuildPath(root, "orders_empty_lines_shape.adtg")
    roundtripPath = fso.BuildPath(root, "orders_empty_lines_shape.roundtrip.xml")
    DeleteIfExists xmlPath
    DeleteIfExists adtgPath
    DeleteIfExists roundtripPath

    Set rs = CreateObject("ADODB.Recordset")
    rs.CursorLocation = adUseClient
    rs.Open EmptyChildShapeQuery(), cn, adOpenStatic, adLockBatchOptimistic, adCmdText

    rs.Save xmlPath, adPersistXML
    rs.Save adtgPath, adPersistADTG
    rs.Close

    Set rs = CreateObject("ADODB.Recordset")
    rs.CursorLocation = adUseClient
    rs.Open adtgPath, "Provider=MSPersist", adOpenStatic, adLockBatchOptimistic, adCmdFile
    rs.Save roundtripPath, adPersistXML
    rs.Close

    AppendManifest Csv(Array("orders_empty_lines_shape", "sqlserver_msdatashape_empty_child_group", "3", "1", xmlPath, adtgPath, roundtripPath))
End Sub

Function EmptyChildShapeQuery()
    EmptyChildShapeQuery = _
        "SHAPE {SELECT TOP 3 OrderId, CustomerId, Freight FROM dbo.SalesOrders ORDER BY OrderId} AS Orders " & _
        "APPEND ({SELECT OrderId, LineId, Quantity FROM dbo.SalesOrderLines WHERE 1 = 0} AS Lines RELATE OrderId TO OrderId) AS Lines"
End Function

Sub SaveEmptyParentShape(cn)
    Dim rs, xmlPath, adtgPath, roundtripPath
    xmlPath = fso.BuildPath(root, "orders_empty_parent_lines_shape.xml")
    adtgPath = fso.BuildPath(root, "orders_empty_parent_lines_shape.adtg")
    roundtripPath = fso.BuildPath(root, "orders_empty_parent_lines_shape.roundtrip.xml")
    DeleteIfExists xmlPath
    DeleteIfExists adtgPath
    DeleteIfExists roundtripPath

    Set rs = CreateObject("ADODB.Recordset")
    rs.CursorLocation = adUseClient
    rs.Open EmptyParentShapeQuery(), cn, adOpenStatic, adLockBatchOptimistic, adCmdText

    rs.Save xmlPath, adPersistXML
    rs.Save adtgPath, adPersistADTG
    rs.Close

    Set rs = CreateObject("ADODB.Recordset")
    rs.CursorLocation = adUseClient
    rs.Open adtgPath, "Provider=MSPersist", adOpenStatic, adLockBatchOptimistic, adCmdFile
    rs.Save roundtripPath, adPersistXML
    rs.Close

    AppendManifest Csv(Array("orders_empty_parent_lines_shape", "sqlserver_msdatashape_empty_parent_rows", "0", "1", xmlPath, adtgPath, roundtripPath))
End Sub

Function EmptyParentShapeQuery()
    EmptyParentShapeQuery = _
        "SHAPE {SELECT TOP 0 OrderId, CustomerId, Freight FROM dbo.SalesOrders ORDER BY OrderId} AS Orders " & _
        "APPEND ({SELECT OrderId, LineId, Quantity FROM dbo.SalesOrderLines WHERE OrderId BETWEEN 100001 AND 100003 ORDER BY OrderId, LineId} AS Lines RELATE OrderId TO OrderId) AS Lines"
End Function

Sub SavePendingChangesShape(cn)
    Dim rs, child, xmlPath, adtgPath, roundtripPath
    xmlPath = fso.BuildPath(root, "orders_pending_changes_shape.xml")
    adtgPath = fso.BuildPath(root, "orders_pending_changes_shape.adtg")
    roundtripPath = fso.BuildPath(root, "orders_pending_changes_shape.roundtrip.xml")
    DeleteIfExists xmlPath
    DeleteIfExists adtgPath
    DeleteIfExists roundtripPath

    Set rs = CreateObject("ADODB.Recordset")
    rs.CursorLocation = adUseClient
    rs.Open PendingChangesShapeQuery(), cn, adOpenStatic, adLockBatchOptimistic, adCmdText

    rs.MoveFirst
    rs.Fields("Freight").Value = 123.45
    rs.Update

    Set child = rs.Fields("Lines").Value
    child.MoveFirst
    child.Fields("Quantity").Value = 777
    child.Update
    child.MoveNext
    child.Delete
    child.AddNew
    child.Fields("OrderId").Value = 100001
    child.Fields("LineId").Value = 1999999999
    child.Fields("LineNumber").Value = 99
    child.Fields("Quantity").Value = 42
    child.Update

    ' MDAC refuses XML persistence for updated hierarchical Recordsets.
    rs.Save adtgPath, adPersistADTG
    rs.Close

    AppendManifest Csv(Array("orders_pending_changes_shape", "sqlserver_msdatashape_pending_changes_adtg_only", "2", "1", "", adtgPath, ""))
End Sub

Function PendingChangesShapeQuery()
    PendingChangesShapeQuery = _
        "SHAPE {SELECT TOP 2 OrderId, CustomerId, Freight FROM dbo.SalesOrders ORDER BY OrderId} AS Orders " & _
        "APPEND ({SELECT OrderId, LineId, LineNumber, Quantity FROM dbo.SalesOrderLines WHERE OrderId BETWEEN 100001 AND 100002 ORDER BY OrderId, LineNumber} AS Lines RELATE OrderId TO OrderId) AS Lines"
End Function

Sub SaveParentInsertDeleteShape(cn)
    Dim rs, xmlPath, adtgPath, roundtripPath
    xmlPath = fso.BuildPath(root, "orders_parent_insert_delete_shape.xml")
    adtgPath = fso.BuildPath(root, "orders_parent_insert_delete_shape.adtg")
    roundtripPath = fso.BuildPath(root, "orders_parent_insert_delete_shape.roundtrip.xml")
    DeleteIfExists xmlPath
    DeleteIfExists adtgPath
    DeleteIfExists roundtripPath

    Set rs = CreateObject("ADODB.Recordset")
    rs.CursorLocation = adUseClient
    rs.Open ParentInsertDeleteShapeQuery(), cn, adOpenStatic, adLockBatchOptimistic, adCmdText

    rs.MoveFirst
    rs.Delete

    rs.AddNew
    rs.Fields("OrderId").Value = 199999
    rs.Fields("CustomerId").Value = 1
    rs.Fields("Freight").Value = CCur("77.77")
    rs.Update

    ' MDAC refuses XML persistence for updated hierarchical Recordsets.
    rs.Save adtgPath, adPersistADTG
    rs.Close

    AppendManifest Csv(Array("orders_parent_insert_delete_shape", "sqlserver_msdatashape_parent_insert_delete_adtg_only", "3", "1", "", adtgPath, ""))
End Sub

Function ParentInsertDeleteShapeQuery()
    ParentInsertDeleteShapeQuery = _
        "SHAPE {SELECT TOP 3 OrderId, CustomerId, Freight FROM dbo.SalesOrders ORDER BY OrderId} AS Orders " & _
        "APPEND ({SELECT OrderId, LineId, LineNumber, Quantity FROM dbo.SalesOrderLines WHERE OrderId BETWEEN 100001 AND 100003 ORDER BY OrderId, LineNumber} AS Lines RELATE OrderId TO OrderId) AS Lines"
End Function

Sub SaveParentRelationKeyUpdateShape(cn)
    Dim rs, xmlPath, adtgPath, roundtripPath
    xmlPath = fso.BuildPath(root, "orders_parent_relation_key_update_shape.xml")
    adtgPath = fso.BuildPath(root, "orders_parent_relation_key_update_shape.adtg")
    roundtripPath = fso.BuildPath(root, "orders_parent_relation_key_update_shape.roundtrip.xml")
    DeleteIfExists xmlPath
    DeleteIfExists adtgPath
    DeleteIfExists roundtripPath

    Set rs = CreateObject("ADODB.Recordset")
    rs.CursorLocation = adUseClient
    rs.Open RelationKeyUpdateShapeQuery(), cn, adOpenStatic, adLockBatchOptimistic, adCmdText

    rs.MoveFirst
    rs.Fields("OrderId").Value = 199998
    rs.Fields("Freight").Value = CCur("123.45")
    rs.Update

    ' MDAC refuses XML persistence for updated hierarchical Recordsets.
    rs.Save adtgPath, adPersistADTG
    rs.Close

    AppendManifest Csv(Array("orders_parent_relation_key_update_shape", "sqlserver_msdatashape_parent_relation_key_update_adtg_only", "2", "1", "", adtgPath, ""))
End Sub

Sub SaveChildRelationKeyUpdateShape(cn)
    Dim rs, child, xmlPath, adtgPath, roundtripPath
    xmlPath = fso.BuildPath(root, "orders_child_relation_key_update_shape.xml")
    adtgPath = fso.BuildPath(root, "orders_child_relation_key_update_shape.adtg")
    roundtripPath = fso.BuildPath(root, "orders_child_relation_key_update_shape.roundtrip.xml")
    DeleteIfExists xmlPath
    DeleteIfExists adtgPath
    DeleteIfExists roundtripPath

    Set rs = CreateObject("ADODB.Recordset")
    rs.CursorLocation = adUseClient
    rs.Open RelationKeyUpdateShapeQuery(), cn, adOpenStatic, adLockBatchOptimistic, adCmdText

    rs.MoveFirst
    Set child = rs.Fields("Lines").Value
    child.MoveFirst
    child.Fields("OrderId").Value = 100002
    child.Fields("Quantity").Value = child.Fields("Quantity").Value + 111
    child.Update

    ' MDAC refuses XML persistence for updated hierarchical Recordsets.
    rs.Save adtgPath, adPersistADTG
    rs.Close

    AppendManifest Csv(Array("orders_child_relation_key_update_shape", "sqlserver_msdatashape_child_relation_key_update_adtg_only", "2", "1", "", adtgPath, ""))
End Sub

Function RelationKeyUpdateShapeQuery()
    RelationKeyUpdateShapeQuery = _
        "SHAPE {SELECT TOP 2 OrderId, CustomerId, Freight FROM dbo.SalesOrders ORDER BY OrderId} AS Orders " & _
        "APPEND ({SELECT OrderId, LineId, LineNumber, Quantity FROM dbo.SalesOrderLines WHERE OrderId BETWEEN 100001 AND 100002 ORDER BY OrderId, LineNumber} AS Lines RELATE OrderId TO OrderId) AS Lines"
End Function

Sub SaveCompositeParentRelationKeyUpdateShape(cn)
    Dim rs, xmlPath, adtgPath, roundtripPath
    xmlPath = fso.BuildPath(root, "orders_composite_parent_relation_key_update_shape.xml")
    adtgPath = fso.BuildPath(root, "orders_composite_parent_relation_key_update_shape.adtg")
    roundtripPath = fso.BuildPath(root, "orders_composite_parent_relation_key_update_shape.roundtrip.xml")
    DeleteIfExists xmlPath
    DeleteIfExists adtgPath
    DeleteIfExists roundtripPath

    Set rs = CreateObject("ADODB.Recordset")
    rs.CursorLocation = adUseClient
    rs.Open CompositeRelationKeyUpdateShapeQuery(), cn, adOpenStatic, adLockBatchOptimistic, adCmdText

    rs.MoveFirst
    rs.Fields("OrderId").Value = 100002
    rs.Fields("ProductId").Value = 14
    rs.Update

    ' MDAC refuses XML persistence for updated hierarchical Recordsets.
    rs.Save adtgPath, adPersistADTG
    rs.Close

    AppendManifest Csv(Array("orders_composite_parent_relation_key_update_shape", "sqlserver_msdatashape_composite_parent_relation_key_update_adtg_only", "2", "1", "", adtgPath, ""))
End Sub

Sub SaveCompositeChildRelationKeyUpdateShape(cn)
    Dim rs, child, xmlPath, adtgPath, roundtripPath
    xmlPath = fso.BuildPath(root, "orders_composite_child_relation_key_update_shape.xml")
    adtgPath = fso.BuildPath(root, "orders_composite_child_relation_key_update_shape.adtg")
    roundtripPath = fso.BuildPath(root, "orders_composite_child_relation_key_update_shape.roundtrip.xml")
    DeleteIfExists xmlPath
    DeleteIfExists adtgPath
    DeleteIfExists roundtripPath

    Set rs = CreateObject("ADODB.Recordset")
    rs.CursorLocation = adUseClient
    rs.Open CompositeRelationKeyUpdateShapeQuery(), cn, adOpenStatic, adLockBatchOptimistic, adCmdText

    rs.MoveFirst
    Set child = rs.Fields("Lines").Value
    child.MoveFirst
    child.Fields("OrderId").Value = 100002
    child.Fields("ProductId").Value = 14
    child.Fields("Quantity").Value = child.Fields("Quantity").Value + 222
    child.Update

    ' MDAC refuses XML persistence for updated hierarchical Recordsets.
    rs.Save adtgPath, adPersistADTG
    rs.Close

    AppendManifest Csv(Array("orders_composite_child_relation_key_update_shape", "sqlserver_msdatashape_composite_child_relation_key_update_adtg_only", "2", "1", "", adtgPath, ""))
End Sub

Function CompositeRelationKeyUpdateShapeQuery()
    CompositeRelationKeyUpdateShapeQuery = _
        "SHAPE {SELECT OrderId, ProductId FROM dbo.SalesOrderLines WHERE (OrderId = 100001 AND LineNumber = 1) OR (OrderId = 100002 AND LineNumber = 1) ORDER BY OrderId} AS Parents " & _
        "APPEND ({SELECT OrderId, ProductId, LineId, LineNumber, Quantity FROM dbo.SalesOrderLines WHERE OrderId IN (100001, 100002) ORDER BY OrderId, LineNumber} AS Lines RELATE OrderId TO OrderId, ProductId TO ProductId) AS Lines"
End Function

Sub SaveNestedPendingChangesShape(cn)
    Dim rs, lines, product, xmlPath, adtgPath, roundtripPath
    xmlPath = fso.BuildPath(root, "orders_lines_product_pending_shape.xml")
    adtgPath = fso.BuildPath(root, "orders_lines_product_pending_shape.adtg")
    roundtripPath = fso.BuildPath(root, "orders_lines_product_pending_shape.roundtrip.xml")
    DeleteIfExists xmlPath
    DeleteIfExists adtgPath
    DeleteIfExists roundtripPath

    Set rs = CreateObject("ADODB.Recordset")
    rs.CursorLocation = adUseClient
    rs.Open NestedPendingChangesShapeQuery(), cn, adOpenStatic, adLockBatchOptimistic, adCmdText

    rs.MoveFirst
    Set lines = rs.Fields("Lines").Value
    lines.MoveFirst
    lines.Fields("Quantity").Value = 888
    lines.Update

    Set product = lines.Fields("Product").Value
    product.MoveFirst
    product.Fields("UnitCost").Value = 321.09
    product.Update

    lines.MoveNext
    Set product = lines.Fields("Product").Value
    product.MoveFirst
    product.Delete

    ' MDAC refuses XML persistence for updated hierarchical Recordsets.
    rs.Save adtgPath, adPersistADTG
    rs.Close

    AppendManifest Csv(Array("orders_lines_product_pending_shape", "sqlserver_msdatashape_nested_pending_changes_adtg_only", "2", "2", "", adtgPath, ""))
End Sub

Function NestedPendingChangesShapeQuery()
    NestedPendingChangesShapeQuery = _
        "SHAPE {SELECT TOP 2 OrderId, CustomerId, Freight FROM dbo.SalesOrders ORDER BY OrderId} AS Orders " & _
        "APPEND ((SHAPE {SELECT OrderId, LineId, LineNumber, ProductId, Quantity FROM dbo.SalesOrderLines WHERE OrderId BETWEEN 100001 AND 100002 ORDER BY OrderId, LineNumber} AS Lines " & _
        "APPEND ({SELECT ProductId, ProductName, UnitCost FROM dbo.SalesProducts WHERE ProductId IN (SELECT ProductId FROM dbo.SalesOrderLines WHERE OrderId BETWEEN 100001 AND 100002)} AS Product RELATE ProductId TO ProductId) AS Product) " & _
        "RELATE OrderId TO OrderId) AS Lines"
End Function

Sub AppendManifest(row)
    Dim path, prefix
    path = fso.BuildPath(root, "manifest.csv")
    If fso.FileExists(path) Then
        prefix = ""
    Else
        prefix = "case,mode,parent_rows,chapters,xml,adtg,roundtrip_xml" & vbCrLf
    End If
    AppendText path, prefix & row & vbCrLf
End Sub

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
