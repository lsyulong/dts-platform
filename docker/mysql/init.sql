-- MySQL Source Test Data
-- 创建示例源数据库表和测试数据，供 ape-dts 同步到 PostgreSQL

CREATE TABLE IF NOT EXISTS customers (
    id INT AUTO_INCREMENT PRIMARY KEY,
    name VARCHAR(100) NOT NULL,
    email VARCHAR(200) NOT NULL,
    phone VARCHAR(30),
    city VARCHAR(50),
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE TABLE IF NOT EXISTS orders (
    id INT AUTO_INCREMENT PRIMARY KEY,
    customer_id INT NOT NULL,
    product_name VARCHAR(200) NOT NULL,
    quantity INT NOT NULL DEFAULT 1,
    amount DECIMAL(12, 2) NOT NULL,
    status VARCHAR(20) NOT NULL DEFAULT 'pending',
    order_date TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (customer_id) REFERENCES customers(id)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE TABLE IF NOT EXISTS products (
    id INT AUTO_INCREMENT PRIMARY KEY,
    sku VARCHAR(50) NOT NULL UNIQUE,
    name VARCHAR(200) NOT NULL,
    category VARCHAR(50),
    price DECIMAL(10, 2) NOT NULL DEFAULT 0,
    stock INT NOT NULL DEFAULT 0,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

-- 插入测试数据
INSERT INTO customers (name, email, phone, city) VALUES
    ('张三', 'zhangsan@example.com', '13800001111', '北京'),
    ('李四', 'lisi@example.com', '13800002222', '上海'),
    ('王五', 'wangwu@example.com', '13800003333', '深圳'),
    ('赵六', 'zhaoliu@example.com', '13800004444', '杭州'),
    ('孙七', 'sunqi@example.com', '13800005555', '成都');

INSERT INTO products (sku, name, category, price, stock) VALUES
    ('SKU-001', '机械键盘 K100', '电子产品', 399.00, 150),
    ('SKU-002', '无线鼠标 M20', '电子产品', 129.00, 300),
    ('SKU-003', '27寸 4K 显示器', '电子产品', 2999.00, 50),
    ('SKU-004', '人体工学椅 Pro', '办公家具', 1899.00, 30),
    ('SKU-005', 'USB-C 扩展坞', '电子产品', 259.00, 200);

INSERT INTO orders (customer_id, product_name, quantity, amount, status) VALUES
    (1, '机械键盘 K100', 1, 399.00, 'completed'),
    (1, '无线鼠标 M20', 2, 258.00, 'completed'),
    (2, '27寸 4K 显示器', 1, 2999.00, 'processing'),
    (3, '人体工学椅 Pro', 1, 1899.00, 'pending'),
    (3, 'USB-C 扩展坞', 3, 777.00, 'completed'),
    (4, '机械键盘 K100', 1, 399.00, 'shipped');
