SET NOCOUNT ON;

IF OBJECT_ID(N'dbo.SalesLegacyDocs', N'U') IS NOT NULL DROP TABLE dbo.SalesLegacyDocs;
IF OBJECT_ID(N'dbo.SalesShipments', N'U') IS NOT NULL DROP TABLE dbo.SalesShipments;
IF OBJECT_ID(N'dbo.SalesPayments', N'U') IS NOT NULL DROP TABLE dbo.SalesPayments;
IF OBJECT_ID(N'dbo.SalesOrderLines', N'U') IS NOT NULL DROP TABLE dbo.SalesOrderLines;
IF OBJECT_ID(N'dbo.SalesOrders', N'U') IS NOT NULL DROP TABLE dbo.SalesOrders;
IF OBJECT_ID(N'dbo.SalesProducts', N'U') IS NOT NULL DROP TABLE dbo.SalesProducts;
IF OBJECT_ID(N'dbo.SalesCategories', N'U') IS NOT NULL DROP TABLE dbo.SalesCategories;
IF OBJECT_ID(N'dbo.SalesEmployees', N'U') IS NOT NULL DROP TABLE dbo.SalesEmployees;
IF OBJECT_ID(N'dbo.SalesCustomers', N'U') IS NOT NULL DROP TABLE dbo.SalesCustomers;
IF OBJECT_ID(N'dbo.SalesRegions', N'U') IS NOT NULL DROP TABLE dbo.SalesRegions;

CREATE TABLE dbo.SalesRegions (
    RegionId tinyint NOT NULL PRIMARY KEY,
    RegionCode char(3) NOT NULL,
    RegionName nvarchar(40) NOT NULL,
    TaxRate decimal(9,4) NOT NULL,
    IsDomestic bit NOT NULL
);

CREATE TABLE dbo.SalesCustomers (
    CustomerId int NOT NULL PRIMARY KEY,
    RegionId tinyint NOT NULL REFERENCES dbo.SalesRegions(RegionId),
    CustomerCode varchar(12) NOT NULL,
    CustomerName nvarchar(80) NOT NULL,
    CustomerNotes nvarchar(max) NULL,
    CreditLimit money NOT NULL,
    SignupDate datetime NOT NULL,
    IsPreferred bit NOT NULL,
    CustomerGuid uniqueidentifier NOT NULL,
    ProfileHash varbinary(16) NOT NULL
);

CREATE TABLE dbo.SalesEmployees (
    EmployeeId smallint NOT NULL PRIMARY KEY,
    RegionId tinyint NOT NULL REFERENCES dbo.SalesRegions(RegionId),
    EmployeeCode varchar(12) NOT NULL,
    EmployeeName nvarchar(80) NOT NULL,
    HireDate datetime NOT NULL,
    Quota numeric(18,4) NOT NULL,
    CommissionRate real NOT NULL
);

CREATE TABLE dbo.SalesCategories (
    CategoryId tinyint NOT NULL PRIMARY KEY,
    CategoryName nvarchar(60) NOT NULL,
    MarginTarget decimal(18,6) NOT NULL
);

CREATE TABLE dbo.SalesProducts (
    ProductId int NOT NULL PRIMARY KEY,
    CategoryId tinyint NOT NULL REFERENCES dbo.SalesCategories(CategoryId),
    ProductName nvarchar(100) NOT NULL,
    ProductGuid uniqueidentifier NOT NULL,
    ProductSku binary(8) NOT NULL,
    ProductImage varbinary(16) NOT NULL,
    ProductDescription nvarchar(max) NULL,
    UnitCost smallmoney NOT NULL,
    WeightReal real NOT NULL,
    RatingFloat float NOT NULL,
    IsDiscontinued bit NOT NULL
);

CREATE TABLE dbo.SalesOrders (
    OrderId int NOT NULL PRIMARY KEY,
    CustomerId int NOT NULL REFERENCES dbo.SalesCustomers(CustomerId),
    EmployeeId smallint NOT NULL REFERENCES dbo.SalesEmployees(EmployeeId),
    OrderDate datetime NOT NULL,
    RequiredDate smalldatetime NOT NULL,
    Freight money NOT NULL,
    Priority tinyint NOT NULL,
    OrderScore float NOT NULL,
    OrderToken uniqueidentifier NOT NULL
);

