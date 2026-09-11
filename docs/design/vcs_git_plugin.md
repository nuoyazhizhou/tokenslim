# VCS Git Plugin

## 1. 简介
`vcs_git_plugin` 是从原 `vcs_plugin` 中彻底剥离出的纯 Git 侧解析器实现。该插件遵循零耦合原则，内置了专属的文本摘要与解析逻辑，用于提升大语言模型 (LLM) 对 Git 命令输出的理解效率并极致压缩 Token 占用。

## 2. 支持的 Git 命令与 Parser
目前全面支持并分类解析以下 Git 命令及其变体：
- **状态类 (Status Profile)**: `git status`, `git checkout`, `git restore`, `git switch`, `git clean`
- **日志类 (Log Profile)**: `git log`, `git show`, `git pull`, `git push`, `git fetch`, `git merge`, `git cherry-pick`, `git rebase` (包括交互式 `-i`), `git revert`, `git reset`, `git bisect`, `git reflog`, `git branch`
- **差异类 (Diff Profile)**: `git diff`, `git diff --cached`, `git diff HEAD`, `git diff --stat` 等
- **其他类**: `git worktree`, `git grep`, `git submodule` 以及各种不产生有效输出的命令 (如 `git tag`, `git add`, `git rm`)

## 3. 核心压缩策略
- **Diff/Patch 提取**: 从 standard diff 格式中剥去视觉分隔符和 `@@` 行中的无效内容，提取两侧变更路径。
- **状态符号映射**: 识别如 `M `, `A `, `D `, `??` 等标准/简短状态，映射为核心文件的变更流。
- **Rebase Todo 清理**: 丢弃交互式 rebase 中的大段帮助注释，仅保留有效的 `pick`, `reword` 等动作流并简化警示信息。
- **Fetch/Push 压缩**: 精简网络传输打印（如 "Unpacking objects", "Enumerating objects"）等纯数字进度日志，提取更新的引用变动。
- **智能锚点注入**: 对 `checkout` 和 `switch` 生成 `branch:` 或 `prev:` 等语义化标记。

## 4. 正则表达式保留说明
对于专属的解析，以下正则表达式被深度保留并在 `methods.rs` 及 `parser.rs` 中专司其职：
- 路径与状态正则：负责高速检测行首的 `modified:`, `new file:`, 或短格式 `M  `.
- 人类可读体积转换正则：用于将特定的数字+Bytes 转换为通用的 `KB`/`MB` 短格式，服务于 `git --stat` 等附带体积的输出（虽部分集成于其他层，但在内部处理时一并缩减）。
