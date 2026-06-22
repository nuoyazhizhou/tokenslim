### android_gradle_plugin
Android/Gradle日志脱水：保留错误Task和资源警告，折叠重复Task状态行，压缩构建路径和环境变量。
## 保留信号
- > Task ... FAILED 行（构建失败的任务）
- warn: removing resource 行（资源删除警告）
- WORKSPACE 等环境变量行（Jenkins关键变量）
## 压缩目标
- 非FAILED的Task行（UP-TO-DATE等）折叠为计数
- 连续5个以上相同包名的资源警告合并
- 构建路径替换为$GRADLE令牌字典

### ansi_cleaner_plugin
ANSI脱水：保留文本内容，移除ANSI控制码，压缩进度条覆盖，丢弃空行。
## 保留信号
- 非空文本行（保留有效内容）
- 进度条最后有效状态（保留最后一次更新）
## 压缩目标
- ANSI转义序列（如\x1b[31m）
- 回车符\r导致的进度条历史记录（仅保留最后一行）
- 空白行（剥离后空行丢弃）

### ansible_plugin
Ansible脱水：保留任务头和主机状态，折叠重复任务细节，压缩语法错误。
## 保留信号
- TASK [名称] 行（任务标题）
- RUNNING HANDLER [名称] 行（处理程序标题）
- ok/changed/failed/unreachable/skipping: [主机] 行（主机状态）
- PLAY RECAP 行（汇总）
- ERROR! 语法错误行（压缩后的错误信息）
## 压缩目标
- 重复的任务输出折叠为单行摘要
- 主机列表合并为范围格式如 host[1,2]
- 详情中的 msg 字段提取并压缩
- 语法错误多行压缩为单行
- 空行和无关注释行删除

### artifact_summary_plugin
构建产物摘要脱水：保留测试失败/错误和 SARIF 关键信号，折叠冗余的测试用例细节和 SARIF 结果条目。
## 保留信号
- 测试套件名称（JUnit 套件标识）
- 测试用例名称（含类名）
- 测试状态（失败/错误/跳过）
- SARIF 规则ID（安全规则标识）
- 严重级别（SARIF 严重性等级）
- 源位置（SARIF 问题位置）
- 工具名称（扫描工具）
## 压缩目标
- 原始 XML/JSON 文本（替换为紧凑摘要）
- 长字符串（使用字典引擎压缩重复令牌）
- 通过状态的测试用例（可能丢弃，仅保留失败/错误/跳过）
- 冗余的 SARIF 结果条目（聚合为摘要）

### bazel_plugin
Bazel输出脱水：保留错误行和关键构建摘要，折叠普通INFO日志，压缩目标列表。
## 保留信号
- error: 行（编译错误）
- BUILD 完成行（如 Build completed）
- INFO: Analyzed 行（分析摘要）
- bazel version 摘要行（版本信息）
- bazel query 目标行（查询结果）
## 压缩目标
- 普通 INFO 日志行（折叠丢弃）
- 冗长的目标列表（压缩为 TARGETS[count]）
- 重复行（去重）

### ci_log_plugin
CI/CD日志脱水：保留步骤、错误、警告和状态信号，折叠详细日志行为统计摘要。
## 保留信号
- ::error 行（错误信号）
- ::warning 行（警告信号）
- ::group::/section_start: 行（步骤起始）
- ::endgroup::/section_end: 行（步骤结束）
- ##[error] 行（Azure错误）
- finished: failure 行（Job失败）
- process completed with exit code 行（退出码）
## 压缩目标
- 步骤内的非关键日志行（折叠为行数统计）
- 缓存操作行（压缩为缓存计数）
- 重试操作行（压缩为重试计数）
- 空行（完全丢弃）

### cloud_log_plugin
云日志脱水：保留命令提示和记录结构，折叠元数据字段，压缩长值和时间。
## 保留信号
- command_lines（命令提示行，如aws logs tail）
- records中的消息文本
- passthrough行（未被识别的行）
- CSV输出或访问摘要（如成功渲染）
## 压缩目标
- 长结构化字段值截断为<TRUNCATED>（超过360字符）
- 示例字段（samples）压缩为摘要格式[pos:shop:price eta= rating=]
- 时间字段精简为'日期 时间'（去掉毫秒和时区）
- 资源路径缩短为前两部分+后8字符（如a/b/c...）
- 多余空白字符合并为单个空格

### cloudformation_plugin
CloudFormation事件行脱水：保留失败/回滚事件，折叠重复状态，压缩冗长输出。
## 保留信号
- 事件行（状态+资源ID）
- 失败/回滚行（FAILED或ROLLBACK状态）
- 错误信号行（keep_error_signal保留）
## 压缩目标
- 空行或全分隔符行
- 锚行（首行非空内容）
- 重复状态事件行（按状态计数折叠）

### db_log_plugin
数据库日志脱水：保留进程ID、持续时间、日志级别、查询标签等关键字段，折叠冗余信息，压缩为紧凑格式。
## 保留信号
- pid（进程ID）
- duration（持续时间毫秒）
- level（日志级别，如LOG、ERROR）
- query（压缩后的查询标签）
- msg（日志消息主体）
## 压缩目标
- 原始时间戳（被移除）
- 原始长查询（被compact_query_label截断或替换）
- 普通duration（仅保留数字，标记SLOW或DUR）
- ROI门控：若压缩后体积更大则回退原文

### dotnet_plugin
.NET构建日志脱水：保留错误定位和堆栈帧方法名，折叠冗余参数。
## 保留信号
- at method(...) in file:line（堆栈帧行）
- file(line,col): error code: message（MSBuild错误行）
- 包含System./Microsoft.的行（疑似.NET相关）
## 压缩目标
- 堆栈帧参数列表（替换为(...)）

### gcc_log_plugin
GCC/Clang编译日志脱水：保留错误、警告和关键信息，折叠重复警告，压缩长路径和宏定义。
## 保留信号
- error: 行（编译错误）
- warning: 行（相同警告类型的前 N 次）
- note: 行（编译提示）
- undefined reference 行（链接器错误）
- Build files have been written to: 等关键行
- CMake Error 等构建错误行
- Error 1 或 Error 2 结尾的行
## 压缩目标
- 长路径替换为 $GCC 令牌字典
- 宏定义（-D...）替换为令牌
- 相同 warning 超过阈值的后续行（折叠）
- 重复行去重（threshold=1）

### generic_text_plugin
通用文本脱水：保留原始文本内容，折叠连续空白行，压缩 ANSI 控制序列和回车重绘。
## 保留信号
- 非空白行文本内容（保留所有有效行）
## 压缩目标
- ANSI 转义序列（移除）
- 连续空白行（折叠为单行）
- 回车重绘行（只保留最后一段）
- 制表符（可选替换为空格）
- 行尾空白（可选修剪）

### git_diff_plugin
Git diff脱水：保留diff header和hunk header，折叠非关键行，压缩文件路径。
## 保留信号
- diff --git行（文件差异标记）
- --- a/行（旧文件路径）
- +++ b/行（新文件路径）
- HUNK_HEADER行（块头部）
- index行（文件索引信息）
## 压缩目标
- 非header行（如上下文内容）
- 文件路径简化（通过token_prefix）

### helm_plugin
Helm输出脱水：保留关键字段和资源类型，折叠重复资源，压缩空格和丢弃非核心行。
## 保留信号
- NAME: 行（资源名称）
- LAST DEPLOYED: 行（部署时间）
- NAMESPACE: 行（命名空间）
- STATUS: 行（状态）
- REVISION: 行（版本号）
- TEST SUITE: 行（测试套件）
- Last Started: / Last Completed: / Phase: 行（状态字段）
- deployment/ / service/ / configmap/ / secret/ 资源行（去重保留）
- error: 行（错误信息）
## 压缩目标
- 行内多余空格（compact_spaces 压缩）
- 重复的资源行（BTreeSet 去重）
- 非字段、非资源、非错误的普通输出行（丢弃）

### java_stack_plugin
Java异常堆栈脱水：保留关键异常类和帧，折叠重复和深层堆栈，压缩类名和路径。
## 保留信号
- 异常行（Exception in thread, Exception, Error等）
- 常见异常类名（java.lang白名单）
- Caused by 行（异常链）
- 堆栈帧前N行（通过阈值）
## 压缩目标
- 重复相同异常堆栈（超过阈值折叠为[DUPLICATE]）
- 深层堆栈超出N的行（截断并添加摘要）
- 抑制异常（suppressed exceptions）压缩为简短形式
- 异常类名（非白名单）用字典编码为$JEX令牌
- 堆栈帧类名和方法用$JST令牌编码
- Caused by 类名用$JCB令牌编码

### json_plugin
JSON脱水：保留JSON结构，折叠长字符串和路径，压缩为紧凑格式并添加前缀。
## 保留信号
- { } 对象结构
- [ ] 数组结构
- 数字/布尔/null 值
- 短字符串（长度≤max_string_val_len）
- 键名（未启用字典化时）
## 压缩目标
- 长字符串（长度>max_string_val_len）替换为字典令牌
- 键名（启用dictionaryize_keys时）替换为字典宏
- JSON文本压缩为单行紧凑格式
- 短JSON通过ROI门控避免膨胀

### kubernetes_docker_plugin
Kubernetes/Docker日志脱水：保留关键事件和结构，折叠容器ID和Pod元数据，压缩为令牌字典。
## 保留信号
- K8S_POD 正则匹配的行（命名空间/Pod）
- DOCKER_ID 正则匹配的容器 ID
- Docker CI 输出（如 'Step X/Y'）
- Kubernetes CI 输出（如 'kubectl' 命令）
- JSON 对象含 'message' 或 'logGroup'
## 压缩目标
- 容器 ID 替换为短令牌（$D）
- Pod 名称/命名空间替换为令牌（$P/$PK）
- JSON 结构解包（展开嵌套）

### markdown_plugin
Markdown脱水：保留标题、链接、图片、列表等核心结构，折叠注释以压缩体积。
## 保留信号
- # 标题行
- [链接]或![图片]
- - 或 * 或 1. 列表项
## 压缩目标
- HTML/XML注释（如 <!-- comment -->）

### maven_plugin
Maven 构建日志脱水：保留错误/警告和构建结果，折叠下载进度行。
## 保留信号
- [ERROR] 行
- [WARNING] 行
- BUILD SUCCESS / BUILD FAILURE
- Tests run: 测试摘要
## 压缩目标
- [INFO] 下载进度行折叠
- 重复 [INFO] 行去重

### ndjson_plugin
NDJSON脱水：保留首尾行，折叠中间行，压缩测试摘要。
## 保留信号
- 前5行（保留开头）
- 后5行（保留结尾）
## 压缩目标
- 中间行（折叠为省略行）

### node_error_plugin
Node.js错误脱水：保留异常类名与消息，折叠堆栈帧为紧凑令牌，压缩自定义类名和文件路径。
## 保留信号
- Error, SyntaxError 等内置异常类名（白名单保留字面量）
- 以 Error 或 Exception 结尾的自定义类名
- 异常消息（msg）
- 堆栈帧中的函数名（func）
- 堆栈帧中的行号（line）
- 堆栈帧中的列号（col）
- <anonymous> 文件（保留字面量）
## 压缩目标
- 非白名单自定义异常类名（字典化）
- 堆栈帧中的文件路径（字典化，除 <anonymous>）
- 缩进符（转换为数字编码）

### nodejs_plugin
Node.js 运行时日志脱水：保留错误和 npm 错误，折叠下载进度。
## 保留信号
- Error / TypeError 行
- npm ERR! 行
- WARN 行
## 压缩目标
- npm 下载进度折叠
- node_modules 路径压缩

### noise_filter_plugin
噪声过滤：检测并替换二进制数据为简短标记，保留纯文本内容。
## 保留信号
- 纯文本行（非二进制控制字符）
## 压缩目标
- 二进制数据（替换为 [BINARY_DATA: Size=..., MD5=...] 标记）

### php_ruby_plugin
PHP/Ruby 错误栈脱水：保留错误关键行，折叠 HTML 包裹。
## 保留信号
- Fatal error: 行（PHP 致命错误）
- PHP Stack trace: 行（PHP 堆栈追踪）
- Uncaught Error: 行（PHP 未捕获错误）
- ActionView::Template::Error（Ruby 模板错误）
- .rb: 行（Ruby 文件引用）
- rake aborted!（Rake 中止）
- HTML 错误页面标题（Whoops! / exception_title）
## 压缩目标
- HTML 标签包裹（<html>/<div> 等）

### protobuf_plugin
Protobuf脱水：保留诊断信息，折叠重复警告，压缩文件路径。
## 保留信号
- error 诊断行（保留错误位置和消息）
- warning 诊断行（最多前6个）
- 错误计数和警告计数（汇总行）
- is_error_line 匹配的行（错误信号）
## 压缩目标
- .proto 文件路径（替换为 $PB 令牌字典）
- 多余空白字符（compact_spaces 压缩）
- 重复的警告（超过6个后丢弃）
- 非诊断且非错误信号的行（丢弃）

### pulumi_plugin
Pulumi部署输出脱水：保留错误信号和资源操作摘要，折叠详细资源行为操作类型+字典化URN，压缩非关键行。
## 保留信号
- 错误行（保留错误信号，如包含error:的行）
- Resources: 行（保留资源计数摘要行）
- 操作计数汇总（如A: 3 D: 1 M: 2，在行尾添加）
## 压缩目标
- 详细资源行（匹配+/-~模式的资源行，折叠为操作类型+字典调用的短标记）
- 资源URN和类型（通过dict_engine.add_path_layered进行令牌字典替换）
- 非资源非错误非摘要行（可能被丢弃或压缩）

### pytest_plugin
pytest脱水：保留测试结果与摘要，折叠重复状态，压缩测试路径和 summary 标签。
## 保留信号
- collected 行（测试收集信息）
- 测试结果行（如 test.py::test_func PASSED）
- summary 行（如 1 failed, 10 passed）
- 测试 session 开始行（如 test session starts）
## 压缩目标
- 完整测试路径替换为 $PY 令牌字典
- 重复的测试结果状态合并计数
- summary 中为零计数的状态丢弃
- 状态标签缩写（如 passed->P, failed->F）
- 多余空格压缩

### python_traceback_plugin
Python异常脱水：保留异常类型和关键堆栈信息，折叠重复异常和深层堆栈，压缩路径和异常消息。
## 保留信号
- Traceback (most recent call last): 头部
- 内置异常类名（如 Exception, ValueError 等）
- 错误消息行（如 ValueError: invalid literal）
- 文件路径和行号（如 File "...", line ..., in ...）
- 异常链中的连接信息（如直接原因、上下文等）
## 压缩目标
- 重复的异常堆栈（超过阈值替换为 [DUPLICATE] 标记）
- 深层堆栈帧（超过阈值截断为 [...]）
- 文件路径（替换为 $PY|FL| 令牌并使用字典）
- 异常类型和消息（替换为 $PY|EX| 令牌并使用字典）
- 链式异常计数摘要（多个异常合并为 [CHAINED] 计数）

### rust_go_plugin
Rust/Go日志脱水：保留编译与测试关键信号，折叠重复编译行，压缩测试统计与路径。
## 保留信号
- Compiling .. (编译开始，折叠后保留统计行)
- Finished .. (编译完成行)
- test .. FAILED (失败测试详情)
- Warning/error/panic (错误警告行)
- goroutine .. [..]: (Go协程信息)
- running N tests (测试开始行，被替换为统计)
## 压缩目标
- 重复的Compiling行折叠为一行计数并隐藏细节
- 多个测试结果汇总为统计行，仅保留失败详情
- 长路径替换为字典令牌如$Pn
- Go测试输出类似压缩

### shell_session_plugin
Shell会话脱水：保留命令和输出，折叠提示符和进度信息。
## 保留信号
- 命令与输出行（未明确丢弃的文本块）
## 压缩目标
- shell提示符（如$ # > PS ~$）
- ANSI转义序列
- 多空格（折叠为单个空格）
- 环境变量赋值行（env var=value）
- robocopy/curl/tar进度行

### smart_code_plugin
代码脱水：保留内置异常和关键字标识符，折叠长自定义标识符为字典令牌，压缩连续空格为长度标记。
## 保留信号
- SyntaxError（JavaScript / Python 内置异常）
- public（关键字）
- short_id（长度≤8的标识符）
## 压缩目标
- 长自定义标识符（长度>8且非关键字非保留异常）压缩为$PK令牌
- 连续空格压缩为$S|长度标记

### smart_path_plugin
智能路径脱水：保留路径上下文，折叠长路径为字典令牌，压缩重复路径。
## 保留信号
- 非路径文本（路径之外的文本，原样保留）
## 压缩目标
- 文件路径（替换为字典令牌）

### spring_boot_plugin
Spring Boot 应用日志脱水：保留关键事件（下载、生命周期、启动），折叠 Maven 下载 URL 和 Spring 组件包名。
## 保留信号
- Downloaded from 行（Maven 下载完成）
- Downloading from 行（Maven 下载开始）
- Spring Boot 行（启动标识）
- Starting application 行（应用启动）
- SPRING_LIFECYCLE_RE 匹配行（Spring 生命周期事件）
## 压缩目标
- Maven 下载 URL 替换为 $SPRING 令牌字典（路径折叠）
- Spring 生命周期日志中的 logger 包名替换为令牌字典（包名压缩）
- Bean 初始化消息中的包名（如配置 extract_beans_packages）折叠为令牌

### sql_plugin
SQL脱水：保留SQL语句结构，折叠超长INSERT VALUES内容。
## 保留信号
- SQL关键字（SELECT、INSERT等）所在行
- INSERT语句的前缀部分（不被截断）
- 短SQL语句（完整保留）
## 压缩目标
- 长INSERT VALUES（超过max_insert_values_len时截断为占位符）

### static_rule_plugin
静态规则脱水：保留进入和保留模式匹配的行，折叠重复行和长块。
## 保留信号
- enter正则匹配的行（区块开始标记）
- keep正则匹配的行（重要行，检测分数0.45）
- 未折叠的独立行（原始内容保留）
## 压缩目标
- 连续重复的行（合并为带计数的'{行}(x次数)'）
- 超过阈值长度的区块（折叠为'$STATIC[...]'占位符）

### syslog_plugin
系统日志脱水：保留时间戳和消息内容，折叠主机名和进程名为字典令牌，压缩重复标识。
## 保留信号
- 时间戳（月 日 时:分:秒）
- 消息内容（msg）
- PID（进程ID，可选）
- 非syslog格式的行（原样保留）
## 压缩目标
- 主机名（替换为$SYS格式中的字典令牌）
- 进程名（替换为$SYS格式中的字典令牌）

### template_driven_plugin
模板驱动脱水：保留模板结构，压缩可变部分为字典令牌。
## 保留信号
- 匹配模板模式的行（保留模板固定文本）
## 压缩目标
- 捕获组中变量部分（压缩为 $TEMPLA 字典令牌）

### terraform_plugin
Terraform脱水：保留资源变更行与错误信号，折叠路径为$TF令牌，压缩计划摘要与已知后计算行。
## 保留信号
- 资源变更行：'# 路径 will be 动作' 格式
- 错误信号行：包含error等关键词的行
- 计划摘要行：'Plan: X to add, Y to change, Z to destroy'
- 已知后计算行：'(known after apply)' 行统计
## 压缩目标
- 长资源路径替换为$TF令牌字典
- 已知后计算行折叠为计数
- 计划摘要格式化为简洁摘要
- ANSI转义序列被清除

### unity_unreal_plugin
Unity/Unreal构建日志脱水：保留Unreal和Unity的日志标签，折叠连续的资源加载噪音。
## 保留信号
- LogUObject、LogHAL、LogLinker、FAndroidApp（Unreal引擎日志标签）
- Unloading、Building AssetBundle、Shader compilation（Unity构建消息）
- Loading .uasset/.prefab/.mat（通用资源加载信息，但可能被聚合）
## 压缩目标
- 连续的Loading Object行（聚合为一条，计数量）

### vcs_az_plugin
Azure DevOps CLI 输出脱水：保留命令锚点、关键K-V和项目信息，折叠空行、括号和噪音行，压缩URL为短形式。
## 保留信号
- az repos show 等命令锚点行（保留命令本身）
- 警报行（通过 map_az_alert 保留）
- K-V 行（key:value 映射为 BR/URL/SS/REPO/PRJ/ID 等符号）
- 列表输出中的 name, id, defaultBranch, project 字段
- 创建结果中的 "Repository created:" 行（标记为 A:）
- 删除结果中的 "Repository deleted:" 行（标记为 D:）
## 压缩目标
- 空行（跳过）
- JSON 数组括号 [ ] 和对象花括号 { }（过滤）
- 重复的 "az ..." 锚点行（仅保留第一个）
- 噪音行（通过 is_az_noise 过滤）
- 长 URL（remoteUrl, webUrl 缩写为短形式）
- 无冒号行视为项目名，标记为 PRJ:（show 函数中）

### vcs_bitbucket_plugin
Bitbucket CLI输出脱水：保留PR/Issue关键元数据，折叠表头分隔线和噪声信息，压缩为紧凑格式。
## 保留信号
- 命令锚点（bitbucket pr list等）
- PR列表数据行（#ID ST:STATE OW:@author title）
- PR视图元数据（状态、描述、分支等）
- PR创建结果（Created PR #...）
- Issue列表数据行（#ID ST:STATUS OW:@assignee title PRI:priority）
- 源代码分支映射（Source:feature-auth->main）
## 压缩目标
- 空行
- 表头行（全大写缩写的标题行）
- 分隔线（全由-或=组成的长行）
- 噪声信息（Created/Updated/Participants/Comments等）
- URL行（URL:或http开头）
- 冗余标题行（如'Pull request #...'）

### vcs_bzr_plugin
Bazaar脱水：保留命令锚点和文件状态，折叠噪音和空行，压缩长路径状态。
## 保留信号
- bzr 命令锚点行（如 bzr status, bzr log）
- 文件状态行（modified, added 等）
- reverted 行（格式化为 ST:R）
- commit 行
- pull/merge/push 行
- 警告/错误信息（map_bzr_alert）
## 压缩目标
- 噪音行（如进度条、统计信息等）
- 空行
- 重复 bzr 命令锚点（只保留第一个）
- 长路径状态文本（映射为短格式）

### vcs_cvs_plugin
CVS脱水：保留命令锚点和文件状态，折叠噪音和分隔线，压缩键值对。
## 保留信号
- cvs 开头的命令锚点行（如 cvs update）
- 状态码映射行（U:, M:, A: 等）
- Index: 行（diff文件索引）
- 错误/冲突行（如 conflict, error:）
- No edits: 行（unedit无编辑文件）
## 压缩目标
- 空行
- 等号分隔线（长度>=8全等号）
- 重复的cvs命令锚点（只保留第一条）
- CVS噪音行（如 cvs server: 信息）
- 日志模板行（is_cvs_log_boilerplate）
- 键值对压缩（如 Working revision: → WR:）
- 状态长词压缩（如 Up-to-date → OK）

### vcs_darcs_plugin
Darcs脱水：保留命令锚点与关键操作信息，折叠无关噪声和冗余行。
## 保留信号
- darcs命令锚点行（如darcs log, darcs status）
- 补丁结构信息（hash, author, date, subject, files）
- Old message: 和 New message: 行（amend命令）
- Rebasing from: 和 Rebasing to: 行（rebase命令）
- 警报映射行（map_darcs_alert返回的非空内容）
## 压缩目标
- 空行
- 噪声行（is_darcs_noise过滤的无关行）
- 长输出中超出cost_gate阈值的冗余行
- 通用fallback中第一条之后的darcs命令

### vcs_fossil_plugin
Fossil脱水：保留命令锚点和状态变更摘要，折叠叙事废话，压缩元数据噪音。
## 保留信号
- fossil status/changes 等命令锚点行
- 映射后的状态码行（M/A/D/R/!）
- Pull/Push 行（sync命令）
- 警报行（map_fossil_alert）
- 非噪音非叙事的有效行（generic fallback）
## 压缩目标
- 空行
- 元数据噪音行（如Repository/Check-ins等）
- 叙事废话行（如Stash changes/Autosync等）
- 非锚点的fossil命令前缀行（重复命令）
- 状态变化前的冗长描述行（语法覆盖）

### vcs_gerrit_plugin
Gerrit脱水：保留命令锚点与核心变更摘要，折叠噪音与长URL，压缩为符号化摘要。
## 保留信号
- 命令锚点行（如 "gerrit query"）
- change ID 行（压缩为 CHG @xxx）
- 关键 key-value 字段（project, branch, status 等，压缩为 PRJ, BR 等）
- reviewer 列表（逗号分隔）
- review 标签（如 Code-Review+2）
- push ref 映射（压缩后保留源和目标分支名）
- checkout 分支和状态（如 Switched to branch, up-to-date）
## 压缩目标
- 噪音行（通过 is_gerrit_noise 过滤，如 Counting objects, remote:）
- 长 URL（以 http 或 Push to ssh:// 开头）
- 进度条/传输中行（以 "..." 结尾且长度小于40）
- 空行
- 重复的 URL
- change ID 完整格式压缩为 CHG @+短id
- key-value 字段键名压缩为短符号（如 project->PRJ）
- refs/heads/ 前缀在 push 映射中移除

### vcs_gh_plugin
GitHub CLI输出脱水：保留命令锚点和结构化数据行，折叠块内多行，压缩URL和键名。
## 保留信号
- gh 命令本身（如 "gh pr list"）
- PR/Issue 表格行（如带序号和标题的行）
- KV 对中的短符号键值（如 ST:success）
- 去符号后的状态行（含缩写URL）
- 通用非空非分隔符非表头行（经空格压缩）
## 压缩目标
- 空行（跳过）
- 分隔线（如 "---" 行）
- 表头行（列标题行）
- 状态符号 ✓ ✗ ○（替换为空格后去除）
- 连续多个空格（压缩为单个空格）
- URL 缩写（如 https://github.com/... 变为 URL:...）
- 长键名替换为短符号（如 workflow -> WF, status -> ST）
- run list 中块内多行合并为一行（空格分隔）

### vcs_git_plugin
Git脱水：保留命令核心输出，折叠图形进度，压缩网络噪音和分页信息。
## 保留信号
- 文件路径（diff/status/add等）
- 状态标记（M, A, D等）
- 提交哈希与提交信息（log）
- reflog条目（最多20条）
- 冲突标记（merge冲突）
- 分支切换信息（checkout reflog）
## 压缩目标
- 进度行（fetch/push/pull网络噪音）
- 图形字符（*|/\图形输出）
- 超出的reflog条目（>20折叠）
- 装饰（origin/->o/, tag: ->t:）
- summary词长（files changed->files, insertions(+) ->ins等）
- 分页提示行（--More--等）

### vcs_glab_plugin
GitLab CLI输出脱水：保留命令及关键结果，折叠表格格式和噪声行，压缩长描述和冗余信息。
## 保留信号
- glab 命令起始行（如 glab mr list）
- MR/Issue 列表行（精简行，如 !123 Title [state] author date）
- MR/Issue 元数据（title, state, author, date 等）
- 创建结果（如 A:!456）
- view 中的 Description 内容
## 压缩目标
- 空行（跳过）
- 分隔符和表格标题（如 ---, ---+ 等）
- 成功标记（✓）
- URL 行（以 http 或 URL: 开头）
- 噪声行（如 is_glab_noise, is_glab_view_noise）
- 创建过程中的提示行（如 Creating merge request）

### vcs_hg_plugin
Hg命令脱水：保留命令首行、变更集、文件操作摘要，折叠重复grafting和注释行。
## 保留信号
- 命令首行（如hg update）
- 变更集ID（grafting行中提取）
- 文件操作摘要（如'3 updated, 2 removed'）
- 分支名（含~表示inactive）
- 日期（压缩为YYYY-MM-DD HH:MM格式）
- histedit命令（pick/edit/fold等）
- shelve列表条目（--list时）
## 压缩目标
- 重复的grafting行（合并为一条，计数）
- 注释行（#开头）与空行
- 分支inactive状态（替换为~）
- 日期（压缩为紧凑格式）
- 跳过merging行（详细信息）

### vcs_p4_plugin
P4脱水：保留change编号、文件操作与结果，折叠统计摘要，压缩长路径和日期。
## 保留信号
- change 编号（如 1234）
- 文件操作（如 opened, edit, add）
- 精简后的 depot 路径（并压缩公共前缀）
- 命令结果（如 resolved, would-update）
- 差异统计摘要（文件数:增加-删除）
## 压缩目标
- 长路径替换为公共前缀（$VCS_P4）
- 日期时间截断为 19 字符 YYYY-MM-DD HH:MM:SS
- 文件大小数字转人类可读（如 1234567 -> 1.2M）
- diff 头行（---/+++）丢弃
- 叙事性噪音行丢弃（如 p4 narrative noise）
- 同步预览摘要压缩为“数 would-update”

### vcs_plugin
VCS 脱水：保留版本控制命令的语义结构，折叠冗余空白和路径信息，压缩差异输出以适应 AI 上下文。
## 保留信号
- diff -- 开头行（差异头部）
- @@ 开头行（差异块范围）
- +++ / --- 开头行（文件路径）
- 状态指示符（如 M、A、D）
- [paths] 路径字典（路径重映射表）
- 日期时间令牌（如 YYYYMMDD HH:MM）
## 压缩目标
- 前导连续空白（非 Python 文件或 hash 范围）
- 内联对齐多空白（非表格或代码注释保护）
- 长绝对路径（替换为 [paths] 字典条目）
- 目录树结构（如 git checkout 输出缩进）
- SVN 更新输出中的冗余行
- 差异输出中的重复文件路径信息

### vcs_repo_plugin
Android Repo 命令输出脱水：保留命令锚点和项目/状态/推送信息，折叠进度条、URL 等噪音，压缩 diff 文件路径和 hunk 头。
## 保留信号
- repo sync/status/upload 命令锚点（第一行）
- project 行：项目路径与状态/哈希
- 推送映射行：HEAD -> refs/...
- diff 文件路径（压缩为 D:前缀）
- hunk 头（压缩后格式）
## 压缩目标
- 进度噪音行：Downloading..., Syncing: ..., Syncing done.
- SSH/HTTPS URL 行
- 重复的命令锚点（仅保留第一行）
- URL 行：ssh://, http://, https://
- diff 行中 a/ b/ 路径前缀（压缩为 D:）
- hunk 头中空格和上下文（压缩为 @@-a,b->c,d@@）

### vcs_svn_plugin
SVN 命令输出脱水：保留命令锚点和变更状态，压缩路径。
## 保留信号
- 原始命令锚点
- 文件变更状态（A/D/M/U/C）
- commit 信息
## 压缩目标
- 长路径替换为 $SVN 令牌字典
- 重复行去重

### web_log_plugin
Web访问日志脱水：保留异常/慢请求信号，折叠常规记录为紧凑摘要，压缩IP/UA/路径等冗余字段。
## 保留信号
- $W|SUMMARY行（健康摘要）
- 异常行（如4xx/5xx错误）
- 慢请求行（slow lines）
- 原始错误日志行（error_log_pattern匹配的）
## 压缩目标
- IP地址替换为字典令牌
- URI路径替换为分层字典令牌
- User-Agent替换为字典令牌
- 时间戳压缩为紧凑格式（YYYY-MM-DD HH:MM:SS）
- 流路径（如/stream/xxx）折叠为前8字符
- URL编码解码（%20等替换）
- 多个连续空格压缩为一个
- 重复的详细记录聚合为标准摘要

### webpack_vite_plugin
Webpack/Vite 构建日志脱水：保留错误和警告行，折叠资产列表，压缩噪音行。
## 保留信号
- ERROR in 行（编译错误）
- Module parse failed 行（模块解析失败）
- warning: 行（构建警告）
- ⚠️ 行（警告符号）
## 压缩目标
- 噪音行（is_noise_line 过滤的废话行）
- 长路径（可能通过字典替换）

### xcode_log_plugin
Xcode构建日志脱水：保留编译链接命令骨架，折叠/dev/null探针噪音，压缩路径参数。
## 保留信号
- CompileC 行（编译命令）
- Linking 行（链接命令）
- clang 行（编译命令）
- Build succeeded/failed 行（构建结果）
## 压缩目标
- /dev/null 探针行（clang/libtool，批量折叠为 $XC|PROBE|x）
- 编译命令中的路径参数（替换为字典令牌 $XC|C|）
- 编译命令中的源文件路径（压缩为 $XC|C| 的一部分）

### xml_html_plugin
XML/HTML脱水：保留标签结构和文本内容，折叠标签间的空白字符，压缩冗余空格。
## 保留信号
- XML/HTML标签（如 <div>）
- 标签属性（如 class="example"）
- 文本内容（标签之间的文字）
## 压缩目标
- 标签间的空白字符（换行、缩进等）

### yaml_plugin
YAML脱水：保留键值结构，折叠长序列，压缩键标识符。
## 保留信号
- 有效YAML的映射键值对结构（键被字典化）
- 序列的前max_seq_len个元素
- YAML解析失败时的原始文本
## 压缩目标
- 长序列截断（超过max_seq_len部分替换为$SEQ-$n占位符）
- 映射键替换为字典宏
- YAML缩进与空白压缩为紧凑格式
- 深度超过max_depth时替换为"...depth limit..."


