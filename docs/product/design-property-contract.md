# 门户、BI 与 API 设计属性合同

日期：2026-10-08，Asia/Shanghai。本文件是本产品采用的实施合同。原型库存确认门户设计/授权、BI 分析、数据服务等产品意图，但没有恢复这些属性的精确 JSON 或全部交互，因此下列结构不能称作从 Axure 原型精确还原的格式。

## 门户布局与主题

`portal.layout` 固定为：

```json
{
  "columns": 12,
  "widgets": [
    {
      "id": "widget-1",
      "type": "task_list",
      "title": "任务运行",
      "module_id": "integration-tasks",
      "width": 6,
      "height": 2
    }
  ]
}
```

- 栅格固定 12 列，组件按数组顺序布局；宽度为 1–12 的整数，高度为 1–8 的整数。最多 48 个组件；发布必须存在至少一个可用组件。空布局可以作为编辑草稿。
- 类型仅允许 `metric/task_list/datasource_list/announcement/chart/shortcut`。组件 ID 非空、最多 80 字符且在门户内唯一；标题非空且最多 120 字符。
- `module_id` 是产品目录中的稳定 `module.id`，不是路径、URL 或 JavaScript。前端提供真实目录选择，后台发布时再次校验模块可用性和发布者授权；运行时每个组件仍需要检查当前访问者的业务权限。
- 数组上移/下移不修改组件 ID；删除只删除该组件。布局没有恢复任意第三方组件、拖拽脚本或用户代码能力。
- 设计器仅预览位置、尺寸、类型和配置来源，不读取实际指标，不生成示例数据。真实内容需要另行接通门户运行接口。

`portal.theme` 固定为 `{primary_color,background_color,title}`，颜色只接受完整 `#RRGGBB` HEX（大小写均可），标题非空且最多 120 字符。禁止额外属性和任意 CSS、HTML、脚本。预览只在 HEX 验证通过后使用颜色；非法输入展示校验提示，使用安全预览色，不把原始字符串注入样式。

## BI 定义与图表

`bi_report.dimensions` 为字段名称数组：`["department","period"]`。名称非空、最多 128 字符，去除两端空格后按不区分大小写检查重复。维度必须来自实际模型或查询返回字段，保存声明不证明查询已经返回这些字段。

`bi_report.measures` 为：

```json
[
  {"name":"employee_count","expression":"employee_id","aggregate":"count"}
]
```

名称非空、最多 128 字符且不重复；源表达式非空、最多 2048 字符；聚合仅允许 `sum/avg/count/min/max`。表达式是受控 SQL 业务定义，不能作为任意代码执行。执行阶段需要语法、字段、授权与计算口径验证。

`bi_report.chart_config` 固定为：

```json
{
  "type": "bar",
  "title": "部门人员分析",
  "x_field": "department",
  "y_field": "employee_count",
  "legend": true
}
```

类型仅允许 `bar/line/pie/table`；标题非空且最多 120 字符；`legend` 必须是布尔。柱状图、折线图、饼图需要非空分类字段和度量字段；表格可以留空轴字段，使用实际返回列。已配置维度/度量时，从相应数组选择并校验引用；尚未声明时可填写实际返回字段名称，发布/执行仍需核对实际结果 schema。

图表结构预览只展示轴、字段、图例位置或表头，不绘制模拟数值、随机趋势、百分比或示例人员。执行器返回真实结果后，才可由运行界面绘制数据。

## API 返回结构

`data_api.response_schema` 为：

```json
[
  {"name":"employee_count","data_type":"BIGINT","nullable":false,"description":"人员数量"}
]
```

设计器复用模型字段编辑器，输出 `name/data_type/nullable/description`，不输出旧字段 `type`。字段名必须非空且不重复，SQL 类型需要后台按支持类型验证。声明与查询结果在发布以及执行阶段核对名称、类型和空值语义；差异需要明确错误，不能用声明造出返回行。

API 输入参数、门户角色授权由另外的业务编辑器负责，本合同不改变它们的值格式。

## 接入与验收

前端入口 `DesignPropertyEditor` 精确匹配：`portal.layout/theme`、`bi_report.chart_config/dimensions/measures`、`data_api.response_schema`。接口为 `{kind,fieldKey,value,onChange,disabled?,properties?}`。`properties` 用于获取同一 BI 草稿中的维度/度量，必须由上层传入当前编辑值，不能用陈旧服务器快照。

纯模型校验位于 `web/src/product/design-editors/design-models.ts`。前端校验用于编辑反馈；后端仍必须使用相同合同进行权威校验。编辑中允许暂时不完整字段，保存/发布需完成相应校验；规范化读取只用于安全渲染，不证明原始属性合法。

本轮验收包括组件添加/删除/顺序和尺寸边界、HEX/CSS 注入拒绝、图表字段必需条件与引用、结构化 BI 声明、API 返回字段格式和专属 dispatch。未启动平台、未执行 SQL、未进行门户发布、图表真实数据或 API 物理调用验收；这些闭环属于后端实现后及最终整体 POC。
