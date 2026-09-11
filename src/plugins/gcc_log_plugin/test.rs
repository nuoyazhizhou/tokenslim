//! gcc_log_plugin 测试模块（文件驱动，严禁 Hardcode）

#[cfg(test)]
mod tests {
    use crate::core::plugin_dispatcher::Plugin;
    use crate::core::text_slicer::SliceType;
    use crate::plugins::gcc_log_plugin::GccLogPlugin;
    use crate::plugins::test_utils::*;

    /// 测试：编译成功样例被插件识别。
    #[test]
    fn detects_gcc_compile() {
        let plugin = GccLogPlugin::new();
        let raw = read_sample_log("gcc_log_plugin", "case_001_compile_success");
        assert!(plugin.detect(&make_log_slice(&raw)).is_some());
    }

    /// 测试：编译错误样例被插件识别。
    #[test]
    fn detects_gcc_error() {
        let plugin = GccLogPlugin::new();
        let raw = read_sample_log("gcc_log_plugin", "case_002_compile_error");
        assert!(plugin.detect(&make_log_slice(&raw)).is_some());
    }

    /// 测试：编译成功样例压缩后不扩张（ROI 门控生效）。
    #[test]
    fn compresses_without_expansion() {
        let plugin = GccLogPlugin::new();
        let raw = read_sample_log("gcc_log_plugin", "case_001_compile_success");
        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
        assert!(
            out.len() <= raw.len(),
            "gcc_log 插件压缩不得扩张: raw={} out={}",
            raw.len(),
            out.len()
        );
    }

    /// 测试：重复警告被折叠为 [WARNING] 摘要且不扩张。
    #[test]
    fn test_case_013_repeated_warnings() {
        let plugin = GccLogPlugin::new();
        let raw = read_sample_log("gcc_log_plugin", "case_013_repeated_warnings");
        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);

        // 验证警告折叠
        assert!(out.contains("[WARNING]"), "应包含 [WARNING] 标记");
        assert!(
            out.contains("repeated") || out.contains("suppressed"),
            "应包含折叠标记"
        );

