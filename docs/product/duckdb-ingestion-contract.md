# DuckDB / SwanLake 全量接入合同

本文记录平台源码已经实现的 DuckDB 数据链边界。它是服务器 POC 的验收输入，不代表真实环境已经通过。

## 数据源配置

`db_type=DuckDB` 表示由 SwanLake 托管的 DuckDB/DuckLake 数据平面：

- `host`：SwanLake Flight SQL 主机或 `http(s)://` 地址；
- `port`：Flight SQL 端口，默认 UI 值为 `4214`；
- `database_name`：已经由 SwanLake 初始化并可见的 DuckDB/DuckLake catalog；
- `username/password`：当前 SwanLake 社区服务端尚无租户认证协议，因此 DuckDB 数据源不收集这两个字段。

平台的控制面仍使用本地 SQLite 或生产 PostgreSQL。DuckDB catalog 不保存控制面用户、角色和任务定义。

## 支持矩阵

| 源端 | 目标端 | 执行适配器 | 当前模式 |
|---|---|---|---|
| MySQL | MySQL / PostgreSQL | ape-dts | full |
| PostgreSQL | MySQL / PostgreSQL | ape-dts | full |
| MySQL / PostgreSQL | DuckDB / DuckLake | SwanLake Flight SQL | full |
| 同一 SwanLake 端点内的 DuckDB catalog | DuckDB / DuckLake | SwanLake Flight SQL | full |

DuckDB 作为源端时，源端和目标端必须指向同一个 SwanLake Flight SQL 端点；跨端点 Flight 数据搬运尚未实现。SQLite 只作为本地控制面测试库，不进入任务数据平面。

## 一次运行的语义

1. 调度器在控制面先创建 `PipelineRun`，再提交引擎，避免出现没有运行记录的外部写入。
2. SwanLake 在同一 Flight SQL 会话中附加 MySQL/PostgreSQL 源库；连接串仅在受控服务链内构造，错误和进度不会返回连接串或密码。
3. 每张源表先物化为会话临时表，使后续分页读取基于固定快照。
4. 引擎计算全局行数，按任务 `batch_size` 执行 `INSERT ... SELECT ... LIMIT/OFFSET`，并回传当前表、行数和百分比。
5. 所有映射表在一个目标事务中建 schema/表、清空并写入。失败、进度接收端断开或在提交前收到取消时执行回滚。
6. 事务提交后发送 `completed` 终态；任务日志只记录稳定的分类信息和脱敏摘要。

目标表不存在时按快照结构创建。目标表已存在时保留其结构，字段不兼容会导致事务失败和回滚。本合同不包含 ALTER、字段映射表达式、CDC、增量位点或跨节点 MPP。

## 服务器 POC 前置条件

- SwanLake `dev` 构建必须包含并加载 `ducklake`、`postgres` 和 `mysql` 扩展；
- `DUCKLAKE_INIT_SQL` 必须附加持久化目标 catalog，不能把正式表写入会话内存默认库；
- SwanLake 与源库之间网络可达，目标 catalog 对运行账号可写；
- 首轮 POC 使用小表验证建表、重跑替换、失败回滚、取消回滚、表目录发现和运行进度；
- 再使用有代表性的宽表、大表和类型边界验证精度、时区、DECIMAL、JSON/BLOB 及资源限制。
