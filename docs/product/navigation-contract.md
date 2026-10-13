# 租户导航策略合同

本模块实现租户内菜单模板的真实分配、读取、撤销和历史查询。它只控制导航和模块入口展示，不授予 API 权限，不替代 Sa 会话及各业务 API 的身份、角色、租户和数据授权检查。菜单与模板继续是 `draft` 定义；本模块没有创建虚假的发布流程。

## 接线接口

共享入口为 `dts_backend::repo::navigation`，四个仓储方法为：

```rust
async fn assign(&self, &AssignNavigationParams) -> Result<NavigationDecisionResult, BackendError>;
async fn revoke(&self, &RevokeNavigationParams) -> Result<NavigationDecisionResult, BackendError>;
async fn current(&self, &ReadNavigationParams) -> Result<NavigationView, BackendError>;
async fn history(&self, &NavigationHistoryFilter) -> Result<NavigationHistoryPage, BackendError>;
```

驱动构造分别为 `SqliteNavigationRepo::new(SqlitePool)`、`PgNavigationRepo::new(PgPool)`，两后端迁移编号均为 `019_tenant_navigation.sql`。

服务构造为 `NavigationService::new(Arc<dyn NavigationRepo>, Arc<NavigationRegistry>)`。服务的 `current(actor)`、`assign(actor, template_id, AssignNavigationRequest)`、`revoke(actor, RevokeNavigationRequest)`、`history(actor, limit, offset)` 对应上述方法。主应用负责模块导出、迁移、BackendContext、服务工厂、真实 API 和前端接线；模块实现本身不会启动平台或执行 POC。

`NavigationRegistry::new(Vec<RegisteredNavigationRoute>)` 只接受服务端构建的已注册产品目录。每条注册记录包含 `id/path/name/required_permissions/sort_order`；名称为纯文本，排序为 0 至 1000000，路径为实际站内注册路径，权限来自真实 `PERMISSIONS`。目录不从 HTTP 反序列化，不能由客户端传入。当前 API 权限特殊路径的实际读要求应在构建目录时明确设置，例如用户管理使用 `user:manage`。后续目录删除路径时，冻结策略中的旧路径也会被读取过滤。

## 请求与权限

分配和撤销必须由当前租户中的真实有效账号执行，同时具备当前数据库角色绑定产生的 `system:write` 与 `user:manage`。历史管理也要求两项权限。普通读取只允许当前有效用户读取本人所属租户；数据库检查用户与租户实际状态，并通过同租户角色关联重新读取当前权限。

客户端分配请求只接受：

```json
{
  "expected_template_version": 3,
  "expected_assignment_version": 0,
  "idempotency_key": "client-stable-operation-key",
  "expires_at": null
}
```

模板 ID 来自路径参数。身份、租户、注册路由、菜单快照、权限列表和结果状态都不从该请求接收。未分配过时 `expected_assignment_version=0`；之后必须使用当前导航接口返回的分配版本。

撤销请求包含 `assignment_id/expected_assignment_version/idempotency_key/reason`。两个请求 DTO 均拒绝未知字段。撤销要求实际分配 UUID、当前 CAS 版本和非空原因；可以撤销已到期策略，已撤销策略不能再次以新幂等键执行撤销。

## 分配与冻结

每个租户只有一个当前分配，首次产生稳定 UUID；重新分配与撤销保持该 UUID，每次成功 CAS 更新版本加一。事务内读取真实模板及全部菜单，验证模板版本、同租户、`draft` 状态、完整真实 `menu_ids` 引用和至多 1000 个菜单。菜单父节点必须全部包含于模板，父子关系不得形成环。源定义读取总量限制为 8 MiB，持久化完整冻结快照限制为 2 MiB。

冻结快照保存模板 ID、版本、名称、版本类型、有效天数，以及每个菜单的实际 ID、版本、父 ID、名称、已注册路径、附加权限、可见性、排序与安全图标标识。菜单路径不得包含外链、协议、查询、片段、转义路径或脚本；附加权限只能收紧已注册路径的当前读要求。图标只能使用字母、数字、下划线和连字符标识。

模板、菜单和实际用户均有真实租户 FK；不可变分配回执及菜单历史 FK 保留历史来源身份。历史来源被引用时不能物理删除。修改定义内容或版本不会自动改变已生效的冻结策略；需要管理员重新分配。冻结历史也不将新定义解释成原批准配置。

数据库时钟确定分配时间和有效期。模板 `validity_days` 可选，范围 1 至 36500；存在时默认从实际分配时间计算期限，不存在时默认无到期时间。显式期限必须在数据库当前时间之后，且不能超过模板默认期限。PostgreSQL 用原生时间类型并保留数据库微秒精度；回执比较原始请求时间语义，重放不会按新的当前时间重新计算期限。

## 导航读取

`NavigationView` 返回：

- `mode`：`default_catalog`、`assigned` 或 `expired`。
- `assignment_id/assignment_version/template_id/template_version/expires_at`：当前实际分配摘要；从未分配的版本为 0。
- `nodes`：已过滤的实际节点，包含 `id/menu_version/parent_id/name/path/sort_order/icon/source/recovery`。
- `filtered_count/observed_at/affects_navigation_only`：过滤数量、数据库观测时间和真实用途。

