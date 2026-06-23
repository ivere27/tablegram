SET NOCOUNT ON;

SELECT
    o.OrderId AS ORDER_ID,
    ol.LineId AS LINE_ID,
    ol.LineNumber AS LINE_NO,
    r.RegionCode AS REGION_CODE,
    r.RegionName AS REGION_NAME,
    r.IsDomestic AS REGION_DOMESTIC,
    c.CustomerCode AS CUSTOMER_CODE,
    c.CustomerName AS CUSTOMER_NAME,
    c.CustomerNotes AS CUSTOMER_NOTES,
    c.CreditLimit AS CUSTOMER_CREDIT_LIMIT,
    c.SignupDate AS CUSTOMER_SIGNUP_DATE,
    c.IsPreferred AS CUSTOMER_PREFERRED,
    c.CustomerGuid AS CUSTOMER_GUID,
    c.ProfileHash AS CUSTOMER_PROFILE_HASH,
    e.EmployeeCode AS EMPLOYEE_CODE,
    e.EmployeeName AS EMPLOYEE_NAME,
    e.HireDate AS EMPLOYEE_HIRE_DATE,
    e.Quota AS EMPLOYEE_QUOTA,
    e.CommissionRate AS EMPLOYEE_COMMISSION_RATE,
    cat.CategoryName AS CATEGORY_NAME,
    cat.MarginTarget AS CATEGORY_MARGIN_TARGET,
    p.ProductName AS PRODUCT_NAME,
    p.ProductGuid AS PRODUCT_GUID,
    p.ProductSku AS PRODUCT_SKU,
    p.ProductImage AS PRODUCT_IMAGE,
    p.ProductDescription AS PRODUCT_DESCRIPTION,
    p.UnitCost AS PRODUCT_UNIT_COST,
    p.WeightReal AS PRODUCT_WEIGHT_REAL,
    p.RatingFloat AS PRODUCT_RATING_FLOAT,
    p.IsDiscontinued AS PRODUCT_DISCONTINUED,
    o.OrderDate AS ORDER_DATE,
    CAST(o.OrderDate AS date) AS ORDER_DATE_ONLY,
    CAST(o.OrderDate AS time(0)) AS ORDER_TIME_ONLY,
    CAST(o.OrderDate AS datetime2(3)) AS ORDER_DATETIME2,
    o.RequiredDate AS REQUIRED_DATE,
    o.Freight AS FREIGHT,
    o.Priority AS PRIORITY_TINYINT,
    o.OrderScore AS ORDER_SCORE,
    o.OrderToken AS ORDER_TOKEN,
    ol.Quantity AS QUANTITY,
    ol.UnitPrice AS UNIT_PRICE,
    ol.DiscountRate AS DISCOUNT_RATE,
    ol.TaxRate AS TAX_RATE,
    CAST(CAST(ol.Quantity AS decimal(19,4)) * CAST(ol.UnitPrice AS decimal(19,4)) * (1 - ol.DiscountRate) AS decimal(19,4)) AS LINE_NET,
    ol.LineComment AS LINE_COMMENT,
    pay.PaymentMethod AS PAYMENT_METHOD,
    pay.PaymentAmount AS PAYMENT_AMOUNT,
    pay.ReceivedAt AS PAYMENT_RECEIVED_AT,
    pay.Approved AS PAYMENT_APPROVED,
    sh.ShippedAt AS SHIPPED_AT,
    sh.TrackingNumber AS TRACKING_NUMBER,
    sh.ShipLabel AS SHIP_LABEL,
    sh.DeliveryWindowStart AS DELIVERY_WINDOW_START,
    sh.DeliveryWindowEnd AS DELIVERY_WINDOW_END,
    CAST(CASE WHEN sh.ShippedAt IS NULL THEN 0 ELSE 1 END AS bit) AS HAS_SHIPPED,
    CAST(ol.LineId * 100000 AS bigint) AS BIG_SEQUENCE,
    CAST(ol.LineNumber AS smallint) AS SMALL_RANK,
    CAST(CAST(ol.Quantity AS decimal(18,4)) / CAST(NULLIF(o.Priority, 0) AS decimal(18,4)) AS numeric(18,4)) AS RATIO_NUMERIC
FROM dbo.SalesOrders AS o
INNER JOIN dbo.SalesCustomers AS c ON c.CustomerId = o.CustomerId
INNER JOIN dbo.SalesRegions AS r ON r.RegionId = c.RegionId
INNER JOIN dbo.SalesEmployees AS e ON e.EmployeeId = o.EmployeeId
INNER JOIN dbo.SalesOrderLines AS ol ON ol.OrderId = o.OrderId
INNER JOIN dbo.SalesProducts AS p ON p.ProductId = ol.ProductId
INNER JOIN dbo.SalesCategories AS cat ON cat.CategoryId = p.CategoryId
INNER JOIN dbo.SalesPayments AS pay ON pay.OrderId = o.OrderId
LEFT JOIN dbo.SalesShipments AS sh ON sh.OrderId = o.OrderId
ORDER BY o.OrderId, ol.LineNumber;
