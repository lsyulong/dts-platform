# 数据 API 明细查询实施合同

本文定义本产品的数据 API V1 合同。它是已购买原型所对应业务的实现设计，不代表从原型恢复了原始代码、精确 JSON 或全部交互。

## 定义与发布

`data_api` 是明细查询服务定义，通用资源身份、租户和版本由资源仓储管理。业务字段使用已有的 `sql`、`parameters`、`response_schema`、`row_limit`、`rate_limit`，以及绑定真实来源的 `metadata_object_id`。

- 来源必须是当前租户通过实际 SwanLake 元数据采集形成的 `metadata_object`，并通过 `MetadataCatalogRepo::verified_binding` 验证。复制对象的 `engine_binding` 字段不能建立来源证明。
- SQL 只能有一条 SELECT，直接读取该证明对应的 `catalog.schema.table`。表别名可用，来源必须完整写明三段名称。
- 投影只能使用实际采集的直接字段及唯一别名。别名不得与任何真实字段同名，避免字段授权歧义。星号、常量投影、聚合、表达式投影、JOIN、CTE、窗口、子查询、外部文件/表函数、采样及其他高级修饰均明确拒绝。聚合报表使用独立 BI 域。
- WHERE 支持实际字段、标量字面值/类型化参数、比较、AND/OR/NOT、NULL/布尔判断、IN 字面值列表、BETWEEN。ORDER BY 只支持实际字段或输出别名。LIMIT/OFFSET 必须是有界固定整数，不允许参数、LIMIT ALL/BY 或逗号分页。
- `row_limit` 为 1 至 1000 的整数。编译将实际 LIMIT 限制为 `min(已有 LIMIT, row_limit + 1)`，缺少 LIMIT 时添加哨兵限制。实际返回超过 `row_limit` 时整次失败，不能截断后声称完整成功。
- `rate_limit` 为 1 至 100000 的配置整数。内部控制台任务本身不伪装为外部应用配额。外部调用限额由独立的应用/凭据/订阅/调用仓储执行，必须以该仓储的真实限制和计数为准。

声明响应结构为：

```json
[
  { "name": "region", "data_type": "VARCHAR", "nullable": false, "description": "地区" },
  { "name": "amount", "data_type": "DECIMAL(18,2)", "nullable": true, "description": "金额" }
]
```

字段数量、顺序、名称、标准化类型和可空性必须逐项符合实际直接投影。未填写或空数组时，发布根据真实元数据和投影推导该结构。字段说明不超过 2048 字节。发布检查只能证明定义与已采集 schema 一致，未执行 SQL；实际物理 schema 变化仍可能使后续调用失败。

V1 支持 BOOLEAN、带宽度的有符号/无符号整数、FLOAT/DOUBLE、VARCHAR、DATE、无时区 TIMESTAMP、DECIMAL(p,s)。复杂类型、UUID、JSON、BLOB、HUGEINT、INTERVAL、带时区时间暂时明确拒绝。实际整数、日期、小数的 JSON 表达保持 QueryService 的无损转换规则；大整数和 DECIMAL 可以使用字符串，不强制转为 JavaScript 浮点数。

`parameters` 使用平台已有类型化参数合同：最多 32 个声明，`{name,data_type,required,default_value?}`，类型为 VARCHAR/BIGINT/DOUBLE/BOOLEAN/DATE/TIMESTAMP。SQL 使用 `$name`。声明必须实际使用，拒绝未知、遗漏必需项、凭据名称/内容和非法类型。解析器在 AST 中替换标量表达式并验证字符串往返，不能通过字符串拼接生成 SQL。

发布需要当前活跃用户拥有 `services:read/write/execute`。它对资源版本做 CAS 更新，将状态变为 `published`，并在服务端专属 `properties.publication` 中冻结 API 版本、真实来源/字段/证明、编译后的 SQL、参数声明、响应结构和上限。该字段不能由通用 HTTP 创建/更新请求写入。定义编辑须先撤回。

撤回需要 `services:read/write`，CAS 修改为 `inactive`、删除服务端发布快照、递增版本。重新发布必须重新读取并验证当前来源。发布、撤回不是运行成功，也不代表已注册定时器或外部凭据。

## 内部控制台真实任务

`DataApiService::enqueue(actor,id,DataApiRunRequest)` 只允许当前租户的登录用户。请求为：

```json
{
  "expected_version": 4,
  "idempotency_key": "console-api-orders-20261009",
  "parameters": { "area": "华东" }
}
```

服务重新验证 `services:read/execute`、当前 API 发布状态/版本、来源证明及当前租户 QueryAccess。只有冻结定义通过校验才能创建真实 `queued` ProductJob，来源为 `data_api`，动作 `execute_data_api`，执行器标识 `data_api_detail_v1`。一个任务包含一个受控 API 节点；不会允许请求直接创建已成功的运行。

worker 重编译发布快照，与实际节点深比较并重新绑定参数。执行前、运行心跳、返回后以及成功访问审计后，重新检查活跃提交者、领域执行权限、当前发布版本/属性和来源证明。执行使用现有租约、fence、取消与只读重试机制；任务定义状态和运行状态独立。

