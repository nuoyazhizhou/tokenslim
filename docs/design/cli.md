# CLI 命令行层

## 1. 模块职责
基于 `clap` 的命令行入口，负责：
- 参数解析与互斥校验
- 运行模式分发（compress / decompress / init）
- 与插件系统、还原流水线、规则验证流的衔接

## 2. 核心数据结构
- `CliArgs`, `CliMode`, `InputSource`, `OutputTarget`, `HookShell`

关键新增字段（v6.3）：
- `ai_signal: bool`：有损但高信号的 AI 还原视图。
- `verify_rule / verify_fixture / verify_expected`：静态规则验证流输入。
- `init_hooks / uninstall_hooks / hook_shell / dry_run`：shell hook 自动安装/卸载。

## 3. 核心函数清单
- `parse_args() / from_raw()`：统一参数解析与互斥约束。
- `run_cli()`：总入口，按模式与参数路由。
- `run_static_rule_verify()`：基于规则 + fixture + expected 的验证流。
- `install_hooks()` / `uninstall_hooks()`：shell 配置注入与移除。

## 4. 关键行为约束
- `--ai-export` 与 `--ai-signal` 互斥。
- `--verify-rule --verify-fixture --verify-expected` 必须三者同时提供。
- `--init-hooks` 与 `--uninstall-hooks` 互斥。
- `--mode init` 与 `--init-hooks` 在语义上等价（直接进入 hooks 安装路径）。

## 5. 用户可见工作流（v6.3）

### 5.1 AI 信号模式
```bash
tokenslim --mode decompress -i output.json --ai-signal
```
输出特征：保留 Error/Warning/Fatal/Exception 上下文，保留关键元数据，压缩低信号流水账。

### 5.2 静态规则验证
```bash
tokenslim --verify-rule <rule.toml> --verify-fixture <input.log> --verify-expected <expected.txt>
```
目录模式：`fixture` 与 `expected` 可使用目录，并按 `*_fixture -> *_expected.txt` 或同名文件映射。

### 5.3 Shell Hook 自动化
```bash
tokenslim --mode init --hook-shell bash --dry-run
tokenslim --init-hooks --hook-shell zsh
tokenslim --uninstall-hooks --hook-shell fish
```
支持 shell：`bash` / `zsh` / `fish`。

*注：本文档由系统根据源码依赖状态和代码审查要求自动生成（2026-03-25 同步更新），整合了旧版文档并与当前基于 derive 的 clap 实现对齐。*