没有分配或显式撤销后，`default_catalog` 按当前权限显示注册目录。生效分配显示冻结节点，同时检查当前注册路径的全部读权限与冻结附加权限。父节点隐藏、无权限或不再注册时，其子孙也不展示。当前角色或权限被撤销后，旧快照不能继续显示受限入口。

已分配策略到期后为 `expired`，不会回退完整产品目录。以下恢复入口在当前权限允许且仍已注册时保留，防止配置锁死：

| 路径 | 当前附加要求 |
|---|---|
| `/system/menus` | `system:read` |
| `/system/menu-templates` | `system:read` |
| `/system/users` | `user:manage` |
| `/account/profile` | 当前有效账号 |

恢复入口还检查该实际注册路径的读权限。读取导航不返回未授权完整菜单快照，也不返回新增 API 权限。`source=catalog` 表示实际注册目录条目，`source=template` 表示真实已分配冻结菜单；它们不是数据业务执行结果。

## 幂等与审计

回执产生稳定 UUID，并以 `tenant_id + idempotency_key` 唯一。事务先验证当前真实管理权限，再比较已存在回执的原 actor、动作、模板或分配 ID、模板版本、CAS 版本、原请求期限与撤销原因。完全相同返回首次完整分配回执；同键不同语义返回冲突，不能泄露其他 actor 的原操作结果。

分配状态、不可变回执、真实菜单历史引用在一个事务提交。回执或引用写失败时，CAS 更新也回滚。SQLite 使用 `BEGIN IMMEDIATE`；PostgreSQL 使用租户写锁、当前分配行锁和当前用户及权限关系读锁，串行化定义读取与分配写入。读取及历史 COUNT/分页共享同一读快照。

历史分页返回实际回执、actor、动作、分配版本、完整冻结结果与数据库时间。到期本身不会造出未发生的操作回执；撤销和重新分配保留原历史。

## 前端接线

`web/src/product/navigation` 导出 `NavigationProvider`、`useNavigation` 和独立使用的 `useNavigationPolicy`。主布局在现有 QueryClient 与身份恢复层内挂载 Provider；Sidebar 和路由入口使用返回的 `phase/nodes/allowsPath(path)/refresh()`。`phase` 为 `unauthenticated/loading/ready/error`，仅 `ready` 时可使用服务端实际返回的节点。路径匹配允许明确入口及其以 `/` 分隔的子路由，不将相似前缀解释为同一入口。

查询缓存使用不含 token 的会话序号；账号、租户、token、当前权限、角色或身份恢复状态变化时切换缓存空间。未登录、正在恢复、当前策略读取或刷新失败及已观测到期时均收起旧节点，不在客户端生成默认目录。同会话尚未到期且已经验证过的策略可在后台重校验期间继续使用，避免布局卸载尚未保存的表单；请求失败后立即关闭显示。到期重新查询使用数据库 `observed_at` 与 `expires_at` 的时间差，并扣除客户端已持有该次响应的时间。默认目录、过期后恢复入口均由真实后端返回。

`web/src/pages/product/NavigationPolicyPanel.tsx` 默认导出管理面板，Props 为可选 `className` 与 `initialTemplateId`。它展示真实当前策略与管理员分页历史；分配前查询实际 `draft menu_template` 分页和详情版本。读取模板需要 `system:read`，分配、撤销和历史仍需要 `system:write + user:manage`，只有读取权限不会出现写操作。客户端不上传菜单快照、权限或结果状态。

面板按模板 ID/版本、当前 CAS 版本、原始指定期限或撤销 ID/版本/原因形成操作语义，并在当前面板会话内保留稳定 UUID。请求失败、关闭弹窗后重新执行相同语义仍使用原 UUID；更改语义产生新键。成功结果显示服务端真实回执 ID、版本与重放标志。关闭正在提交的弹窗后，迟到成功会刷新实际策略，不会重新打开弹窗或显示旧操作结果；账号与权限切换后不会回流原会话结果。定义变更与当前策略 CAS 冲突由后端返回，面板不会自动修改版本重试。

精确请求路径为 `GET /product/navigation/current`、`GET /product/navigation/history`、`POST /product/navigation/assign/{template_id}` 和 `POST /product/navigation/revoke`；它们使用现有 `/api/v1` 客户端前缀及发起请求时的明确 Authorization。

## 验证状态

新增共享纯边界测试 5 例和 SQLite 仓储测试 8 例源码，覆盖真实引用/注册路径/父闭合与环、默认期限、当前权限收紧、过期不回退、双权限与跨租户拒绝、稳定回放、并发 CAS、定义冻结、恢复入口、撤销、审计失败回滚及真实历史 FK。

已使用现有 rustfmt 进行 Rust 源码格式解析，并用已有 Python SQLite 验证实际 001/002/004/019 迁移语法、019 重复执行及外键检查。Rust 编译与上述仓储测试由主代理统一执行；本记录不将测试源码存在表述为测试通过。

前端两个独立 Vitest target 已串行通过 12 例，覆盖同会话后台重校验保留已验证策略、权限/会话切换及旧请求回流、身份恢复、刷新失败隐藏缓存、过期策略与路径边界、跨租户/外链响应拒绝、只读不执行、实际模板版本及 CAS、失败后稳定幂等键、关闭待完成弹窗、身份切换后的迟到回执和真实撤销原因。使用现有 Node、Vitest 与 MSW，未安装工具。前端全量 TS、Vite、lint 和 Rust 集成检查由主代理负责。整体 POC 尚未开展，本模块不声称真实引擎执行能力。