CREATE TABLE dbo.SalesOrderLines (
    LineId bigint NOT NULL PRIMARY KEY,
    OrderId int NOT NULL REFERENCES dbo.SalesOrders(OrderId),
    LineNumber smallint NOT NULL,
    ProductId int NOT NULL REFERENCES dbo.SalesProducts(ProductId),
    Quantity smallint NOT NULL,
    UnitPrice money NOT NULL,
    DiscountRate decimal(9,4) NOT NULL,
    TaxRate numeric(9,4) NOT NULL,
    LineComment varchar(160) NULL
);

CREATE TABLE dbo.SalesPayments (
    PaymentId int NOT NULL PRIMARY KEY,
    OrderId int NOT NULL REFERENCES dbo.SalesOrders(OrderId),
    PaymentMethod varchar(16) NOT NULL,
    PaymentAmount money NOT NULL,
    ReceivedAt datetime NULL,
    Approved bit NOT NULL
);

CREATE TABLE dbo.SalesShipments (
    ShipmentId int NOT NULL PRIMARY KEY,
    OrderId int NOT NULL REFERENCES dbo.SalesOrders(OrderId),
    ShippedAt datetime NULL,
    TrackingNumber varchar(32) NULL,
    ShipLabel varbinary(max) NULL,
    DeliveryWindowStart datetime NULL,
    DeliveryWindowEnd datetime NULL
);

CREATE TABLE dbo.SalesLegacyDocs (
    LegacyDocId int IDENTITY(1,1) NOT NULL PRIMARY KEY,
    OrderId int NOT NULL REFERENCES dbo.SalesOrders(OrderId),
    LineId bigint NOT NULL REFERENCES dbo.SalesOrderLines(LineId),
    LegacyCode char(6) NOT NULL,
    LegacyText text NULL,
    LegacyNText ntext NULL,
    LegacyImage image NULL,
    LegacyRowVersion rowversion NOT NULL
);

INSERT dbo.SalesRegions (RegionId, RegionCode, RegionName, TaxRate, IsDomestic) VALUES
    (1, 'SEL', N'Seoul', 0.1000, 1),
    (2, 'BUS', N'Busan', 0.0975, 1),
    (3, 'ICN', N'Incheon', 0.0950, 1),
    (4, 'TYO', N'Tokyo', 0.0825, 0),
    (5, 'SFO', N'San Francisco', 0.0875, 0),
    (6, 'BER', N'Berlin', 0.1900, 0);

DECLARE @i int = 1;
WHILE @i <= 8
BEGIN
    INSERT dbo.SalesCategories (CategoryId, CategoryName, MarginTarget)
    VALUES (
        @i,
        N'Category ' + CONVERT(nvarchar(10), @i),
        CAST((@i * 1250 + 333) AS decimal(18,6)) / CAST(10000 AS decimal(18,6))
    );
    SET @i += 1;
END;

SET @i = 1;
WHILE @i <= 12
BEGIN
    INSERT dbo.SalesEmployees (
        EmployeeId, RegionId, EmployeeCode, EmployeeName, HireDate, Quota, CommissionRate
    )
    VALUES (
        @i,
        CONVERT(tinyint, ((@i - 1) % 6) + 1),
        'EMP' + RIGHT('000' + CONVERT(varchar(3), @i), 3),
        N'Employee ' + CONVERT(nvarchar(10), @i) + N' 담당',
        DATEADD(day, @i * 37, CONVERT(datetime, '2018-01-01', 120)),
        CAST(50000 + (@i * 7777.1250) AS numeric(18,4)),
        CAST((@i % 7) AS real) / CAST(100 AS real)
    );
    SET @i += 1;
END;

