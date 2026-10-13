# 主数据控制面合同

本合同说明当前真实存储和业务状态，不表示下游物理发布或分发已完成。模型、记录和编码规则使用现有 `product_resource` 作为唯一前端业务对象；`009_master_data.sql` 的类型表提供同事务唯一性、冻结版本、历史和序号设施，不创建另一套显示记录。

`MasterDataService` 接收现有 `ResourceInput`、`ResourceUpdate`、`ResourceAction`。所有写都重新验证真实、同租户、有效用户和租户；业务写、引用、资源版本和资源审计在一个仓储事务提交。审批另行检查实际角色权限 `master_data:approve`，普通 `master_data:write` 不足以审批。SQLite 保留完整语义，PostgreSQL 用同一租户行锁协调关联写。

模型 `columns` 有严格字段名、类型、空值和字段数量校验。`key_fields` 必须引用非空的标量字段，不能用 JSON 或浮点数作为身份。模型发布将实际版本及 schema 写入冻结表，资源状态变为 published；返回 `physical_published:false` 和 `publication_scope:control_plane_schema`，没有执行 DuckDB DDL。冻结后不能原地修改，扩展结构需要新的模型版本对象。

记录的 `values` 必须匹配冻结模型；未知字段、错类型、必填空值或缺少身份字段都会被拒绝。DECIMAL 使用精确十进制字符串，不经浮点数转换。DATE、带时区 TIMESTAMP、UUID 和 DECIMAL 身份先规范化，再对带字段名、类型值的稳定 JSON 计算 SHA-256，固定长度指纹进入数据库唯一约束。业务显示键 `business_key` 另有同租户、同模型的唯一约束，不能用更换显示键绕过实际字段身份唯一性。

新记录为 draft，提交变为 pending，审批为 approved/rejected，记录操作人、时间、原因和不可变版本快照。创建人和提交人都不能审批本人记录；审批意见必填。pending 不能编辑。approved 的编辑生成新 draft 修订，既有批准快照保持只读；曾批准记录不能改模型、业务显示键或实际身份字段。驳回记录可以纠正后重新提交。`approved_records` 为分发器返回各记录最新实际批准版本，即使当前记录已有新的草稿，也不会把草稿当作已批准数据。

模型冻结表和记录历史禁止更新；提交及审批历史禁止删除。有实际审批历史的记录不能硬删除。未提交草稿可以删除，被关联定义或编码分配引用的对象由真实外键保护。

编码规则在同事务写入真实序号状态，宽度限 1 至 18，起始序号不能超出位宽。编码分配为真实独立事件，不改变规则的定义版本；每个模型编码和每个规则序号都有数据库唯一约束。分配参数带 `idempotency_key`，重试只在操作者、规则和规则版本完全一致时返回同一次分配，不给另一账号暴露历史编码。分配后不能改模型、前缀、位宽或起始号。最后一个序号分配后保留该值并标记耗尽，后续失败不继续推进或溢出。

主数据模型拒绝字段名精确命中 password、pwd、password_hash、secret、token、access_token、refresh_token、api_key、private_key、client_secret，避免成为运行凭据存储。普通业务 PII 仍是业务数据，需要后续数据权限和脱敏出口实施。`*_credential_ref` 作为主数据字段时仅是业务字符串，不解析为凭据，不进入引擎认证配置。此控制面合同不声称已完成脱敏、外部系统分发或物理数仓发布。
