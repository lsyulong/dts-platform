# 组织成员与负责人合同

本模块以当前租户中的实际组织、账号和唯一成员归属实现加入、调动、移除、负责人任免及历史查询。组织继续使用真实 `ProductResource(kind=organization)` 定义及无环父级关系；成员与负责人不由普通 JSON 配置产生，不创建镜像用户或虚构管理角色。控制面使用 SQLite/PostgreSQL，调动不会改变 RBAC 角色或数据权限。

## 接线与权限

共享入口为 `dts_backend::repo::organization_members`；驱动构造为 `SqliteOrganizationMembersRepo::new(pool)` 与 `PgOrganizationMembersRepo::new(pool)`，迁移为两后端的 `020_organization_members.sql`。`OrganizationMembersService::new(Arc<dyn OrganizationMembersRepo>)` 的 `organization/membership/members/transfer/remove/manager/history` 方法对应仓储动作，API 同名函数在新的 `api/organization_members.rs` 中。注册、工厂、现有目录和路由装配由主代理负责。

普通详情、成员、任意指定用户归属和历史查询均要求实际有效的当前租户账号及数据库角色中的精确 `system:read`。写动作要求精确 `system:write + user:manage`，事务内再次检查用户、租户、角色绑定和权限；客户端负责人字段、显示名称及前端按钮状态均不构成授权。SQLite 使用 `BEGIN IMMEDIATE`，PostgreSQL 锁定当前租户、当前身份/权限关系和需要修改的实际用户，协调其他控制面仓储的写事务。

公告可见性的窄接口为 `self_organization(tenant_id, actor_id) -> Result<Option<String>, BackendError>`。调用方只能使用已认证身份构造两个参数；仓储验证实际有效用户与租户，查询该 actor 的唯一实际归属，再核对组织及祖先当前可用。它无需 `system:read`，不返回用户目录、其他成员或任意指定用户的归属，不回退旧属性。无归属返回 `None`；失效身份返回 Forbidden，数据库异常保持错误。当前可用组织是本项目的 `draft/published` 定义，`inactive` 分支不能作为当前有效归属使用。

实际 API 路径为：

- `GET /product/organizations/{organization_id}`。
- `GET /product/organizations/{organization_id}/members?q=&limit=20&offset=0`。
- `GET /product/organization-members/users/{user_id}`。
- `POST /product/organization-members/users/{user_id}/transfer`。
- `POST /product/organization-members/users/{user_id}/remove`。
- `POST /product/organizations/{organization_id}/manager`。
- `GET /product/organization-members/history?organization_id=&user_id=&limit=20&offset=0`。

## 唯一归属、版本和调动

`organization_membership` 的主键为 `tenant_id + user_id`，对应一个真实当前组织或已移除的空归属。首次写入产生稳定成员 UUID 和版本 1；每次归属变更加一。移除不会删除这条记录，从而保留版本并防止再次加入时发生 ABA。没有任何历史成员记录时，查询返回 `membership=null`，加入使用 `expected_membership_version=0`；历史已移除成员必须使用其实际版本。

加入和调动必须是实际 active 用户，目标组织及全部祖先必须同租户且当前可用，检查深度有 128 上限和环保护。移除允许当前已停用成员，由真实管理员带理由清理归属；它不增加任何权限。数据库真实用户状态 FK 更新会保留停用状态，不能把停用账号解释为 active。仍有当前归属的账号不能直接软删为 deleted；移除归属后可软删，历史身份仍保留。

调动请求只包含 `expected_membership_version/target_organization_id/expected_target_version/reason/idempotency_key/manager_handover`。目标版本是实际组织资源版本，成员版本是实际成员关系版本，两者不能混用。移除没有目标字段。DTO 拒绝未知字段，身份与租户只从会话构造。

同租户单用户唯一归属由数据库主键保证，CAS、实际引用和不可变回执在一个事务提交。不存在的用户、外租户用户/组织、已停用目标、版本冲突或审计失败均不能留下部分调动。普通成员变化不改组织定义版本；负责人任免会增加实际组织资源版本。

## 负责人继任与卸任

`organization_manager` 通过实际复合 FK 指向该组织的真实成员关系，以及同租户实际用户。新负责人必须 active 且已经属于本组织；无法用自由文本或其他组织成员任命。

任免使用 `expected_organization_version/manager_user_id/reason/idempotency_key`。`manager_user_id=null` 是带理由的明确卸任。相同负责人以新请求再次任命属于无变化冲突，不生成虚构操作。

