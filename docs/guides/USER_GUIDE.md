# TokenSlim 使用手册

> 面向第一次使用 TokenSlim 的开发者。  
> 最后更新: 2026-06-21  
> 配套：[5 分钟 Quickstart](./QUICKSTART.md) · [SDK 使用文档](./SDK_USAGE.md)

---

## 一、TokenSlim 解决什么问题

TokenSlim 用于压缩冗长的开发输出，例如 VCS 日志、编译器错误、构建流水线、测试结果、数据库日志和结构化文本。目标是在保留关键语义的同时减少给 LLM 的输入体积。

**节省区间**（实测）：

| 输入类型 | 节省率 | 例子 |
|---|---|---|
| 编译错误（gcc / rustc） | 80%–95% | 5000 行 cargo test 输出 → 200 行 |
| 大型 git log | 70%–90% | 200 个 commit → 20 行 |
| Android/Gradle 构建 | 60%–85% | 100K 行 → 5K 行 |
| 重复结构化日志（K8s / cloud） | 80%–95% | 1000 条 access log → 50 条 + 计数 |
| pytest / 单元测试 | 50%–80% | 长 traceback 折叠 |
| YAML / JSON 配置 | 40%–70% | 大量重复 key 字典化 |

当前能力概览：

- **55 个插件**（14 个 VCS、1 个统一 VCS、40 个 non-VCS）
- **1107 个测试样本**全审计通过并冻结
- **3 个语言 SDK**（Node.js / Python / Java）

---

## 二、5 分钟上手（最常用 3 个命令）

### ① 包装命令并压缩

```bash
tokenslim run git status
tokenslim run cargo test
tokenslim run pytest
tokenslim run cmake --build build
```

### ② 起服务给 SDK / IDE 扩展用

```bash
TOKENSLIM_PORT=10086 tokenslim-server
```

### ③ 压缩文件到 JSON

```bash
tokenslim -i build.log -o output.json --preset balanced
```

详细分步说明见 [QUICKSTART.md](./QUICKSTART.md)。

---

## 三、4 个常见场景（实战模板）

### 场景 A：把 git log 喂给 LLM 写 changelog

```bash
# 1) 拉 200 条 commit
git log --oneline -n 200 > /tmp/gitlog.txt

# 2) 压缩（自动选 vcs_git_plugin）
tokenslim -i /tmp/gitlog.txt -o /tmp/gitlog.slim.json --preset ai

# 3) 把压缩串 + 字典送进 LLM prompt
cat /tmp/gitlog.slim.json
```

```ts
// 在代码里：
import { TokenSlimClient } from 'tokenslim';
const slim = new TokenSlimClient();
const r = await slim.compress(rawGitLog, { plugin_hint: 'vcs_git_plugin' });
const prompt = `以下是压缩后的 git log（节省 ${(100 - r.ratio * 100).toFixed(1)}% token）：\n${r.compressed}\n\n请按 semver 规范生成 changelog。`;
```

---

### 场景 B：CI 流水线失败日志 → 自动诊断

```bash
# CI 日志通常上千行，大部分是 progress
cargo test --no-fail-fast 2>&1 | tee /tmp/test.log
tokenslim -i /tmp/test.log -o /tmp/test.slim.json --preset ai
```

**保留什么**（实测）：

- ❌ FAILED 的 test 名称 + 文件:行号
- ❌ `error[E0xxx]:` 错误码和消息
- ❌ `panicked at` panic 位置
- ✅ 压缩 `Compiling xxx v0.1.0` 进度行（折叠为计数）
- ✅ 压缩 `running N tests` / `test result: ok` 重复行

效果：1 万行 cargo 输出 → 50 行结构化诊断，LLM 一眼能定位失败。

---

### 场景 C：K8s / Cloud 日志查 5xx 突发

```bash
# 1000 条 access log，99% 是 200，只有 5 条 503
kubectl logs -l app=api --tail=1000 > /tmp/access.log
tokenslim -i /tmp/access.log -o /tmp/access.slim.json
```

**保留什么**：

- ❌ 5xx / 4xx 全保留 + 路径 + 时间
- ✅ 200 健康检查聚合为 `[HEALTH] x=N, 99% 2xx`
- ✅ IP / UA / path 字典化（`$P1=10.0.0.1`、`$W/U2=curl/7.79`）

效果：1000 行 → 30 行，重点全在。

---

### 场景 D：复用 LLM 输出回写（round-trip）

```bash
# 压缩
tokenslim -i build.log -o out.json --preset balanced
# 解压（保证 round-trip 安全）
tokenslim decompress -i out.json -o roundtrip.txt
diff build.log roundtrip.txt   # 应该完全一致（或仅去噪部分）
```

round-trip 安全：TokenSlim 不会偷偷改字面量，只折叠、字典化、聚合。

---

## 四、常用命令清单

### 包装命令

```bash
tokenslim run <cmd>                          # 通用包装
tokenslim run --explain-route -- <cmd>       # 只看路由决策，不执行
tokenslim --preset ai -- <cmd>               # 强制 AI 预设
tokenslim --preset ai --format text -- <cmd> # 输出格式 text / json
```

### 服务

```bash
TOKENSLIM_PORT=10086 tokenslim-server                 # 起 HTTP server
TOKENSLIM_HOST=0.0.0.0 TOKENSLIM_PORT=10086 tokenslim-server  # 监听所有网卡
```

### 文件级

```bash
tokenslim -i in.log -o out.json              # 压缩文件
tokenslim -i in.log -o out.json --preset ai  # AI 预设
tokenslim decompress -i out.json -o back.txt # 解压
tokenslim decompress -i out.json --ai-export # 导出 AI 可读版
```

