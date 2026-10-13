# 真实元数据采集合同

本文定义本产品的实际采集行为与内部来源证明，不假定购买原型提供了相同数据结构。采集是专属业务流程，不通过通用资源 CRUD 伪造采集完成。平台服务保持停止，真实 SwanLake 全流程在整体 POC 阶段验证。

## 来源和范围

采集定义为 `metadata_collection`，属性 `source_type` 必须明确填写：

- `swanlake`：只读取当前租户绑定的实际 SwanLake 查询服务。不能同时填写任意 `datasource_id`，也不能从请求注入 `engine_binding`。
- `datasource`：预留真实驱动采集路径。当前没有专属数据源元数据驱动，执行明确返回错误，不把配置保存或另一台 SwanLake 的结果当作该数据源采集成功。

`catalog_filter/schema_filter/table_filter` 是单个精确名称；空、NULL 或纯空白表示不限定该维度。非空名称保留原值，不当作 glob、正则或逗号分隔清单，长度最多 256 UTF-8 字节且无控制字符。`schedule` 可保存配置，本轮不宣称周期采集已注册到调度器。

```json
{
  "source_type": "swanlake",
  "catalog_filter": "lake",
  "schema_filter": "main",
  "table_filter": "orders"
}
```

采集只运行一条固定 SELECT：将 `information_schema.tables` 与 `information_schema.columns` 按 catalog/schema/table LEFT JOIN，返回真实表类型、字段名、类型、可空性和位置。只读取 BASE TABLE/VIEW，按真实身份与字段位置排序。筛选通过类型化 SQL AST 的文本或 NULL 字面值绑定，沿用任务参数绑定的往返校验；没有用户可提供任意采集 SQL、外部路径或函数的入口。

没有用于“只取前若干条”的 SQL LIMIT。查询服务完整消费结果流；超过现有 10,000 行、JSON/Arrow 容量或超时边界会失败。采集另限最多 1,000 个表、10,000 个字段与 8 MiB 类型化结构。超限不能应用部分结果，也不能依据部分结果将其余表标记缺失。扩大规模时需要增加有明确完整性证明的分批采集协议，不能简单截断。

## 真实运行接口

`MetadataService::enqueue(actor,id,MetadataRunRequest{expected_version,idempotency_key})` 由元数据采集的专属 `execute` 产品动作调用，返回持久化 `EnqueuedProductJob`。操作人来自当前认证身份。定义必须为同租户当前版本，状态为 draft 或 published；inactive 拒绝执行。提交同时要求实际 `metadata:read`、`metadata:execute` 和同租户查询引擎绑定。

提交响应仅表示 queued。运行列表、详情、取消、重试复用 `/api/v1/product/jobs` 的真实接口，`source_kind=metadata_collection`，使用元数据域读取与执行权限。校验返回 `executed=false/schedule_registered=false`，不会调用分析引擎或构造采集统计。

运行快照冻结 format version、executor、tenant、来源 kind/id/version、完整采集属性、精确 scope、提交者与动作。只有一个 `collect` 节点，类型为 `metadata_information_schema`，节点定义只包含固定 scope，不接受任意 SQL。实际执行会重新编译快照，并比较实际节点定义与快照，不静默采用后来编辑的范围。

## 原子反射

`MetadataCatalogRepo::apply_snapshot` 只供执行器调用，不暴露给前端。输入含类型化表/字段快照、当前采集定义/版本、操作人、job/node 及 `(worker_id,fence)`。同一 SQLite IMMEDIATE 或 PostgreSQL 事务中执行：

1. 确认租户和操作人仍 active，实际精确元数据 read/execute 权限仍存在，采集定义仍是冻结的版本与属性。
2. 锁定并确认 job/node 正在运行，租约未过期，fence 相符，没有取消请求，scope 与固定快照一致。
3. 按唯一身份 `(tenant,engine,catalog,schema,table)` 创建或更新 `metadata_object` 及内部字段快照。
4. 只对完整采集 scope 内未再出现的已发现对象标记 `discovery_status=missing`、状态 inactive，不删除历史结构与引用。
5. 写入仅包含身份、版本、计数和事件类型的脱敏审计，同时将实际采集节点的结果与 succeeded 状态提交。

对象反射、内部证明、审计及节点成功是同一个事务；任何失败全部回滚。提交末尾再次检查真实数据库时钟的租约与取消状态，过期或旧 fence 不得留下半套目录。完整空结果可以将该 scope 内对象标记 missing；引擎失败或不完整结果不能这么做。

内部 `metadata_catalog` 维护实际对象 ID/版本、真实 catalog/schema/table、表类型、字段快照、SHA-256 结构指纹、present/missing 与最后观察信息。结构指纹依据类型化真实身份、表类型与按位置排序的字段结构计算。重复相同快照不会新增对象，也不增加对象版本；物理结构、采集维护字段或发现状态真正变化时才递增版本。用户的非采集属性与说明保留。观察时间更新放在内部目录中，避免每次相同采集都制造新版本。

由于节点提交与反射共用事务，执行器在反射后中断时，接管者能观察已成功节点，不会把它重新当作未提交运行。运行总体终态随后由持有有效租约的执行器完成。取消在反射事务提交之前阻止应用；已经提交的目录更新保留真实成功节点和审计，后来的取消不能撤销已提交业务事实。

## 内部来源证明

只有采集仓储构造对象属性：

```json
{
  "source_type": "swanlake",
  "table_catalog": "lake",
  "schema_name": "main",
  "table_name": "orders",
  "object_type": "table",
  "columns": [{"name":"id","data_type":"BIGINT","nullable":false,"ordinal_position":1}],
  "source_fingerprint": "由真实结构生成的64位十六进制指纹",
  "discovery_status": "present",
  "engine_binding": {"engine":"swanlake","tenant_id":"实际租户"}
}
```

`verified_binding(tenant,object_id,object_version)` 必须查到内部目录，当前资源版本相符且 published、目录 present、定位与绑定属性匹配、字段与内部快照一致、重新计算的指纹一致，才返回真实 `MetadataEngineBinding`。前端手工新建相同属性、伪造 engine_binding、跨租户查询、旧版本或手改字段，都不能凭这些字段获得来源证明。

质量服务提交时与执行器执行前、续期和结果提交前都重新查询内部证明，绑定真实 `catalog.schema.table`，不会将附加数据库中同名的另一张表当作目标。质量节点仍冻结元数据和规则版本；物理 schema 漂移导致真实查询错误，不能静默切换定义。

## 结果和已验证范围

运行保存实际反射概要：`collected_tables/collected_columns/created_objects/updated_objects/unchanged_objects/missing_objects/scope/reflection_committed`。表和字段结构保存在元数据目录，而不是把整个 information_schema 查询明细塞进运行结果预算；这个区别不会丢弃结构或把截断查询标记成功。

新增测试覆盖精确筛选的单引号安全、真实列分组与位置排序、重复/不完整结果拒绝、目录身份与结构指纹、重复采集、变更版本、精确范围内 missing、引擎失败不应用结果。SQLite 仓储测试还以实际事务故障验证回滚，并验证取消、过期租约、旧 fence、操作人失效、权限撤回、定义版本变化与伪造来源证明不能提交。主代理使用已有工具链串行执行后，4 项单元测试、4 项 SQLite 仓储测试和 3 项服务测试已全部通过。真实 SwanLake POC 尚未运行。
