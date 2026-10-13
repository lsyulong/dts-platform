# 只读开发任务与质量任务运行合同

本文是本产品已实现接口的实施合同，不代表已从购买的原型恢复出相同的内部数据结构。SQL 文档、开发工作流与质量任务使用真实持久化运行记录；定义状态与运行状态分别管理。全平台 POC 在业务模块建设后进行，本轮没有启动平台或分析引擎。

## 定义、发布与运行的区别

`sql_document` 将 SQL 和参数声明保存在资源属性中。`development_workflow` 将节点、依赖及全局参数声明保存在 `properties.definition`。SQL 文档和开发工作流发布前校验所有节点的 SQL AST、参数名与依赖；发布使用资源版本的乐观锁，将定义状态改为 `published`。

发布响应明确返回 `publication_scope="validated_read_only_definition"`、`executed=false` 和 `schedule_registered=false`。发布不证明物理表存在、分析引擎在线或查询成功，也不注册 JiaScheduler 周期任务。当前周期、时区字段是保存的配置，注册与撤回真实周期任务仍需独立调度适配。

提交运行返回 HTTP 202 和数据库中的 `queued` 运行。执行器领取任务、真实调用租户绑定的 SwanLake 查询服务，只有观察到真实结果并成功持久化后，节点才可变为 `succeeded`。没有模拟进度、随机统计或用户手动创建成功运行的接口。

SQL 文档与开发工作流只接受单条只读查询节点。DML、DDL、外部文件读取及受限函数由现有只读 SQL 校验器拒绝。物理模型发布、ETL 写节点、CDC 与多引擎写事务不属于这个执行器的已完成能力。

## 类型化参数

SQL 使用表达式中的 `$name` 占位符，禁止占位符替代表名。参数最多 32 个，名称为 ASCII 字母开头的字母、数字和下划线，最多 80 字节。声明字段如下；未声明输入、重复声明、任何节点都未使用的声明都会被拒绝。

```json
{
  "sql": "SELECT customer_id FROM customers WHERE customer_id = $customer_id AND enabled = $enabled",
  "parameters": [
    {"name": "customer_id", "data_type": "BIGINT", "required": true},
    {"name": "enabled", "data_type": "BOOLEAN", "required": true, "default_value": true}
  ]
}
```

| 声明类型 | 运行输入 | 约束 |
| --- | --- | --- |
| `VARCHAR` | JSON 字符串 | 最多 8192 UTF-8 字节，无控制字符 |
| `BIGINT` | JSON 整数 | 有符号 64 位范围，数字字符串不隐式转换 |
| `DOUBLE` | JSON 数字 | 有限数值 |
| `BOOLEAN` | JSON 布尔值 | `false` 与缺省值不同 |
| `DATE` | JSON 字符串 | 有效 `YYYY-MM-DD` |
| `TIMESTAMP` | JSON 字符串 | 有效日期时间；带 RFC3339 时区的输入归一为 UTC 无时区时间 |

`required=true` 不允许缺失或 `null`。可选参数缺失时使用 `default_value`；无默认值时为 `null`。显式 `default_value=null` 与没有默认值在声明中分别保留，`0` 与 `false` 不当作缺省输入。

绑定将值构造成类型化 SQL AST 字面值，再替换 AST 占位符。没有将运行文本拼接进 SQL 模板的路径。输出 SQL 再次进行只读校验和解析；所有字符串字面值必须与绑定前的逻辑字符串逐项一致，否则拒绝执行。针对当前固定版本 SQL 打印器的单引号行为，字符串先按双单引号规则编码，再验证往返值，避免成对引号或反斜线改变查询含义。

参数对象最大 16 KiB。凭据类参数名、Bearer/JWT/私钥以及带认证信息的连接字符串等明显凭据值会被拒绝；任务定义不应存储凭据或敏感原文，凭据应走数据源凭据管理。该校验不能代替企业对业务数据的分类与访问控制。运行参数和原始快照只留在内部持久化记录中，运行读取响应不返回参数值、SQL 模板或定义快照；历史数据结果还必须按照当前查看者的数据授权重新校验。

工作流声明示例：

```json
{
  "definition": {
    "parameters": [{"name": "day", "data_type": "DATE", "required": true}],
    "nodes": [
      {"id": "orders", "name": "订单查询", "sql": "SELECT COUNT(*) AS orders FROM orders WHERE order_day = $day", "depends_on": []},
      {"id": "payments", "name": "支付查询", "sql": "SELECT COUNT(*) AS payments FROM payments WHERE payment_day = $day", "depends_on": ["orders"]}
    ]
  }
}
```

工作流支持 1 至 64 个节点，节点 ID 唯一，依赖无重复、缺失或环。当前依赖表达执行顺序，不表示将上游结果自动注入下游 SQL。前端节点编辑必须保留 `definition.parameters`；参数声明与每次运行参数是两个独立表单。

## HTTP 接口与权限

