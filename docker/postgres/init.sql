-- PostgreSQL Target Init
-- 创建与 MySQL 源表结构一致的目标表（ape-dts 同步目标）
-- DTS Platform 元数据表由 dts-server 在 dts_platform 自动迁移创建。
-- 以下账号与密码仅用于本地演示；业务查询不连接控制面元数据库。
CREATE ROLE dts_sync LOGIN PASSWORD 'dts_sync_local_demo';
CREATE ROLE dts_query LOGIN PASSWORD 'dts_query_local_demo';
CREATE DATABASE warehouse OWNER dts_sync;
REVOKE CONNECT ON DATABASE dts_platform FROM PUBLIC;
\connect warehouse
REVOKE ALL ON DATABASE warehouse FROM PUBLIC;
GRANT CONNECT ON DATABASE warehouse TO dts_sync, dts_query;

-- 同步目标表 (ape-dts 会将 MySQL 数据同步到这里)
CREATE TABLE IF NOT EXISTS customers (
    id SERIAL PRIMARY KEY,
    name VARCHAR(100) NOT NULL,
    email VARCHAR(200) NOT NULL,
    phone VARCHAR(30),
    city VARCHAR(50),
    created_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS orders (
    id SERIAL PRIMARY KEY,
    customer_id INT NOT NULL,
    product_name VARCHAR(200) NOT NULL,
    quantity INT NOT NULL DEFAULT 1,
    amount DECIMAL(12, 2) NOT NULL,
    status VARCHAR(20) NOT NULL DEFAULT 'pending',
    order_date TIMESTAMPTZ DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS products (
    id SERIAL PRIMARY KEY,
    sku VARCHAR(50) NOT NULL UNIQUE,
    name VARCHAR(200) NOT NULL,
    category VARCHAR(50),
    price DECIMAL(10, 2) NOT NULL DEFAULT 0,
    stock INT NOT NULL DEFAULT 0,
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

GRANT USAGE ON SCHEMA public TO dts_sync, dts_query;
GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA public TO dts_sync;
GRANT USAGE, SELECT ON ALL SEQUENCES IN SCHEMA public TO dts_sync;
GRANT SELECT ON ALL TABLES IN SCHEMA public TO dts_query;
ALTER DEFAULT PRIVILEGES FOR ROLE dts_sync IN SCHEMA public
    GRANT SELECT ON TABLES TO dts_query;
