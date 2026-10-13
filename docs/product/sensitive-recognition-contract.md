# 敏感识别实施合同

这是本产品的实际实施合同，并非声称从购买原型恢复了全部交互或内部数据结构。四个目录实体分别是 `data_classification`、`recognition_rule`、`recognition_task` 和由真实扫描产生的 `sensitive_finding`。开发验证使用 SQLite；PostgreSQL 实现采用相同领域接口。整体引擎 POC 留到产品功能阶段完成后进行。

## 定义与发布

密级包含整数 `rank`（1 至 999，同租户唯一）和必填 `handling_policy`。发布密级只是启用分类定义，不代表平台已自动执行该处理要求。

识别规则包含真实 `classification_id`、`pattern`、`match_target`（`column_name` 或 `sample_value`）、`sample_limit`（缺省 1000，最多 1000）和 `source_types`（缺省 `["text"]`；允许 `text`、`number`、`date`、`boolean`）。Rust regex 限制表达式为 1024 字节，编译空间和 DFA 缓存各 1 MiB，嵌套深度 32；不支持回溯引用和环视。规则发布会冻结当前已发布密级版本。

识别任务包含 `rule_id`、`object_id` 和必填 `sample_limit`；任务上限不能超过规则上限。发布要求规则及密级已发布，元数据拥有内部采集仓储签发的当前 SwanLake 证明。任务发布冻结规则、密级、真实元数据版本、完整字段快照、catalog/schema/table 和结构指纹。`publication` 仅能由服务端生成，HTTP 创建和编辑不能提交这个字段；已发布定义需先撤回才能编辑。撤回立即阻止新任务和既有运行的下一次授权检查。

## 实际执行与成本

扫描只连接当前租户绑定的 SwanLake QueryService。绑定来自 `MetadataCatalogRepo::verified_binding`，不信任请求或资源自行声明的 engine binding。编译器只生成对真实三段表名、显式字段列表的 `SELECT ... LIMIT n`；无外部扫描、任意 SQL 或表函数入口。样本模式选择与 `source_types` 匹配的受支持标量列；名称模式使用受支持字段的 `LIMIT 0` 获取真实 schema，不读取数据行。

执行进入真实 ProductJob 队列，来源 `recognition_task`，动作 `execute_recognition`，固定单节点 `recognition`。执行前、心跳、实际查询返回后都检查当前用户、精确 `sensitive_data:read/execute` 权限、运行租约 fence、取消状态、冻结定义版本和采集证明。受控查询授权要求当前表的完整字段授权或精确 `data_security:all_data`，即使扫描只选择部分字段，也不会放宽表数据权限。

实际结果必须匹配冻结字段名称、类型、非空约束和完整 row_count。最多 128 列、1000 行，单值最多 4096 字节，总匹配文本最多 1 MiB，编码后的原始查询响应最多 2 MiB；超过任一上限则整次失败，不截断后宣称成功。原始数据只在受控扫描函数的内存中用于 Rust regex 匹配，退出扫描后仅传递无原样本的计数对象。

`sample` 始终表示本次有界取样；少于上限、零匹配或零行都不能证明整表不敏感。`column_names` 表示名称推断，不等同于内容识别。平台不会自动确认密级、创建脱敏规则或改变授权。

## 无样本结果与真实发现

节点结果为 `SensitiveScanResult`：`evaluation_scope`、`sample_limit`、`sampled_rows`、逐列 `checked_values/matched_values/null_values`、`definition_hash`、`producer_committed` 和 `finding_ids`。摘要哈希只覆盖冻结定义，不计算或保存原始敏感值的摘要。结果不包含 rows、样本文本或完整查询输出。

`SensitiveFindingRepo::commit_scan` 仅供内部 worker 调用。同一数据库事务内重验实际 Running 单节点、未取消且未过期的租约 fence、当前用户精确权限、冻结来源版本和实际采集证明，然后仅对 `matched_values > 0` 的列创建发现、目录投影及发现审计，最后同时将真实节点与任务 CAS 为 Succeeded。`producer_committed=true` 和实际 finding_ids 只能在该事务成功提交后返回。任何冲突、写入错误或租约到期会回滚全部发现及成功状态；零匹配也由同样流程提交实际成功，但没有发现记录。

目录投影只包含真实引用、job/node、定义摘要、范围说明和复核状态，不包含原始数据或匹配计数。发现实体不能通过普通资源创建、更新或删除制造可信结果。typed finding API 才能读取证据，并按当前读者自己的表授权、真实已成功来源任务和当前采集证明重新检查，记录访问审计。缺少查询授权守卫时拒绝读取，不能用提交者的历史权限代替读者权限。

## 人工复核

确认或忽略必须提供 `expected_version` 和非空 `reason`（最多 2000 字节），仅 Pending 状态允许复核。确认还必须在同一事务检查当前冻结任务、规则、密级和采集证明；定义撤回或版本改变后不能将旧发现确认为当前分类。忽略允许处理过时发现，仍要求真实发现来源、当前 `sensitive_data:read/write`、理由、CAS 和审计。

typed 发现状态与目录投影状态/版本同一事务更新，审计保存真实复核人、理由、操作和数据库时间。复核响应为不含计数的回执：id、status、version、reviewed_by、review_reason、reviewed_at、`classification_enforced:false`；需要证据时另走受控读取接口。

## 接线与验收边界

- `SensitiveRecognitionService::new(resources, jobs, metadata, findings, auth, QueryAccess).with_query_authorization(...)`；定义 validate/publish/withdraw 和任务 enqueue 由对应业务动作桥接。
- ProductJobService 注入实际 finding repo；运行列表、取消、失败和只读重试复用真实 jobs 接口。公开结果视图必须使用 `authorize_recognition_result`，禁止把普通 rows 历史输出作为扫描结果。
- typed finding 路由为 `GET /product/sensitive-findings`、`GET /product/sensitive-findings/{id}`、`POST /product/sensitive-findings/{id}/confirm` 和 `/dismiss`；薄路由由主任务统一注册。
- 敏感专页包含配置、实际运行和发现复核三个工作区。任务动作使用 `/product/resources/recognition_task/{id}/actions/{action}`；请求携带实际 `expected_version`，执行传入稳定逻辑幂等键。运行结果只显示当前授权、实际成功且有 producer 回执的节点计数；finding 链接指向真实发现记录。所有 typed 证据查询缓存绑定用户、租户、会话及精确权限范围，读取被拒绝时立即隐藏旧计数；未选择任务或发现时不会请求 undefined 详情。
- 数据库 migration 018 注册 typed finding、发现审计及密级顺序唯一约束，真实 job/node 和同租户资源 FK 保护发现来源。
- 当前编写的单元测试检查正则成本、实际计数/null、空样本、超限、schema 漂移、来源证明和复核输入；SQLite 仓储测试通过实际队列领取/节点开始，检查原子成功、fence/到期/取消/失效来源回滚、零匹配和复核 CAS。仓储测试中的观测数为隔离测试夹具，不是实际 SwanLake 扫描或整体 POC。
- 无定时器注册、无全库外部驱动扫描、无智能语义分类、无自动策略执行承诺；这些能力需要各自真实执行与验证闭环后再扩展。