正在调动或移除的用户如果是当前负责人，必须在同一事务提交 `manager_handover={expected_organization_version,successor_user_id}`。继任者必须是本组织中另一个实际 active 成员；明确 `successor_user_id=null` 表示卸任。未提供离任决定、选择自己、普通成员试图附带负责人修改或原组织版本变化均拒绝。成员移动前先完成真实负责人关联变更，真实成员 FK 保护该顺序。失败会一起回滚负责人、组织版本、成员归属和回执。

负责人账号后来停用时，查询返回真实停用状态及 `manager_valid=false`，不会伪造有效负责人或自动授予继任角色。管理员可以先完成实际继任或带理由卸任，再清理该成员。

## 定义、删除和旧配置

现有 `002_product_resources.sql` 已用 `(tenant_id,kind,parent_id)` 真 FK、同锁子项检查及防环逻辑保护组织层级。020 当前成员 FK 和负责人 FK 阻断有实际成员/负责人组织的删除；将组织分支设为 inactive 时，数据库递归检查该分支的实际成员并拒绝直接关闭。

成员历史冻结实际操作时的原/目标组织 ID、编码、名称、版本与父 ID。历史不以永久组织 FK 阻止已经清空的组织删除；当前真实关系仍受 FK 保护，历史不会被重新解释成当前组织。真实用户和 actor 历史 FK 保留身份。

旧 `properties.member_ids/manager_user_id` 不自动迁移为当前归属，详情返回 `legacy_membership_ignored` 说明待核对。数据库拒绝新增或更新普通组织定义中的这两个配置字段，前端目录须将它们设为只读并通过 typed 流程操作。实际负责人变更会清除旧字段和旧外部引用，增加真实资源版本并记录资源审计。

组织详情返回 `{organization,manager,manager_valid,member_count,children_count,legacy_membership_ignored}`。`organization.properties` 不返回旧 `member_ids`，负责人 ID 只由当前实际关联投影；引用列表过滤旧成员/负责人配置后仅加入当前真实负责人引用。成员列表独立真实分页，不将首页数组当成全组织成员。历史与列表 COUNT/分页共享同一读取快照，搜索使用字面 LIKE 转义。

员工 DTO 只序列化 `id/tenant_id/username/display_name/status`。仓储查询不读取或返回密码、密码 hash、会话、角色和调用凭据。成员对象为 `{user,membership,is_manager}`，归属包含稳定 ID、当前组织、版本、最近 actor 和数据库时间。

## 幂等和页面

每次写入要求稳定 UUID。租户与幂等键唯一，回执比较原 actor、动作、实际用户/组织 ID、原始 CAS 版本、理由和完整继任决定；相同语义重放首次结果，其他语义或其他 actor 返回冲突。重放前仍检查当前实际管理权限。回执记录真实 actor、理由、数据库时间和冻结结果，更新及删除被数据库触发器拒绝。

新 `OrganizationsPage({module})` 保留组织定义工作区，选择真实组织后打开 `OrganizationMembersPanel({organizationId})`。新 typed hooks 位于 `web/src/product/organization-members`，复用已建立的 opaque 身份序号和明确请求 token，权限或身份切换后重建页面状态。

页面查询真实用户目录、用户归属、组织版本、本组织成员和历史。负责人离任必须选择实际继任成员或明确卸任；失败保留原版本与同语义 UUID，不会自动改版本重试。关闭待完成弹窗后不会重新打开；身份切换后不显示原会话回执。停用成员只允许理由明确的移除，调动和新任负责人按钮禁用。目录和成员均实际分页，读取失败不显示旧缓存或伪造零人数。

## 验证状态

已用现有 rustfmt 解析新 Rust 源码，现有 Python SQLite 检查实际 001/002/004/020 DDL、重复执行、FK、成员版本及组织关闭保护。新增 2 例共享纯测试和 9 例 SQLite 仓储测试源码，Rust 执行由主代理统一进行；测试源码存在不代表已经通过。

新组织前端定向 Vitest 7/7 通过；与修订导航的 12 例一起串行验证，共 19/19 通过。定向 Oxlint 无警告或错误，Prettier 已通过。完整 TS、Vite、Rust/HTTP 集成由主代理统一验证。本模块没有启动服务或执行整体 POC，不声称完成全部平台功能。