所有业务 SQL 必须经过 QueryAuthorizationService 的当前表/列授权与真实访问审计，以及 MaskingService 的当前输出策略。未配置授权或脱敏服务时失败。当前完整数据权限也不能绕过 API 的真实采集证明。固定元数据采集器的内部 information_schema 例外不适用于数据 API。

actual QueryResult 在输出脱敏前先核对发布时冻结的原始列名/类型、行数、每行完整键集合、值类型/整数范围/小数精度尺度及 NULL 约束。此步骤不构造可返回的原始响应，脱敏也不能隐藏真实表结构漂移。之后使用同身份、同租户、同 SQL 的 opaque MaskingPlan 执行脱敏，从计划的实际输出描述推导有效响应结构，并再次严格校验脱敏后的 QueryResult。

响应的 `source_response_schema` 保留发布时冻结的原始声明；`response_schema` 和 `effective_response_schema` 描述本次真实输出。当前 hash、redact、partial 输出字段变为 VARCHAR，原 NULL 保持 NULL，因此可空性保持原始声明；未来只有计划明确声明会引入 NULL 时才扩大可空性。没有根据偶然观察值猜测字段类型或可空性。数值脱敏列不会被声称仍返回数值。`schema_policy_hash` 和 `mask_applied` 记录当前服务端输出计划；客户端无法构造计划或受保护结果。真实结果超过存储/查询限额时失败，不保存半份成功结果。

保存的结果只包含实际输出、真实 schema/版本、`complete: true` 和服务器签发的脱敏 receipt。内部调用标记 `invocation_scope: authenticated_console_jobs`、`external_credentials_ready: false`、`rate_limit_enforced: false`；这些标记描述内部任务，不能当作外部开放能力证明。

HTTP 提交/retry 的 raw EnqueuedProductJob 必须由 `ProductJobService::submission_view` 投影后返回，不能直接泄露冻结 SQL、参数或持久化结果。列表仅返回状态汇总。详情重新用当前读者授权、来源证明及脱敏策略验证真实结果；API 撤回、版本/证明变化、授权失效或策略不可恢复时，结果设为 `None` 并返回 `result_access: restricted`。真实成功状态与数据库中的实际结果保留，不伪造成失败或删除结果。

## 外部应用调用的接口边界

数据 API 域提供不可由客户端构造的 `PreparedDataApiCall`，仅公开 `sql()`：

```text
prepare_invocation(actor, api_id, expected_version, parameters) -> PreparedDataApiCall
reauthorize_invocation(actor, prepared) -> ()
protect_invocation(prepared, raw QueryResult, masking_service, opaque MaskingPlan)
    -> ProtectedDataApiResult
map_invocation(prepared, ProtectedDataApiResult) -> actual response Value
```

prepare 绑定活跃所有者身份、租户、当前发布资源/版本、真实采集证明以及已安全绑定的 SQL。reauthorize 重查上述全部关系；不同用户/租户不能使用该对象。protect 使用上述先验证原始响应、再脱敏、最后验证有效结构的流程，并消费原始 QueryResult；其返回对象只提供已脱敏 `query_result()` 供服务器签名使用。map 只接受不可由客户端构造、与本发布版本绑定的 ProtectedDataApiResult。此域不签发应用凭据、不管理配额，也不能独立证明外层调用链已经完成授权。

外部调用采用同步受控执行。接入方必须按以下顺序完成真实流程：

1. ServiceAccessService 验证真实应用凭据和当前批准订阅，原子预留真实配额，返回 opaque CallPermit；应用凭据不能冒充 Sa 用户 token。
2. 以 permit 的当前活跃所有者 AuthIdentity、permit 的 API ID/版本调用 prepare，再用 QueryAuthorizationService 和 MaskingService 校验真实数据权限与输出策略。
3. 执行前、等待期间和返回后重验 permit、prepared、opaque 查询授权和脱敏计划；65 秒内执行真实 QueryService。超时、取消、撤回、凭据/订阅失效或策略改变时记录实际失败/拒绝。
4. 用 protect 验证原始 schema、执行当前输出脱敏、验证有效 schema；调用方不能先自行脱敏后把结果当原始查询交回。记录访问审计并在返回前再检查授权，使用 map 得到安全响应；ServiceAccessService 持久化真实调用结果、行数和时长。只有成功落账才返回数据。

该同步流程不新增跨仓储 call→job 绑定，不声称外部排队能力已经实现。外部凭据、订阅审批、实际配额和调用审计由其专属服务提供，以实际接线和验证状态为准。

## 验证与范围

定向纯单元源覆盖真实投影/响应推导、别名与类型/可空性、禁止的 SQL 形状、参数引号注入、冻结快照替换、完整真实结果和超限、整数宽度/小数尺度/时区类型。SQLite + 实际 Sa 认证/元数据仓储的服务测试源覆盖真实反射证明、发布不执行业务查询、实际观察结果持久化、超限/schema 漂移、排队和运行中撤回、历史结果限制及伪造证明。

这些执行器观察值明确是测试夹具；它们验证控制面、SQLite 仓储与服务编排，不代表真实 SwanLake、外部客户端或全平台 POC 已完成。按当前用户要求，服务保持停止，整体 POC 放在模块实现之后。
