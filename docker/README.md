# 本地集成示例

此 Compose 用于本地集成验证，包含演示数据库账号。启动前准备兼容的 Linux `ape-dts` 文件，见 [bin/README.md](bin/README.md)。本轮只做配置解析检查，没有在当前电脑构建镜像或启动这些服务。

空控制面数据库首次启动需设置 `BOOTSTRAP_ADMIN_PASSWORD`（至少 12 字节）。默认会话使用内存存储，控制面重启后需要重新登录。镜像已启用 `auth-redis` 编译选项；若连接已有 Redis，可设置 `AUTH_STORAGE=redis` 和 `AUTH_REDIS_URL`，无需为单进程部署额外启动 Redis。

PostgreSQL 的 `dts_platform` 库保存用户、角色和连接凭据等控制面元数据；`warehouse` 库保存同步目标的演示业务表。SwanLake 使用仅能读取业务表的 `dts_query` 账号连接 `warehouse`，不会直接挂载控制面元数据库。演示同步目标应配置数据库 `warehouse`，用户名 `dts_sync`、密码 `dts_sync_local_demo`。这些账号和密码需在实际部署时替换。

`postgres/init.sql` 只会在 PostgreSQL 数据卷首次初始化时执行。已有数据卷不会自动获得新增数据库、角色和授权；应先备份并核对现有数据，再按该 SQL 执行受控迁移。不要为了应用这份示例清空已有数据卷。

控制面与业务数据的隔离不代替业务租户的数据授权。当前一个 SwanLake 端点只绑定一个指定租户；多个租户的分析数据应使用独立端点或后续实现并验收的数据访问授权模块。

单节点同步适配目前只支持 MySQL/PostgreSQL 全量追加，目标表需要预先存在。DuckDB sink、CDC、幂等重跑与崩溃恢复属于后续交付范围，见 [产品交付表](../docs/product/axure-delivery-map.md)。
