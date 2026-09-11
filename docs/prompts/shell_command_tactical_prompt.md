# Shell Command Compression Tactical Prompt

用途：指导开发 Windows `cmd.exe`、PowerShell、Linux/macOS `bash`、`zsh`、`fish` 等通用 shell 命令输出压缩插件。

本文件是开发战术提示词，不是 LLM 语义门禁的 active profile。语义门禁请使用 `semantic_audit_profiles.md` 中的短 profile。

## 目标边界

- 输入对象是“交互式或脚本化 shell 命令执行 transcript”，包括命令行、stdout、stderr、退出码、提示符、工作目录、环境变量片段和 shell 报错。
- 适用 shell：Windows `cmd.exe`、PowerShell、PowerShell Core、POSIX `sh/bash/zsh/fish`、Git Bash、WSL、CI runner shell。
- 不替代专门插件：如果输出主体明显是 `git`、`cargo`、`maven`、`kubectl`、`pytest`、`nginx` 等已支持工具，应优先路由到对应专用插件；shell 插件只负责 shell 包装层、通用命令和路由兜底。

## 绝对保留项

- 第一条可识别命令必须作为输出第一行保留；包含提示符时可压缩提示符，但不能丢命令文本。
- 保留 shell 类型线索：PowerShell `PS C:\...>`、cmd `C:\...>`、bash/zsh `$`/`#`、CI `+ command`、脚本 shebang、`.ps1/.bat/.cmd/.sh` 文件名。
- 保留工作目录、用户/主机、虚拟环境、容器/WSL/CI runner 等会影响命令语义的上下文。
- 保留环境变量赋值、参数、flag、子命令、管道、重定向、here-doc、glob、引号、转义、命令替换和后台执行标记。
- 保留 stdout/stderr 的区别；如果原文可区分，压缩后也必须可区分。
- 保留退出状态：`exit code`、`ERRORLEVEL`、`$LASTEXITCODE`、signal、timeout、canceled、killed、segfault。
- 保留所有错误信号：command not found、permission denied、access denied、no such file、syntax/parser error、parameter binding error、execution policy、encoding/mojibake、path too long、glob/no match、pipe broken。

## 推荐压缩形态

保持首行命令原样或近似原样，然后按需追加短字段：

```text
<original command line>
SH:<ps|cmd|bash|zsh|fish|sh|unknown> CWD:<path> EXIT:<code|signal|unknown>
OUT:<summary or representative stdout>
ERR:<error class>: <message>
```

当输出很短时，不要强行结构化；优先使用 `prefer_non_expanding(raw, compacted)` 回退到原文或更短表达。

空成功输出必须防歧义：

```text
<original command line>
EXIT:0 ST:[CLEAN]
```

失败但无 stdout 时必须保留错误：

```text
<original command line>
EXIT:127 ERR:command-not-found: foo
```

## Shell 家族专项规则

### Windows cmd.exe

- 保留 `ERRORLEVEL`、批处理标签、`call`、`set`、`setlocal/endlocal`、`for /f`、`if errorlevel`、`%VAR%` 展开语义。
- 保留 `The system cannot find the path specified`、`is not recognized as an internal or external command`、`Access is denied` 等原生错误核心句。
- 路径压缩前必须识别 Windows 盘符、UNC 路径和带空格路径；不要把 `C:\` 当作协议。

### PowerShell

- 保留 cmdlet 名、参数名、错误类别、`CategoryInfo`、`FullyQualifiedErrorId`、脚本路径和行列号。
- `ParserError`、`ParameterBindingException`、`UnauthorizedAccess`、`ExecutionPolicy`、`CommandNotFoundException` 必须显式保留。
- 表格输出可压缩列宽，但列名和行含义必须可恢复。
- 保留 `$LASTEXITCODE` 与 PowerShell 异常的区别；二者不是同一种失败。

### POSIX sh/bash/zsh/fish

- 保留 `set -e/-u/-o pipefail`、subshell、pipeline、redirection、heredoc、glob、alias/function、trap/signal 等影响执行结果的结构。
- 保留 `command not found`、`permission denied`、`No such file or directory`、`syntax error near unexpected token`、`bad substitution`、`unbound variable`。
- CI trace 中以 `+ command` 展开的命令可以去掉重复 prompt，但必须保留真实执行命令和失败点。

## 聚合与去噪

- 可删除 spinner、progress bar、重复 banner、颜色码、提示符重复、无意义空行和纯装饰分隔线。
- 可聚合重复成功行，例如大量 `rm`、`copy`、`mkdir`、`chmod`，但必须保留数量、代表路径和失败项。
- 不得让少数失败被多数成功淹没；失败项必须独立输出 `ERR` 或 `!` 标记。
- 对长路径使用路径字典压缩；避免把 URL、邮箱、命令选项误识别为路径。

## 路由建议

- 如果第一行命令是专门工具且已有插件，优先专用插件：`git`、`svn`、`hg`、`p4`、`cargo`、`mvn`、`gradle`、`kubectl`、`docker`、`pytest` 等。
- 如果 transcript 包含多个异构命令、shell 自身失败、脚本 glue 失败、CI wrapper 失败，适合 shell 插件。
- 如果输出只有纯文本文件内容且没有命令上下文，应退给 `generic_text` 或对应格式插件。

## 审计 checklist

- G4：压缩输出第一行的命令 token 必须与原始第一条命令一致。
- G3：原文含 error/fatal/failed/denied/not found/exception/parser/exit non-zero 时，压缩后必须保留对应错误信号。
- 空输入/空输出：只有明确的空输入或成功无输出 case 才允许压缩成 `ST:[CLEAN]`。
- ROI：结构化标签导致膨胀时必须退回更短表达。
- 语义：LLM 看到压缩文本后，应能判断“运行了什么、在哪里运行、成功还是失败、失败原因、下一步该查哪里”。