        // ROI 门控
        assert!(
            out.len() <= raw.len(),
            "警告折叠不得扩张: raw={} out={}",
            raw.len(),
            out.len()
        );
    }

    /// 测试：构建摘要样例输出包含统计信息。
    #[test]
    fn test_case_014_build_summary() {
        let plugin = GccLogPlugin::new();
        let raw = read_sample_log("gcc_log_plugin", "case_014_build_summary");
        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);

        // 验证构建摘要
        assert!(out.contains("[SUMMARY]"), "应包含 [SUMMARY] 标记");
        assert!(
            out.contains("errors") || out.contains("warnings"),
            "应包含错误/警告统计"
        );

        // ROI 门控
        assert!(
            out.len() <= raw.len(),
            "构建摘要不得扩张: raw={} out={}",
            raw.len(),
            out.len()
        );
    }

    /// 测试：链接器输出样例被压缩为 $LD 行。
    #[test]
    fn test_case_015_linker_output() {
        let plugin = GccLogPlugin::new();
        let raw = read_sample_log("gcc_log_plugin", "case_015_linker_output");
        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);

        // 验证链接器压缩
        assert!(out.contains("$LD"), "应包含 $LD 标记");
        assert!(
            out.contains("undefined reference"),
            "应保留 undefined reference"
        );

        // ROI 门控
        assert!(
            out.len() <= raw.len(),
            "链接器压缩不得扩张: raw={} out={}",
            raw.len(),
            out.len()
        );
    }

    /// 测试：CMake configure 样例被压缩为 $CMAKE 行。
    #[test]
    fn test_case_016_cmake_configure() {
        let plugin = GccLogPlugin::new();
        let raw = read_sample_log("gcc_log_plugin", "case_016_cmake_configure");
        assert!(plugin.detect(&make_log_slice(&raw)).is_some());
        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
        assert!(out.contains("$CMAKE"));
        assert!(out.len() <= raw.len());
    }

    /// P2-20（C-5）：Windows CRLF 日志经压缩后不残留 `\r`，且错误行前缀匹配不被破坏。
    #[test]
    fn crlf_input_normalized_to_lf() {
        let plugin = GccLogPlugin::new();
        // CRLF 结尾的 gcc 诊断样本（Windows 终端重定向产物）
        let raw = "main.c:3:5: error: undeclared 'x'\r\n  3 |     x = 1;\r\n      |     ^\r\nerror: build failed\r\n";
        let out = compress_to_string(&plugin, raw, SliceType::LogBlock);
        // 输出不得残留 CR
        assert!(!out.contains('\r'), "CRLF 归一失败，输出残留 CR: {:?}", out);
        // 错误签名行必须被保留（前缀匹配未被 `\r` 破坏）
        assert!(out.contains("error:"), "error 行被吞: {:?}", out);
        // ROI 门控不扩张
        assert!(out.len() <= raw.len());
    }

    /// 测试：Ninja 进度样例被压缩为 $NINJA 行。
    #[test]
    fn test_case_017_ninja_progress() {
        let plugin = GccLogPlugin::new();
        let raw = read_sample_log("gcc_log_plugin", "case_017_ninja_progress");
        assert!(plugin.detect(&make_log_slice(&raw)).is_some());
        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
        assert!(out.contains("$NINJA"));
        assert!(out.len() <= raw.len());
    }

    /// 测试：CTest 失败样例被压缩为 $CTEST 行。
    #[test]
    fn test_case_019_ctest_failure() {
        let plugin = GccLogPlugin::new();
        let raw = read_sample_log("gcc_log_plugin", "case_019_ctest_failure");
        assert!(plugin.detect(&make_log_slice(&raw)).is_some());
        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
        assert!(out.contains("$CTEST"));
        assert!(out.contains("fail") || out.contains("FAILED"));
        assert!(out.len() <= raw.len());
    }

    /// 测试：CMake configure 失败样例被压缩且保留错误信号。
    #[test]
    fn test_case_020_cmake_configure_failure() {
        let plugin = GccLogPlugin::new();
        let raw = read_sample_log("gcc_log_plugin", "case_020_cmake_configure_failure");
        assert!(plugin.detect(&make_log_slice(&raw)).is_some());
        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
        assert!(out.contains("$CMAKE") || out.contains("CMake Error"));
        assert!(out.to_ascii_lowercase().contains("error"));
        assert!(out.len() <= raw.len());
    }

    /// 测试：CI 中 CMake+Ninja+CTest 组合日志被正确压缩。
    #[test]
    fn test_case_023_ci_cmake_ninja_ctest() {
        let plugin = GccLogPlugin::new();
        let raw = read_sample_log("gcc_log_plugin", "case_023_ci_cmake_ninja_ctest");
        assert!(plugin.detect(&make_log_slice(&raw)).is_some());
        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
        assert!(out.contains("$CMAKE"));
        assert!(out.contains("$NINJA"));
        assert!(out.contains("$CTEST"));
        assert!(out.len() <= raw.len());
    }

    /// 测试：nm 符号表样例被 detect 命中，压缩为 $NM 行（全局保留、局部折叠、未定义保留）。
    #[test]
    fn test_case_025_nm_symbols() {
        let plugin = GccLogPlugin::new();
        let raw = read_sample_log("gcc_log_plugin", "case_025_nm_symbols");
        assert!(plugin.detect(&make_log_slice(&raw)).is_some());

        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
        // 全局符号保留：含 main/U printf
        assert!(out.contains("$NM T main"), "应保留全局符号 T main");
        assert!(out.contains("$NM U printf"), "应保留未定义符号 U printf");
        // 局部符号折叠摘要
        assert!(out.contains("$NM local:"), "应包含局部符号折叠摘要");
        assert!(
            out.contains("t=4") && out.contains("b=2") && out.contains("d=1"),
            "局部符号应按类型计数: {out}"
        );
        // ROI 门控：折叠后显著变小
        assert!(
            out.len() <= raw.len(),
            "nm 压缩不得扩张: raw={} out={}",
            raw.len(),
            out.len()
        );
    }

    /// 测试：size 节大小样例被 detect 命中，压缩为 $SIZE 行并去除冗余 hex 列。
    #[test]
    fn test_case_026_size_sections() {
        let plugin = GccLogPlugin::new();
        let raw = read_sample_log("gcc_log_plugin", "case_026_size_sections");
        assert!(plugin.detect(&make_log_slice(&raw)).is_some());

        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
        assert!(out.contains("$SIZE"), "应包含 $SIZE 标记");
        assert!(
            out.contains("a.out text=1344") && out.contains("data=296"),
            "应保留 key 数值: {out}"
        );
        // 数值列折叠后不再出现原始 hex 列（678/8b6 这类十六进制值）
        assert!(
            !out.contains("678") && !out.contains(" 8b6"),
            "冗余 hex 列应被折叠: {out}"
        );
        assert!(out.len() <= raw.len(), "size 压缩不得扩张");
    }

    /// 测试：objdump/readelf 节表样例被 detect 命中，压缩为 $SECTION 行保留节名与 Size。
    #[test]
    fn test_case_027_objdump_sections() {
        let plugin = GccLogPlugin::new();
        let raw = read_sample_log("gcc_log_plugin", "case_027_objdump_sections");
        assert!(plugin.detect(&make_log_slice(&raw)).is_some());

        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
        assert!(out.contains("$SECTION"), "应包含 $SECTION 标记");
        assert!(
            out.contains(".text") && out.contains(".rodata"),
            "应保留节名: {out}"
        );
        assert!(out.contains("size=0000012a"), "应保留节 Size: {out}");
        // ROI 门控
        assert!(
            out.len() <= raw.len(),
            "节表压缩不得扩张: raw={} out={}",
            raw.len(),
            out.len()
        );
    }

    /// 测试：nm -D 动态符号样例被 detect 命中，带 @GLIBC 版本尾的符号完整保留，
    /// 弱局部小写类型折叠为摘要。
    #[test]
    fn test_case_028_nm_dynamic_symbols() {
        let plugin = GccLogPlugin::new();
        let raw = read_sample_log("gcc_log_plugin", "case_028_nm_dynamic_symbols");
        assert!(plugin.detect(&make_log_slice(&raw)).is_some());

        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
        // 带版本尾的未定义符号完整保留（版本是链接缺口判定的关键）
        assert!(
            out.contains("$NM U printf@GLIBC_2.2.5"),
            "应保留带版本尾的 U 符号: {out}"
        );
        assert!(
            out.contains("$NM U malloc@@GLIBC_2.2.5"),
            "应保留 @@ 双 at 版本符号: {out}"
        );
        // 弱符号 W（大写）与全局符号逐条保留
        assert!(
            out.contains("$NM W __cxa_finalize@@GLIBC_2.2.5"),
            "应保留弱符号 W 及其版本尾: {out}"
        );
        assert!(out.contains("$NM T main"), "应保留全局符号 T main");
        // 弱局部 w 折叠入摘要
        assert!(
            out.contains("$NM local:") && out.contains("w=3"),
            "弱局部 w 应折叠计数: {out}"
        );
        // ROI 门控
        assert!(
            out.len() <= raw.len(),
            "nm -D 压缩不得扩张: raw={} out={}",
            raw.len(),
            out.len()
        );
    }

    /// 测试：readelf -S 节表样例被 detect 命中，压缩为 $SECTION 行保留节名与 Size，
    /// 跳过 NULL 空节，折叠地址/偏移/对齐冗余列。
    #[test]
    fn test_case_029_readelf_sections() {
        let plugin = GccLogPlugin::new();
        let raw = read_sample_log("gcc_log_plugin", "case_029_readelf_sections");
        assert!(plugin.detect(&make_log_slice(&raw)).is_some());

        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
        assert!(out.contains("$SECTION"), "应包含 $SECTION 标记");
        assert!(
            out.contains(".text") && out.contains(".data"),
            "应保留节名: {out}"
        );
        // readelf Size 取自第 5 列（00012a 是 .text 的 Size）
        assert!(
            out.contains("size=00012a"),
            "应保留 readelf 节 Size(parts[5]): {out}"
        );
        // NULL 空节不输出
        assert!(!out.contains("$SECTION NULL"), "NULL 空节应被跳过: {out}");
        // 折叠冗余地址列：readelf .text 的原始 Address 列被折叠
        assert!(
            !out.contains("00000000000004f0"),
            "VMA/Section 地址列应被折叠: {out}"
        );
        // ROI 门控
        assert!(
            out.len() <= raw.len(),
            "readelf 节表压缩不得扩张: raw={} out={}",
            raw.len(),
            out.len()
        );
    }

    /// 测试：ar -t 归档成员清单被 detect 命中，折叠为计数 + 扩展名分组摘要 `$AR archive N members`，
    /// 命令锚点行被折叠，成员名不逐条残留。
    #[test]
    fn test_case_030_ar_members() {
        let plugin = GccLogPlugin::new();
        let raw = read_sample_log("gcc_log_plugin", "case_030_ar_members");
        assert!(plugin.detect(&make_log_slice(&raw)).is_some());

        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
        assert!(out.contains("$AR archive"), "应包含 $AR 标记: {out}");
        assert!(out.contains("members"), "应包含成员计数: {out}");
        // 成员名折叠为扩展名分组统计（本样例 6 个 .o 成员）
        assert!(
            out.contains("o=6") || out.contains(".o=6"),
            "应包含 .o 扩展名统计: {out}"
        );
        // 成员名不逐条残留，命令锚点行折叠
        assert!(!out.contains("add.o"), "成员名应折叠为扩展统计: {out}");
        assert!(!out.contains("ar -t"), "命令锚点行应被折叠: {out}");
        // ROI 门控
        assert!(
            out.len() <= raw.len(),
            "ar -t 压缩不得扩张: raw={} out={}",
            raw.len(),
            out.len()
        );
    }

    /// 测试：objdump -t 符号表被 detect 命中，全局/弱符号逐条保留 $OBJ，
    /// 局部符号按类型折叠摘要，地址/Size 列折叠，缺 type 的全局符号以 ? 占位。
    #[test]
    fn test_case_031_objdump_symbols() {
        let plugin = GccLogPlugin::new();
        let raw = read_sample_log("gcc_log_plugin", "case_031_objdump_symbols");
        assert!(plugin.detect(&make_log_slice(&raw)).is_some());

        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
        // 全局函数/数据符号逐条保留
        assert!(out.contains("$OBJ F main"), "应保留全局函数符号: {out}");
        assert!(
            out.contains("$OBJ O global_var"),
            "应保留全局数据符号: {out}"
        );
        assert!(
            out.contains("$OBJ F func_add") && out.contains("$OBJ F func_mul"),
            "应保留其余全局函数: {out}"
        );
        // 缺 type 列的全局符号（__bss_start）以 ? 占位
        assert!(
            out.contains("$OBJ ? __bss_start"),
            "空 type 全局符号应变占位: {out}"
        );
        // 局部符号折叠为摘要（main.c 的 df、.text/.rodata 的 d、.bss 的 O）
        assert!(
            out.contains("$OBJ local:") && out.contains("df=1"),
            "局部符号应折叠计数: {out}"
        );
        // 原始地址/Size 列被折叠
        assert!(
            !out.contains("0000000000000000 g     F .text"),
            "原始符号行地址列应被折叠: {out}"
        );
        // ROI 门控
        assert!(
            out.len() <= raw.len(),
            "objdump -t 压缩不得扩张: raw={} out={}",
            raw.len(),
            out.len()
        );
    }

    /// 测试：objdump -d 反汇编被 detect 命中，保留函数标签 $FUNC 与助记符 $ASM，
    /// 折叠每行相对偏移与机器码字节，跳转目标 <func_add> 自含保留。
    #[test]
    fn test_case_032_objdump_disasm() {
        let plugin = GccLogPlugin::new();
        let raw = read_sample_log("gcc_log_plugin", "case_032_objdump_disasm");
        assert!(plugin.detect(&make_log_slice(&raw)).is_some());

        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
        // 函数边界标签
        assert!(out.contains("$FUNC main"), "应保留函数标签 main: {out}");
        assert!(
            out.contains("$FUNC func_add"),
            "应保留函数标签 func_add: {out}"
        );
        // 助记符序列
        assert!(
            out.contains("$ASM push %rbp") && out.contains("$ASM ret"),
            "应保留助记符序列: {out}"
        );
        // 跳转目标自含保留
        assert!(
            out.contains("call 0 <func_add>"),
            "应保留含目标地址的 call: {out}"
        );
        // 折叠：原始函数标签地址与机器码字节不残留
        assert!(!out.contains("<main>:"), "原始函数标签地址应被折叠: {out}");
        assert!(!out.contains("48 89 e5"), "机器码字节列应被折叠: {out}");
        // ROI 门控
        assert!(
            out.len() <= raw.len(),
            "objdump -d 压缩不得扩张: raw={} out={}",
            raw.len(),
            out.len()
        );
    }

    /// 测试：ar -t 嵌套目录路径成员清单被 detect 命中，折叠为 $AR 计数+扩展名摘要，
    /// 嵌套路径成员全部被统计且扩展名分组正确。
    #[test]
    fn test_case_033_ar_nested_members() {
        let plugin = GccLogPlugin::new();
        let raw = read_sample_log("gcc_log_plugin", "case_033_ar_nested_members");
        assert!(plugin.detect(&make_log_slice(&raw)).is_some());

        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
        assert!(out.contains("$AR archive"), "应包含 $AR 标记: {out}");
        assert!(out.contains("members"), "应包含成员计数: {out}");
        // 6 个 .o 成员，扩展名分组统计
        assert!(
            out.contains("o=6") || out.contains(".o=6"),
            "应统计 6 个 .o 成员: {out}"
        );
        // 嵌套路径成员不逐条残留
        assert!(
            !out.contains("core/render/shader.o"),
            "嵌套路径成员应折叠为扩展统计: {out}"
        );
        // 命令锚点行折叠
        assert!(!out.contains("ar -t"), "命令锚点行应被折叠: {out}");
        // ROI 门控
        assert!(
            out.len() <= raw.len(),
            "ar 嵌套成员压缩不得扩张: raw={} out={}",
            raw.len(),
            out.len()
        );
    }

    /// 测试：objdump -r 重定位表被 detect 命中，保留 $RELOC 类型与重定位符号名，
    /// 折叠 Offset/Info/Sym.Value/addend，带 @GLIBC 版本尾的符号完整保留。
    #[test]
    fn test_case_034_objdump_relocations() {
        let plugin = GccLogPlugin::new();
        let raw = read_sample_log("gcc_log_plugin", "case_034_objdump_relocations");
        assert!(
            plugin.detect(&make_log_slice(&raw)).is_some(),
            "objdump -r 重定位表应被 detect 命中"
        );

        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
        // 重定位类型与符号名保留
        assert!(
            out.contains("$RELOC R_X86_64_PC32 func_add"),
            "应保留类型+符号 func_add: {out}"
        );
        assert!(
            out.contains("$RELOC R_X86_64_PC32 global_var"),
            "应保留全局变量重定位: {out}"
        );
        // 版本尾符号完整保留（链接缺口判定关键）
        assert!(
            out.contains("$RELOC R_X86_64_PC32 printf@GLIBC_2.2.5"),
            "应保留带 @GLIBC 版本尾的重定位符号: {out}"
        );
        // 折叠：Offset/addend 不残留
        assert!(!out.contains("func_add-0x"), "addend 后缀应被剥离: {out}");
        assert!(
            !out.contains("000000000000000c"),
            "重定位 Offset 列应被折叠: {out}"
        );
        // 命令锚点行折叠
        assert!(
            !out.contains("RELOCATION RECORDS"),
            "重定位节头应被折叠: {out}"
        );
        // ROI 门控
        assert!(
            out.len() <= raw.len(),
            "objdump -r 压缩不得扩张: raw={} out={}",
            raw.len(),
            out.len()
        );
    }

    /// 测试：ar rcs 创建归档 verbose 被 detect 命中，保留 $AR_CREATE 添加成员与归档名，
    /// 折叠创建命令行，区别于 ar -t 只读清单。
    #[test]
    fn test_case_035_ar_create_verbose() {
        let plugin = GccLogPlugin::new();
        let raw = read_sample_log("gcc_log_plugin", "case_035_ar_create_verbose");
        assert!(
            plugin.detect(&make_log_slice(&raw)).is_some(),
            "ar rcs -v 创建归档应被 detect 命中"
        );

        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
        // 归档创建提示与成员添加记录保留
        assert!(
            out.contains("$AR_CREATE archive librender.a"),
            "应保留归档创建提示: {out}"
        );
        assert!(
            out.contains("$AR_CREATE core/render/shader.o"),
            "应保留添加成员目录: {out}"
        );
        assert!(
            out.contains("$AR_CREATE utils/math/vector.o"),
            "应保留嵌套路径成员: {out}"
        );
        // 创建命令行折叠
        assert!(!out.contains("ar rcs -v"), "创建命令行应被折叠: {out}");
        // ROI 门控
        assert!(
            out.len() <= raw.len(),
            "ar 创建 verbose 压缩不得扩张: raw={} out={}",
            raw.len(),
            out.len()
        );
    }

    /// P2-19 回归：诊断块内的源码上下文行被折叠为 [DIAG] 摘要，
    /// 不再无状态地整行丢弃。
    #[test]
    fn p2_19_diagnostic_context_folded_with_summary() {
        let plugin = GccLogPlugin::new();
        let raw = read_sample_log("gcc_log_plugin", "case_013_repeated_warnings");
        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
        assert!(
            out.contains("[DIAG]"),
            "诊断块应折叠出 [DIAG] 摘要: {out}"
        );
        assert!(
            !out.contains("   10 |") && !out.contains("      |     ^"),
            "诊断上下文源码行不得原样保留: {out}"
        );
    }

    /// P2-19 回归：诊断块之外的管道竖线行（Gradle 依赖树 / git graph）不得
    /// 被误判为诊断上下文而误删。
    #[test]
    fn p2_19_pipe_lines_outside_diag_block_kept() {
        let plugin = GccLogPlugin::new();
        let raw = read_sample_log("gcc_log_plugin", "case_036_mixed_diag_and_graph");
        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
        assert!(
            out.contains("|\\") && out.contains("| *") && out.contains("|/"),
            "git graph 竖线行应保留: {out}"
        );
        assert!(
            out.contains("git log --oneline --graph"),
            "graph 命令行应保留: {out}"
        );
    }
}