SET @i = 1;
WHILE @i <= 48
BEGIN
    INSERT dbo.SalesCustomers (
        CustomerId, RegionId, CustomerCode, CustomerName, CustomerNotes,
        CreditLimit, SignupDate, IsPreferred, CustomerGuid, ProfileHash
    )
    VALUES (
        @i,
        CONVERT(tinyint, ((@i - 1) % 6) + 1),
        'CUST' + RIGHT('0000' + CONVERT(varchar(4), @i), 4),
        N'고객 ' + CONVERT(nvarchar(10), @i) + N' / Customer ' + CONVERT(nvarchar(10), @i),
        CASE WHEN @i % 5 = 0 THEN NULL ELSE N'Notes row ' + CONVERT(nvarchar(10), @i) + N' & <sales> "mixed"' END,
        CAST(10000 + (@i * 432.75) AS money),
        DATEADD(day, @i * 9, CONVERT(datetime, '2019-01-01', 120)),
        CASE WHEN @i % 3 = 0 THEN 1 ELSE 0 END,
        CONVERT(uniqueidentifier, RIGHT('00000000' + CONVERT(varchar(8), @i), 8) + '-1111-2222-3333-' + RIGHT('000000000000' + CONVERT(varchar(12), @i), 12)),
        CONVERT(varbinary(16), HASHBYTES('MD5', 'customer-' + CONVERT(varchar(10), @i)))
    );
    SET @i += 1;
END;

SET @i = 1;
WHILE @i <= 30
BEGIN
    INSERT dbo.SalesProducts (
        ProductId, CategoryId, ProductName, ProductGuid, ProductSku, ProductImage,
        ProductDescription, UnitCost, WeightReal, RatingFloat, IsDiscontinued
    )
    VALUES (
        @i,
        CONVERT(tinyint, ((@i - 1) % 8) + 1),
        N'Product ' + CONVERT(nvarchar(10), @i) + N' 혼합',
        CONVERT(uniqueidentifier, RIGHT('00000000' + CONVERT(varchar(8), @i), 8) + '-AAAA-BBBB-CCCC-' + RIGHT('000000000000' + CONVERT(varchar(12), @i), 12)),
        CONVERT(binary(8), CONVERT(varbinary(8), @i * 65537)),
        CONVERT(varbinary(16), HASHBYTES('MD5', 'product-' + CONVERT(varchar(10), @i))),
        CASE WHEN @i % 4 = 0 THEN NULL ELSE N'Long product text ' + REPLICATE(N'상품', (@i % 5) + 1) END,
        CAST(2.50 + (@i * 1.37) AS smallmoney),
        CAST(0.5 + (@i * 0.125) AS real),
        CAST(1.25 + (@i * 0.03125) AS float),
        CASE WHEN @i % 13 = 0 THEN 1 ELSE 0 END
    );
    SET @i += 1;
END;

SET @i = 1;
WHILE @i <= 240
BEGIN
    INSERT dbo.SalesOrders (
        OrderId, CustomerId, EmployeeId, OrderDate, RequiredDate, Freight,
        Priority, OrderScore, OrderToken
    )
    VALUES (
        100000 + @i,
        ((@i - 1) % 48) + 1,
        CONVERT(smallint, ((@i - 1) % 12) + 1),
        DATEADD(minute, @i * 73, CONVERT(datetime, '2024-01-01T08:00:00', 126)),
        CONVERT(smalldatetime, DATEADD(day, 3 + (@i % 9), DATEADD(minute, @i * 73, CONVERT(datetime, '2024-01-01T08:00:00', 126)))),
        CAST(5.00 + (@i % 17) * 2.35 AS money),
        CONVERT(tinyint, (@i % 5) + 1),
        CAST(@i AS float) / CAST(7 AS float),
        CONVERT(uniqueidentifier, RIGHT('00000000' + CONVERT(varchar(8), @i), 8) + '-DDDD-EEEE-FFFF-' + RIGHT('000000000000' + CONVERT(varchar(12), @i), 12))
    );
    SET @i += 1;
END;

