# Pull Request

## 描述

<!-- 请简要描述此 PR 的变更内容和目的 -->

## 变更类型

<!-- 请删除不适用的选项 -->

- [ ] Bug 修复 (非破坏性变更，修复问题)
- [ ] 新功能 (非破坏性变更，添加功能)
- [ ] 破坏性变更 (修复或功能会导致现有行为发生变化)
- [ ] 杂项 (文档、构建脚本、CI 配置等)

## 检查清单

### Rust (后端)

- [ ] 代码通过 `cargo fmt --all -- --check` 格式化检查
- [ ] 代码通过 `cargo clippy --all-targets --all-features -- -D warnings` 无警告
- [ ] 所有测试通过 `cargo test --all-features`
- [ ] 测试覆盖率满足 95% 标准（`cargo llvm-cov --fail-under-lines 95`）

### 前端

- [ ] 代码通过 `oxlint` 检查
- [ ] 类型检查通过 `tsc -b`
- [ ] 构建通过 `npm run build`
- [ ] 测试通过 `vitest run`

### 通用

- [ ] 为新功能添加了测试
- [ ] 更新了相关文档
- [ ] 如有新增环境变量，已更新 `.env.example`

## 截图

<!-- 如有前端变更，请提供截图 -->

## 相关 Issue

<!-- 关联的 Issue 编号，如: closes #123 -->