### 诊断

```bash
tokenslim encoding --format text             # 编码自检
tokenslim workspace --format llm             # 输出 LLM 友好的 workspace 描述
tokenslim workspace --inject                 # 注入到 .tokenslim-context.md
tokenslim plugins                            # 列出所有插件
tokenslim explain-plugin vcs_git_plugin      # 解释某个插件怎么压
```

### 节省统计

```bash
tokenslim gain                              # 总计
tokenslim gain --daily                      # 按天
tokenslim gain --by-filter                  # 按插件家族
tokenslim gain --json                       # JSON 输出
```

---

## 五、预设（preset）选择

| 预设 | 节省率 | 保真度 | 适用场景 |
|---|---|---|---|
| `lossless` | 0%–30% | 100% | 不能丢任何字（合规 / 审计） |
| `balanced` | 40%–70% | 99% | 默认；通用 LLM 喂入 |
| `ai` | 70%–95% | 95% | 给 LLM 二次推理时追求极致节省 |

选错预设的代价：太激进可能让 LLM 误判上下文。

---

## 六、插件覆盖速查

| 场景 | 主要插件 |
| ---- | -------- |
| VCS | `vcs_git_plugin`, `vcs_svn_plugin`, `vcs_hg_plugin`, `vcs_p4_plugin`, `vcs_gh_plugin`, `vcs_glab_plugin`, `vcs_az_plugin`, `vcs_bitbucket_plugin`, `vcs_gerrit_plugin`, `vcs_repo_plugin`, `vcs_bzr_plugin`, `vcs_cvs_plugin`, `vcs_darcs_plugin`, `vcs_fossil_plugin` |
| 构建/编译 | `gcc_log_plugin`, `rust_go_plugin`, `maven_plugin`, `android_gradle_plugin`, `bazel_plugin`, `dotnet_plugin`, `xcode_log_plugin`, `webpack_vite_plugin` |
| 测试 | `pytest_plugin`, `ndjson_plugin`, `nodejs_plugin`, `maven_plugin` |
| DevOps/IaC | `terraform_plugin`, `ansible_plugin`, `pulumi_plugin`, `cloudformation_plugin`, `helm_plugin`, `kubernetes_docker_plugin`, `ci_log_plugin` |
| 数据库/日志 | `db_log_plugin`, `web_log_plugin`, `syslog_plugin`, `cloud_log_plugin` |
| 运行时错误 | `java_stack_plugin`, `node_error_plugin`, `python_traceback_plugin`, `php_ruby_plugin` |
| 结构化文本 | `json_plugin`, `yaml_plugin`, `xml_html_plugin`, `markdown_plugin`, `protobuf_plugin`, `smart_code_plugin`, `artifact_summary_plugin` |
| Shell | `shell_session_plugin`, `generic_text_plugin` |
| 工具 | `noise_filter_plugin`, `ansi_cleaner_plugin`, `smart_path_plugin`, `static_rule_plugin`, `template_driven_plugin`, `unity_unreal_plugin` |

完整列表见 [plugins_registry.md](../../plugins_registry.md)。

---

## 七、如何验证输出

每跑一次压缩，都建议：

1. **看压缩率** — 输出文件大小 / 输入文件大小，应在常见区间内（见第一节）。
2. **回放验证** — `tokenslim decompress -i output.json -o roundtrip.txt` 还原，对比原输入。
3. **AI Export 复核** — `tokenslim decompress -i output.json --ai-export` 导出 AI 友好版，对 error/warning 上下文做人工 spot check。
4. **看 showcase case** — `samples/<plugin>_plugin/` 里有该插件的标杆样本，作为期望输出的对照。

如果某次修改影响插件输出，跑回归流水线（见 [TESTING.md](../development/TESTING.md)）：

```bash
tokenslim run powershell -File scripts/audit_all_case_metrics.ps1 -RequireSemanticGate -FailOnAnyFailure
```

---

## 八、SDK 与扩展

| 入口 | 文档 |
|---|---|
| Node.js / TypeScript | [packages/sdk-nodejs/](../../packages/sdk-nodejs/) · npm: `tokenslim` |
| Python | [sdk/python/tokenslim_sdk.py](../../sdk/python/tokenslim_sdk.py) |
| Java | [sdk/java/TokenSlimClient.java](../../sdk/java/TokenSlimClient.java) |
| VSCode 扩展 | [vscode-extension/](../../vscode-extension/) |
| Chrome 扩展 | [chrome-extension/](../../chrome-extension/) |
| JetBrains 插件 | [jetbrains-plugin/](../../jetbrains-plugin/) |
| SDK 三语言详解 | [SDK_USAGE.md](./SDK_USAGE.md) |

---

## 九、更多入口

| 需求 | 文档 |
| ---- | ---- |
| 项目总览 | [README.md](../../README.md) |
| 5 分钟上手 | [QUICKSTART.md](./QUICKSTART.md) |
| SDK 使用 | [SDK_USAGE.md](./SDK_USAGE.md) |
| 架构设计 | [docs/development/ARCHITECTURE.md](../development/ARCHITECTURE.md) |
| 插件开发 | [docs/development/PLUGIN_DEVELOPMENT.md](../development/PLUGIN_DEVELOPMENT.md) |
| 插件注释速查 | [docs/development/PLUGIN_COMMENT_REFERENCE.md](../development/PLUGIN_COMMENT_REFERENCE.md) |
| 文档组织 | [DOCS_ORGANIZATION.md](../../DOCS_ORGANIZATION.md) |
| GitHub | <https://github.com/nuoyazhizhou/tokenslim> |