以下路径以 `/api/v1` 为前缀。发布、校验由产品动作桥接到 `ProductJobService`，动作请求还遵守产品动作请求的版本合同。

| 接口 | 输入或结果 | 权限与行为 |
| --- | --- | --- |
| `POST /product/{kind}/{id}/jobs` | `{expected_version,idempotency_key,parameters:{...}}` → 202 `{job,created}` | SQL 文档/工作流需要 `development:read`、`development:execute`，来源必须已发布且版本相符 |
| `GET /product/jobs` | `source_kind`、`source_id`、`status`、`limit`、`offset` → `{data,total}` | 按租户与来源域读取；默认工作流，页大小 1 至 50 |
| `GET /product/jobs/{id}` | 真实运行及真实节点记录 | 跨租户或不支持来源返回 404；本租户再校验对应域 `read` |
| `POST /product/jobs/{id}/cancel` | 更新后的真实运行 | 需要对应域 `read`、`execute`；分析引擎离线仍可提交取消 |
| `POST /product/jobs/{id}/retry` | `{idempotency_key}` → 202 `{job,created}` | 仅失败或取消的已校验只读运行；重新校验权限、租户引擎、当前定义与版本，创建新运行 |

幂等键长度 1 至 128 字节，只接受字母、数字和 `_-.:`。同一租户、提交者和幂等键须对应完全相同的来源、版本、动作、参数和快照，否则冲突。服务仍会校验当前来源；来源已编辑时旧提交应查询已获得的 job ID，而不是将旧版本当作新运行提交。

`quality_task` 的新提交由 `QualityService` 专属执行动作编译。列表、详情、取消及重试由上述真实运行接口读取，权限为 `quality:read`、`quality:execute`。SQL 定义发布另需 `development:write`。

请求中的提交者来自认证身份，不允许任意指定用户或租户。读取权限不授予执行权限。执行前、执行过程中续租时与结果提交前，执行器重新检查当前用户/租户仍可用、实际精确执行权限仍存在、引擎仍绑定同租户，以及定义没有撤回或改变。历史运行读取不要求查询引擎在线。

SQL 文档、工作流和质量节点还必须接入 `QueryAuthorizationService`。执行器对实际绑定后 SQL 进行 authorize 和执行前 reauthorize，查询期间续租及返回结果时再 reauthorize。结果保存前必须写入数据访问审计；授权撤回记录 Denied，不保存查询结果；查询失败、取消或容量失败记录 Failed。缺少授权服务时节点失败且不调用分析引擎，审计无法提交时也不能发出假成功结果。固定 information_schema 元数据采集仅由专属编译器生成，采用 metadata read/execute 权限和反射审计；用户 SQL 无法通过标签获得这项例外。

## 不可变快照与执行控制

运行固定保存 source kind/id/version、actor、action、定义属性快照、归一后的类型化参数和节点 SQL 模板。执行器从快照重新编译，并逐项比较节点 ID、名称、依赖、模板定义，不采用后来编辑的 SQL。当前来源若已撤回或版本变化，旧运行失败；规则或物理结构漂移导致查询真实失败。

运行状态为 `queued/running/succeeded/failed/cancelled/reconciliation_required`；节点还包含 `pending/skipped`。只有仓储 `enqueue` 创建 `queued`。节点失败在同一仓储事务中将依赖它的待运行后继标记 `skipped`，整个运行失败会跳过其余待运行节点。成功运行要求所有节点成功，没有取消请求和未完成节点。

执行器 claim 仅领取白名单 `sql_document/development_workflow/quality_task/metadata_collection/bi_report`，按绑定租户隔离。元数据节点专门执行固定 information_schema 查询，并将真实目录反射与节点结果原子提交，合同见 [metadata-collection-contract.md](metadata-collection-contract.md)。数据库租约有效期 30 秒，以数据库时钟与 `(worker_id,fence)` 校验每次续期和结果提交；查询期间每 5 秒续期。旧执行器不能在新 fence 上提交结果。只读租约过期可领取重试，保留已成功节点，其他节点复位待运行；非可重放任务过期只能进入 `reconciliation_required`，由独立受控流程确认失败或取消，不能自动重放写入或手动宣称成功。

取消排队任务立即记录取消。取消运行中任务先记录 `cancel_requested`，执行器在续期或查询返回时观察，丢弃当前只读查询等待、持久化真实节点取消，然后终止运行。客户端请求中断不伪造分析引擎的底层 kill 保证；只读语义使租约接管与丢弃等待不会重复写业务数据。

worker 由应用显式启动并持有 handle。正常 shutdown 停止领取新任务，完成当前查询边界后退出；未完成工作由真实租约机制接管。handle 被直接释放会停止执行 future，仍依赖租约过期恢复。构造服务或提交请求本身不启动无限后台任务。本轮平台保持停止。

## 结果、错误与容量边界

