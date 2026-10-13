# 标准发布、模型绑定与实际采集结构检查

本合同是本产品的实现规范，不声称已从购买原型恢复全部交互。实际目录沿用 `naming_standard`、`field_standard`、`enum_standard`、`unit_standard`；不另造 `standard_code` 实体。标准编码属于资源 `code`，枚举内容编码属于 `enum_values[].value`。

## 版本与生命周期

标准定义的草稿保存在既有 `product_resource`。发布需要当前有效用户的 `standards:write` 与 `standards:execute`，验证领域属性和固定子版本后，同一事务保存不可变标准修订、真实外键、资源发布状态与脱敏审计。发布后的实际资源版本即 `revision_version`，不能用创建时间或客户端标记代替修订证据。

修订把当前已发布定义放回草稿，原有发布修订保持有效。新版本发布不会自动升级模型或指标已经固定的旧版本。撤回按 `revision_version`、`state_version` 和理由进行 CAS；被撤回修订的后续发布与执行失败关闭，不修改已经存在的物理数据。若撤回的是当前发布定义版本，该定义回到草稿；撤回旧版本不能悄悄改变后来草稿或其他有效修订。

`field_standard` 的 `unit_id` / `unit_version`、`enum_id` / `enum_version` 必须成对，且指定同租户真实已发布有效修订。子标准的哈希由服务端读取固定。草稿、撤回版本、跨租户对象、客户端声明的哈希不能代替此证据。当前字符串枚举编码只支持 `VARCHAR` 字段，单位只可绑定数值类型；其他类型的枚举内容需要真实值规则支持。

命名正则使用 Rust regex，完整名称匹配，模式最多 2048 字符，正则和 DFA 构造预算各 1 MiB；不支持反向引用。字段数据类型按实际 DuckDB 模型编译器规范化。业务 `definition` 文字被固定，但文字描述本身不能证明业务语义合规。

## 模型字段固定引用

模型字段保留核心 `name/data_type/nullable/description`，增加可选配对 `standard_id/standard_version` 和 `naming_standard_id/naming_standard_version`。表命名标准用模型顶层同名配对。绑定 API 只接受真实 ID 与版本，服务端解析不可变修订及子版本，生成真实绑定集与哈希。请求不能构造 `FrozenStandardRef` 或发布证明。

`bound_fields` 仅覆盖显式绑定字段；`all_fields` 要求每个模型字段绑定字段标准。字段类型必须与固定标准规范类型一致；标准要求非空时，模型字段或真实主键必须提供有效非空约束。更严格的模型非空约束可以满足允许空值的标准。表与列命名标准须匹配正确 `object_type`。

绑定保存后模型版本增加，完整属性哈希被冻结。生命周期状态版本增加不会自动令同一属性绑定失效，字段或引用发生变化则必须显式重绑。已经发布的模型只有 `upgrade=true` 才能更换绑定，并原子回到草稿；必须重新经过真实物理发布或结构核对，不能伪造延长旧 `model_publication` 回执。空绑定不产生“标准通过”。未配置的旧模型发布明确记录 `not_configured`；配置了绑定但无法验证时禁止降级。

## 实际模型发布门禁

`StandardGovernanceService::prepare_model_publication` 返回不可由客户端创建或反序列化的 `ModelStandardPublishClaim`。准备阶段核验当前有效用户与 `models:read/models:write/models:execute/standards:read`，固定模型版本、属性哈希、绑定版本及所有有效修订，原子 CAS 模型为 `publishing`。

实际引擎操作仍由 `ModelPublisher` 执行：真实 Flight SQL 写入或现有基础表结构核对、实际三段物理身份、字段与主键核验。定义验证或网络请求发出不能成为发布成功。

`finish_model_publication` 使用实际引擎结果。资源状态、`model_publication` 回执和门禁审计必须同一数据库事务提交，重复完成仅可重放相同占用和相同实际结果。标准撤回、用户停用、权限改变或占用冲突不能产生假发布。物理操作已经报告成功但控制面证据失效时，保存真实回执并记录 `publish_unknown/invalidated`，要求核对实际物理结果；不能假称已经回滚。

回执包含 `model_version`（真实发布占用版本）、`receipt`（实际引擎回执）、`standards.state`、`binding_version/binding_hash/dependencies`、真实门禁 ID。模型发布后的版本为占用版本加一，保持既有实际模型消费者的版本约定。

超过数据库时间 10 分钟的 `publishing` 占用可以通过实际资源版本 CAS 接管，原门禁保留为 `superseded`。陈旧占用或 `publish_unknown` 的新占用明确 `needs_reconciliation=true`：只允许核对已有实际基础表，表不存在时仍是结果待核对，不重复 `CREATE TABLE`。旧占用的迟到回执保留为独立未知发布记录，不能覆盖后来模型。实际结果完成后，服务再核验当前有效身份及 `models:read` 决定是否返回资源；读权限撤销不妨碍记录已发生的物理结果。