DECLARE @orderId int = 100001;
DECLARE @lineNo int;
WHILE @orderId <= 100240
BEGIN
    SET @lineNo = 1;
    WHILE @lineNo <= 3
    BEGIN
        INSERT dbo.SalesOrderLines (
            LineId, OrderId, LineNumber, ProductId, Quantity, UnitPrice,
            DiscountRate, TaxRate, LineComment
        )
        VALUES (
            CONVERT(bigint, @orderId) * 10 + @lineNo,
            @orderId,
            CONVERT(smallint, @lineNo),
            ((@orderId + @lineNo) % 30) + 1,
            CONVERT(smallint, ((@orderId + @lineNo) % 9) + 1),
            CAST(10.00 + (((@orderId + @lineNo) % 37) * 3.21) AS money),
            CAST(((@orderId + @lineNo) % 6) AS decimal(9,4)) / CAST(100 AS decimal(9,4)),
            CAST(((@orderId + @lineNo) % 11) AS numeric(9,4)) / CAST(100 AS numeric(9,4)),
            CASE WHEN (@orderId + @lineNo) % 7 = 0 THEN NULL ELSE 'line comment ' + CONVERT(varchar(20), @orderId) + '-' + CONVERT(varchar(4), @lineNo) END
        );
        SET @lineNo += 1;
    END;
    SET @orderId += 1;
END;

INSERT dbo.SalesPayments (PaymentId, OrderId, PaymentMethod, PaymentAmount, ReceivedAt, Approved)
SELECT
    OrderId - 100000,
    OrderId,
    CASE OrderId % 4 WHEN 0 THEN 'card' WHEN 1 THEN 'wire' WHEN 2 THEN 'cash' ELSE 'coupon' END,
    CAST(SUM(CAST(Quantity AS decimal(19,4)) * CAST(UnitPrice AS decimal(19,4)) * (1 - DiscountRate) * (1 + TaxRate)) AS money),
    CASE WHEN OrderId % 10 = 0 THEN NULL ELSE DATEADD(hour, OrderId % 48, MIN(CONVERT(datetime, '2024-01-01T00:00:00', 126))) END,
    CASE WHEN OrderId % 11 = 0 THEN 0 ELSE 1 END
FROM dbo.SalesOrderLines
GROUP BY OrderId;

INSERT dbo.SalesShipments (
    ShipmentId, OrderId, ShippedAt, TrackingNumber, ShipLabel, DeliveryWindowStart, DeliveryWindowEnd
)
SELECT
    OrderId - 100000,
    OrderId,
    CASE WHEN OrderId % 8 = 0 THEN NULL ELSE DATEADD(hour, OrderId % 72, CONVERT(datetime, '2024-01-04T09:00:00', 126)) END,
    CASE WHEN OrderId % 8 = 0 THEN NULL ELSE 'TRK' + CONVERT(varchar(20), OrderId) END,
    CASE WHEN OrderId % 9 = 0 THEN NULL ELSE CONVERT(varbinary(max), HASHBYTES('SHA1', 'ship-' + CONVERT(varchar(20), OrderId))) END,
    DATEADD(hour, OrderId % 48, CONVERT(datetime, '2024-01-05T08:00:00', 126)),
    DATEADD(hour, (OrderId % 48) + 4, CONVERT(datetime, '2024-01-05T08:00:00', 126))
FROM dbo.SalesOrders;

INSERT dbo.SalesLegacyDocs (
    OrderId, LineId, LegacyCode, LegacyText, LegacyNText, LegacyImage
)
SELECT TOP 12
    ol.OrderId,
    ol.LineId,
    'LG' + RIGHT('0000' + CONVERT(varchar(4), ROW_NUMBER() OVER (ORDER BY ol.OrderId, ol.LineNumber)), 4),
    CASE
        WHEN ol.LineNumber = 2 THEN NULL
        ELSE CONVERT(varchar(8000), 'legacy text <doc> & line ' + CONVERT(varchar(20), ol.LineId))
    END,
    CASE
        WHEN ol.LineNumber = 3 THEN NULL
        ELSE CONVERT(nvarchar(4000), N'레거시 문서 ' + CONVERT(nvarchar(20), ol.LineId))
    END,
    CASE
        WHEN ol.LineNumber = 1 THEN NULL
        ELSE CONVERT(varbinary(max), HASHBYTES('SHA1', 'legacy-image-' + CONVERT(varchar(20), ol.LineId)))
    END
FROM dbo.SalesOrderLines AS ol
ORDER BY ol.OrderId, ol.LineNumber;
