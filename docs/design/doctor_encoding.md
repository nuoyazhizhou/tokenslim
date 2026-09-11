# Doctor Encoding 模块设计文档

## 概述

`encoding` 是 TokenSlim 的环境诊断工具之一，用于检测操作系统、Shell、代码页、PowerShell、Python、Node.js、JDK 的编码配置，识别可能导致乱码的风险，并生成可执行的修复建议。

## 模块结构

```
src/core/doctor_encoding/
├── mod.rs      # 模块导出
├── types.rs    # 类型定义
└── methods.rs  # 核心逻辑
```

## 类型定义

### EncodingRiskLevel

风险等级枚举：`Ok` | `Warn` | `Fail`

### 信号结构

- `OsSignal`: 操作系统信息（name, version, locale）
- `ShellSignal`: Shell 信息（name, raw, host）
- `CodepageSignal`: 代码页信息（value, is_utf8）
- `RuntimeSignal`: 运行时信息（detected, version, note）— 复用给 PowerShell/Python/Node/JDK

### EncodingDoctorReport

总报告结构，包含 risk + 各信号 + recommendations。

## 核心逻辑

### 检测流程

1. **OS 检测**: 复用 `sys_env::get_environment_info()`
2. **Shell 检测**: 读取环境变量（POWERSHELL_DISTRIBUTION_CHANNEL, ComSpec, SHELL 等），优先强证据
3. **Codepage 检测**: Windows 下尝试读取 chcp 输出
4. **运行时检测**: 通过 `Command::new(cmd).args(["--version"])` 探测 PowerShell/Python/Node/JDK

### 风险判定规则

| 风险等级 | 触发条件 |
|---------|---------|
| FAIL | Windows codepage 非 UTF-8 + 多运行时编码不明确 |
| WARN | 环境混合（locale 与 shell 编码不一致）或关键信号未知 |
| OK | UTF-8 证据链完整 |

### 建议生成

基于风险和信号生成 3-6 条可执行建议，包括 codepage 切换、PowerShell profile 配置、Python/Node/JDK 编码参数等。

## 输出格式

- **Text**: 面向人类，包含 Summary/Signals/Recommendations 三个段落
- **JSON**: 面向机器，固定 schema，便于 CI/IDE 消费

## CLI 接口

```bash
tokenslim encoding                           # 默认 text
tokenslim encoding --format text             # 文本报告
tokenslim encoding --format json             # JSON 报告
tokenslim encoding --fix                     # 生成修复命令
```

## 测试

`tests/doctor_encoding.rs` 覆盖：
- 风险分级（OK/WARN/FAIL）
- JSON shape 稳定性
- Text 输出结构

## 后续演进

- `--fix` 自动修复（需用户确认）
- 与 `env` 融合形成统一环境健康报告