模型字段和绑定字段最多 1000 项；重复引用先按真实修订身份去重，最多 256 个不同修订。绑定载荷最多 256 KiB，发布占用及检查证据最多 1 MiB，实际引擎回执最多 512 KiB。超过容量明确拒绝，不能降级成未配置或伪造完成。

## 指标的固定版本消费

原子指标和复合指标的 `unit_id/unit_version` 必须成对。配置单位时，指标发布从真实标准仓储读取指定有效不可变修订，固定修订哈希和单位属性；旧有效修订可继续使用，新草稿不能替代既有固定版本。未配置单位的旧定义兼容保存，但不能声称通过了单位标准验证；已配置单位而仓储缺失时失败关闭。

指标冻结模型依赖时重新验证实际模型发布回执中的标准门禁、绑定哈希及全部有效固定修订。任务提交或重试、worker 执行前与运行期间及输出前、当前读者读取历史结果、受控指标查询授权均调用同一 `metrics::verify_current` 重建验证。标准撤回或模型定义改变后，不继续执行或释放旧结果；保留真实作业状态，不将授权拒绝伪造成原作业执行失败。

应用将同一标准仓储注入 `MetricsService`、`ProductJobService`、`QueryAuthorizationService` 的 `with_standard_repo`。无配置字段不等于已通过标准验证；实际模型回执明确记录 `not_configured` 或 `validated`。

## 采集后的检查范围

检查仅针对真实 `metadata_catalog` 证明对应的采集快照，固定 `model_version/binding_version/metadata_version`、三段 `catalog.schema.table` 身份、源指纹与观测时间。元数据对象和内部证明、实际模型发布回执必须一致；通用资源手工输入不能制造采集证明。

V1 `evaluation_scope=collected_schema`，结果 `conformant/non_conformant`，结构差异逐项记录。检查字段存在、额外字段、规范数据类型、有效空值约束和固定命名规则。返回明确 `data_values_evaluated=false`、`unit_semantics_evaluated=false`。采集快照观测时间不代表当前实时结构；检查不执行枚举内容扫描，也不验证计量单位业务意义。内容检查需要质量规则与真实质量 worker 的计数证据。

检查读取源快照后进行纯结构比较，持久化前同事务重新构建并比较快照哈希、当前权限及所有固定修订。源发生变化时失败关闭，不保存假成功。SQLite 和 PostgreSQL 使用一致语义。

## HTTP 与界面接线

- 标准动作：`POST /product/resources/:kind/:id/actions/validate|publish|revise|withdraw`。撤回载荷为 `{revision_version,expected_state_version,reason}`，资源 `expected_version` 保留外层。
- 历史：`GET /product/standards/:kind/:id/revisions?limit&offset`，只读真实已发布修订，不把草稿作为发布版本。
- 模型绑定：`GET/PUT /product/standards/consumers/data_model/:id/bindings`。写入请求包含模型与绑定 CAS、覆盖范围、列引用及显式升级。
- 实际结构：`POST /product/standards/consumers/data_model/:id/inspect`。
- `StandardsPage` 提供定义与发布、真实修订及带原因撤回；`ModelStandardBindingsPanel` 提供字段与命名修订选择、显式升级及实际结构检查。界面权限范围变化清空旧结果，刷新被拒后立即隐藏旧证据。

标准的 `unit convert` 与命名样本验证保留既有真实领域实现；版本动作仅负责上述发布生命周期，不能用通用状态按钮替代版本事务。模型列编辑器必须保留所有固定引用字段，不能默默丢弃它们。

## 交付与验证边界

新增 022 仓储负责不可变修订、外键绑定历史、真实发布占用和采集检查持久化。服务新文件、类型化 HTTP 接口、专页以及模型发布与指标单位消费者的接线已经提交，统一编译和定向 Rust 测试由根代理执行。

纯服务测试源覆盖正则完整匹配、版本配对、证据篡改、字段类型与主键空值、模型引用保留、物理回执及采集范围。仓储测试使用真实 SQLite 事务。前端定向测试覆盖版本深链接、当前授权拒绝、撤回 CAS、显式升级、真实结构错误和禁止冒充内容扫描。整体 POC 在功能模块完成后进行，不在本阶段启动服务。

独立 `model_publication_service_test` 通过生产 `ProductService.action`、真实 Sa 身份与 SQLite 标准仓储调用本地受控 tonic Flight SQL 协议夹具。六项测试源分别验证首次真实协议建表与回执、建表后核对失败及恢复不重建、恢复时表缺失不重建、实际字段类型不符、执行前标准撤回阻断引擎请求、建表后标准撤回保存实际副作用并记录未知结果。协议夹具提供受控 Arrow 观测，不代表真实 DuckDB/SwanLake 引擎 POC 已通过。
