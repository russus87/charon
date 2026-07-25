-- Seed di prova per Charon su SQL Server. Applicato da run-mssql-test.sh via
-- sqlcmd DOPO l'avvio del container (l'immagine mssql non ha un initdb.d).
--
-- Crea charon_src (schema + dati) e charon_full (vuoto, destinazione del clone
-- "pieno" DROP+CREATE+dati). Stessi dati del seed Postgres, per coerenza.

IF DB_ID('charon_src') IS NULL CREATE DATABASE charon_src;
IF DB_ID('charon_full') IS NULL CREATE DATABASE charon_full;
GO

USE charon_src;
GO

IF OBJECT_ID('dbo.orders', 'U') IS NOT NULL DROP TABLE dbo.orders;
IF OBJECT_ID('dbo.customers', 'U') IS NOT NULL DROP TABLE dbo.customers;
GO

CREATE TABLE dbo.customers (
    id    INT IDENTITY(1,1) PRIMARY KEY,
    name  NVARCHAR(200) NOT NULL,
    email NVARCHAR(200)
);
CREATE TABLE dbo.orders (
    id          INT IDENTITY(1,1) PRIMARY KEY,
    customer_id INT NOT NULL,
    total       DECIMAL(10,2) NOT NULL
);
GO

INSERT INTO dbo.customers (name, email) VALUES
    (N'Alice Rossi',   N'alice.rossi@example.com'),
    (N'Bruno Bianchi', N'bruno.bianchi@example.com'),
    (N'Carla Verdi',   N'carla.verdi@example.com');

INSERT INTO dbo.orders (customer_id, total) VALUES
    (1, 19.90),
    (1, 5.00),
    (2, 120.00),
    (3, 0.99);
GO
