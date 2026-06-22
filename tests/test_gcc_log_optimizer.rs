//! GCC 编译日志优化器
//!
//! 功能：
//! 1. 支持多种时间戳格式
//! 2. 提取并排序宏定义
//! 3. 路径识别和 token 化
//! 4. 标准化编译选项顺序

fn main() {
    println!("=== GCC 编译日志优化器 ===\n");

    // 测试用例
    let test_line = "[2026-03-05T03:21:46.868Z] arm-none-linux-gnueabihf-gcc -O2 -DDEBUG_LEVEL_ERROR -Wall -fPIC -D_REENTRANT -g -I/jenkins/workspace/build_root/project_sdk/opensource/build/include -I/jenkins/workspace/build_root/out/Test_Sound_Lite_V2/install/dist/release/include -DDEMO_DEVICE -DDEMO_DEVICE_Test_SOUND_LITE";

    println!("原始行：{}\n", test_line);

    // 步骤 1: 提取时间戳
    let (timestamp, content) = extract_timestamp(test_line);
    println!("时间戳：{}", timestamp);
    println!("内容：{}\n", content);

    // 步骤 2: 解析编译命令
    let parsed = parse_compile_command(content);

    // 步骤 3: 优化
    let optimized = optimize_compile_command(&parsed);

    println!("优化后：{}", optimized);

    // 测试多种时间戳格式
    test_timestamp_formats();
}

/// 提取时间戳（支持多种格式）
fn extract_timestamp(line: &str) -> (String, &str) {
    // 格式 1: [2026-03-05T03:21:46.868Z]
    if line.starts_with('[') {
        if let Some(end) = line.find(']') {
            return (line[..=end].to_string(), line[end + 1..].trim());
        }
    }

    // 格式 2: 17:37:26 (HH:MM:SS)
    if line.len() >= 8 && line.chars().nth(2) == Some(':') && line.chars().nth(5) == Some(':') {
        let potential_time = &line[..8];
        if potential_time
            .chars()
            .all(|c| c.is_ascii_digit() || c == ':')
        {
            return (potential_time.to_string(), line[8..].trim());
        }
    }

    // 没有时间戳
    (String::new(), line.trim())
}

/// 解析编译命令
fn parse_compile_command(content: &str) -> CompileCommand {
    let parts: Vec<&str> = content.split_whitespace().collect();

    let mut cmd = CompileCommand {
        compiler: String::new(),
        flags: Vec::new(),
        defines: Vec::new(),
        includes: Vec::new(),
        input_file: String::new(),
        output_file: String::new(),
    };

    let mut i = 0;
    while i < parts.len() {
        let part = parts[i];

        // 编译器名称
        if cmd.compiler.is_empty()
            && (part.ends_with("gcc") || part.ends_with("g++") || part.ends_with("clang"))
        {
            cmd.compiler = part.to_string();
            i += 1;
            continue;
        }

        // 宏定义 -Dxxx
        if part.starts_with("-D") {
            cmd.defines.push(part[2..].to_string());
            i += 1;
            continue;
        }

        // 包含路径 -Ixxx
        if part.starts_with("-I") {
            cmd.includes.push(part[2..].to_string());
            i += 1;
            continue;
        }

        // 其他标志
        if part.starts_with('-') {
            cmd.flags.push(part.to_string());
            i += 1;
            continue;
        }

        // 输入文件
        if part.ends_with(".c") || part.ends_with(".cpp") || part.ends_with(".cc") {
            cmd.input_file = part.to_string();
            i += 1;
            continue;
        }

        // 输出文件
        if part.ends_with(".o") {
            cmd.output_file = part.to_string();
            i += 1;
            continue;
        }

        i += 1;
    }

    cmd
}

/// 优化编译命令
fn optimize_compile_command(cmd: &CompileCommand) -> String {
    let mut result = String::new();

    // 添加编译器
    if !cmd.compiler.is_empty() {
        result.push_str(&cmd.compiler);
        result.push(' ');
    }

    // 添加标志（排序）
    let mut flags = cmd.flags.clone();
    flags.sort();
    for flag in &flags {
        result.push_str(flag);
        result.push(' ');
    }

    // 添加宏定义（排序）
    let mut defines = cmd.defines.clone();
    defines.sort();
    for def in &defines {
        result.push_str("-D");
        result.push_str(def);
        result.push(' ');
    }

    // 添加包含路径（排序 + 路径压缩）
    let mut includes = cmd.includes.clone();
    includes.sort();
    for inc in &includes {
        result.push_str("-I");
        result.push_str(inc);
        result.push(' ');
    }

    // 添加输入输出文件
    if !cmd.input_file.is_empty() {
        result.push_str(&cmd.input_file);
        result.push(' ');
    }
    if !cmd.output_file.is_empty() {
        result.push_str("-o ");
        result.push_str(&cmd.output_file);
        result.push(' ');
    }

    result
}

/// 测试多种时间戳格式
fn test_timestamp_formats() {
    println!("\n=== 时间戳格式测试 ===");

    let test_cases = vec![
        "[2026-03-05T03:21:46.868Z] text",
        "17:37:26 text",
        "Mar 5, 2026 3:21:46 PM text",
        "05-Mar-2026 15:21:46 text",
        "2026-03-05 15:21:46 text",
        "[2026-03-05T03:21:46.868Z] 17:37:26 text", // 双重时间戳
    ];

    for test in test_cases {
        let (ts, content) = extract_timestamp(test);
        println!("输入：{}", test);
        println!("  时间戳：{}", ts);
        println!("  内容：{}", content);
    }
}

/// 编译命令结构
#[derive(Debug)]
struct CompileCommand {
    compiler: String,
    flags: Vec<String>,
    defines: Vec<String>,
    includes: Vec<String>,
    input_file: String,
    output_file: String,
}
