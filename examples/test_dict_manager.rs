use std::borrow::Cow;
use std::sync::Arc;
use tokenslim::core::dictionary_manager::DictionaryManager;
use tokenslim::core::stream_reader::SliceInput;
use tokenslim::core::text_slicer::{SliceMode, SlicerConfig, TextSlicer};

fn main() {
    println!("测试字典管理器与 TextSlicer 集成");
    println!("====================================");

    // 创建字典管理器
    let dict_manager = Arc::new(DictionaryManager::new());

    // 创建带有字典管理器的 TextSlicer
    let config = SlicerConfig {
        mode: SliceMode::Line,
        skip_empty_lines: true,
    };
    let slicer = TextSlicer::with_dict_manager(config, dict_manager.clone());

    // 测试文本 1: 包含路径
    let test_text1 = "/home/user/project/src/main.rs:10: error: undefined variable 'x'";
    // 测试文本 2: 包含宏
    let test_text2 = "gcc -O2 -Wall -I/usr/include -c main.c -o main.o";
    // 测试文本 3: 包含编译命令
    let test_text3 = "[2024-01-01T12:00:00Z] /usr/bin/gcc -c source.c -o source.o";

    let test_cases = vec![
        ("包含路径", test_text1),
        ("包含宏", test_text2),
        ("包含编译命令", test_text3),
    ];

    for (description, text) in test_cases {
        println!("\n测试: {}", description);
        println!("文本: {}", text);

        let input = SliceInput {
            raw: Cow::Borrowed(text),
            offset: 0,
            line_number: 1,
            file_metadata: None,
        };

        let slice = slicer.slice_line(&input);

        println!("切片标记:");
        println!("  has_paths: {}", slice.flags.has_paths);
        println!("  has_macros: {}", slice.flags.has_macros);
        println!(
            "  has_compile_commands: {}",
            slice.flags.has_compile_commands
        );
    }

    // 等待字典管理器处理完所有消息
    std::thread::sleep(std::time::Duration::from_millis(100));

    // 获取字典
    let snapshot = dict_manager.snapshot();
    let path_dict = &snapshot.paths;
    let macro_dict = &snapshot.macros;
    let command_dict = &snapshot.flags;

    println!("\n生成的字典:");
    println!("路径字典大小: {} 条目", path_dict.len());
    println!("宏字典大小: {} 条目", macro_dict.len());
    println!("编译命令字典大小: {} 条目", command_dict.len());

    // 打印部分字典内容
    println!("\n路径字典示例:");
    for (token, path) in path_dict.iter().take(5) {
        println!("  {} -> {}", path, token);
    }

    println!("\n宏字典示例:");
    for (token, macro_str) in macro_dict.iter().take(5) {
        println!("  {} -> {}", macro_str, token);
    }

    println!("\n编译命令字典示例:");
    for (token, command) in command_dict.iter().take(5) {
        println!("  {} -> {}", command, token);
    }

    println!("\n测试完成！");
}