节点结果来自真实 `QueryResult`，含真实列信息、行、行数及查询服务给出的图表建议。运行的 `returned_rows` 是节点真实返回行数之和，不是全表扫描行数或 ETL 吞吐量。现有查询服务的结果行数限制仍适用；每个查询外层最长等待 65 秒。

仓储限制每个结果及所有节点结果合计不超过 256 KiB。执行器每节点分配 `(256 KiB - 8 KiB) / 节点数` 的结果预算，额外空间预留给运行概要。超过预算时节点真实失败，不截断后标记成功；用户需缩小查询列/行或改为聚合。快照上限 512 KiB、节点定义 64 KiB且合计 512 KiB、仓储参数上限 64 KiB、错误上限 4000 UTF-8 字节。

失败保存固定、受控的错误分类，不直接保存引擎原始异常中的 SQL、连接字符串或数据值。运行成功不代表质量规则通过；质量任务的各节点有独立业务判定。

## 历史结果的读取保护

运行列表只返回状态、节点依赖、时间和尝试次数等真实控制信息，`job.result`、`node.result` 均为空；有已保存结果时标明 `result_access="summary"`，详情需要单独授权。所有运行视图还将 `definition_snapshot`、`parameters` 与 `node.definition` 投影为空对象，避免从 SQL 字面值或运行参数读取业务数据。数据库中的不可变快照没有被修改。

详情按当前 reader 的实际授权重新编译被冻结的查询，而非沿用原提交者的授权。SQL/工作流和 BI 使用普通只读数据访问授权；质量使用只允许真实已终止任务、成功节点、精确冻结 SQL 和实际元数据证明的受控 `authorize_quality_result` 路径。授权、重验或审计失败时，结果被投影为空并明确返回 `result_access="restricted"` 与原因码，节点和任务真实状态不变。元数据仅可在 `metadata:read` 下返回经过类型投影的真实采集/反射计数与范围，不返回目录查询原始数据行。

结果成功读取记入访问审计，审计提交后还重验一次当前权限。列表、详情、取消响应、重试/运行的幂等响应都需要采用同一视图；再次提交已成功任务不会绕过历史结果保护。结果保留在仓储中，恢复合法授权后可再次读取。质量报告与监控聚合使用同一数据授权边界，不能静默漏掉无权任务后宣称窗口统计完整。

SQL/工作流与 BI 的真实 QueryResult 在映射、预算检查和持久化前经过 MaskingService，适用策略在执行前、续期、返回后以及审计后重验。每个含实际查询行的节点结果附带 server-signed `privacy` 收据，摘要覆盖实际 columns/rows/row_count/suggested_chart；BI 保留实际 suggested_chart 以支持完整摘要验证。任何被脱敏字段的图表建议会被清理，存储结果不得另夹带原始行。

历史结果再次经过当前查看者的脱敏策略。可信签名正确且策略相同时不会重复计算不可逆 HMAC；策略改变、签名损坏或服务未配置时结果受限并要求重新执行，不能从旧不可逆结果恢复原值。没有收据的旧数据按当前策略作为原始结果处理，客户端自报已脱敏标记无效。固定受控质量查询只产生 typed counter result，经过 opaque quality-count 脱敏计划重验，无原始样本行，不把 QueryResult 收据混入 QualityCheckResult。元数据采集不释放 information_schema 原始结果行。

## 质量执行挂钩（受控执行）

质量服务冻结任务、质量对象、元数据和规则版本，`prepare_claimed(job)` 从不可变快照重新构造全部只读 COUNT 查询并与节点定义深比较。公共执行器只运行这些已验证查询；`map_node_result(node,result)` 将真实查询列值映射为规则 `checked_rows/failed_rows` 与判定，不生成模拟统计。

质量任务可使用保存的 `draft` 或 `published` 定义，不接受 inactive。规则执行使用冻结规则与元数据；物理 schema 变化由分析引擎返回真实错误，不静默改用当前规则。

`sample_limit` 可选，正整数且不超过 1,000,000；存在时每条规则基于受限样本计算，并明确输出 `evaluation_scope="sample"`，否则为 `full_table`。空数据不判通过，采样通过不代表全表通过。运行概要只标记成功执行的检查数量，各规则计数留在节点结果中，避免重复相加造成误导。

## 验证

单元测试源覆盖参数 AST 绑定、单双引号与反斜线、NULL/false/0、日期与数字类型、凭据/未声明参数、DAG 环与写 SQL 拒绝。SQLite + 实际 Sa 会话的服务测试覆盖幂等排队、发布不执行、节点失败与跳过、真实重试、权限撤回、跨租户读取、读取不授予执行、排队取消、运行中取消时 query future 被释放，以及超量结果失败而无伪造结果。查询执行测试使用明确标注的可控测试接口，真实 SwanLake 全流程留到整体 POC。

测试源已提交工作区，编译与定向运行由主代理在现有工具链串行执行；本文不预先声称所有 Rust 测试通过。
