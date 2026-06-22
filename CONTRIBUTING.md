# TokenSlim 贡献指南

感谢考虑为 TokenSlim 做贡献！本文档是一份**面向贡献者**的入口：指引你按正确顺序读文档、提 PR、跑测试。

---

## 1. 行为准则

本项目遵守以下原则：

- **建设性反馈** — Code review 时针对代码、不针对人。
- **小步快跑** — 大改动请先开 issue 讨论；小修复 / 新插件配置可以直接发 PR。
- **质量门禁** — 见 §3，改完必须跑过 4 步流水线才能收口。

---

## 2. 接手顺序

1. 读 [`README.md`](./README.md)（或 [简体中文版](./README.zh-CN.md)），了解用户入口和核心命令。
2. 读 [`docs/development/ARCHITECTURE.md`](./docs/development/ARCHITECTURE.md)，理解核心模块、CLI、插件和 Sidecar。
3. 读 [`docs/development/PLUGIN_DEVELOPMENT.md`](./docs/development/PLUGIN_DEVELOPMENT.md)，理解压缩协议 V1 的全部法则——所有 parser / rule 都必须遵守这套宪法。
4. 读 [`docs/development/TESTING.md`](./docs/development/TESTING.md)，理解改完代码后必跑的 4 步质量门禁。
5. 跑 `cargo test` 与 `cargo clippy`，确认本地基线通过。
6. 开始贡献。

---

## 3. 改完代码后必须跑什么

**详见 [`docs/development/TESTING.md`](./docs/development/TESTING.md)**。TL;DR：

```bash
# 1. 物理 case 质量门禁
tokenslim run python scripts/audit_sample_case_quality.py --plugin <plugin>

# 2. 压缩语义 + 冻结门禁
tokenslim run python scripts/audit_case_metrics.py --plugin <plugin> --version vYYYYMMDD_r1 --require-semantic-gate --fail-on-regression

# 3. 全插件健康检查（收口前必跑）
tokenslim run python scripts/audit_all_case_metrics.py --version vYYYYMMDD_r1 --require-semantic-gate --fail-on-regression --fail-on-frozen-change --fail-on-any-failure

# 4. 刷新能力索引
tokenslim run python scripts/generate_plugin_capability_index.py
```

**禁止**只跑 1-2 个就声称"已审计"。

---

## 4. 三种典型贡献路径

### 4.1 新增一个 case（最常见）

1. 在 `samples/<plugin>_plugin/` 下添加真实样本文件（**不要在测试里手写长字符串**）。
2. 在 `showcase.rs` 注册新 case。
3. `tokenslim run cargo test --lib <plugin>` 确认通过。
4. 按 §3 跑完整 4 步流水线。
5. PR。

### 4.2 新增一个插件

1. 创建 `src/plugins/<plugin>_plugin/` 目录与 plugin 入口。
2. 在 `config/plugins/<plugin>.json` 写配置。
3. 在 `src/plugins/mod.rs` 加 `pub mod <plugin>_plugin;`。
4. 在 `samples/<plugin>_plugin/` 至少放 1 个真实 case。
5. 在 `showcase.rs` 注册。
6. 按 §3 跑完整 4 步流水线。
7. PR。

### 4.3 修改已有插件的 parser / rule

1. 先 `tokenslim run cargo test --lib <plugin>` 确认现有测试全通过。
2. 改完后再次 `tokenslim run cargo test --lib <plugin>` 确认。
3. 按 §3 跑步骤 2-4 流水线。
4. 任何 `regressed` / `semantic_gate_failed` 必须修掉再发 PR。
5. PR。

---

## 5. 提交规范

- **Commit message 风格**：参考历史 commit，简洁说明 why 而不是 what。
- **PR 标题**：`<scope>: <summary>`，例 `web_log: handle Envoy W3C 4xx burst`。
- **PR 描述**：包含
  - 改了什么（1-2 句）
  - 关联 issue（如果有）
  - 跑了哪些验证（cargo test、4 步流水线、产物截图等）
  - 是否引入新 case / 新配置

---

## 6. 遇到问题怎么办

- **路由选错了插件** → `tokenslim run --explain-route -- <command>` 看为什么。
- **压缩产物不符合预期** → 翻 `docs/development/PLUGIN_DEVELOPMENT.md` 的法则 0–F + Non-VCS 聚合原则。
- **审计脚本报 `regressed` / `semantic_gate_failed`** → 看 `docs/audit/<plugin>/<plugin>.vX_r1.diff.md` 定位回退。
- **遇到文档/脚本不一致** → 直接开 issue，PR 修文档同样欢迎。

---

## 7. 许可证

贡献的代码默认按 [MIT](./LICENSE) 协议授权。提交 PR 即表示同意。
