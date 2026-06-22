use std::sync::Arc;
use std::time::Instant;
use tokenslim::core::dictionary_manager::DictionaryManager;
use tokenslim::core::stream_reader::SliceInput;
use tokenslim::core::text_slicer::{SliceMode, SlicerConfig, TextSlicer};

fn main() {
    // 创建字典管理器
    let dict_manager = Arc::new(DictionaryManager::new());

    // 创建文本切片器
    let config = SlicerConfig {
        mode: SliceMode::Hybrid,
        skip_empty_lines: true,
    };
    let slicer = TextSlicer::with_dict_manager(config, dict_manager.clone());

    // 准备测试数据
    let test_texts = vec![
        "This is a test line with a path: /home/user/project/src/main.rs",
        "Another line with a macro: -DDEBUG=1",
        "A line with a compile command: gcc -c file.c -o file.o",
        "A line with a stack trace: at main() in file.c:10",
        "A line with a log header: [2023-01-01T12:00:00Z] INFO: Test message",
        "Line without special content",
        "Another line with a path: C:\\Windows\\System32\\Microsoft\\Windows\\Fonts\\Arial.ttf",
        "Line with multiple paths: /home/user/project/src/main.rs and /home/user/project/src/utils.rs",
        "Another line with the same path: /home/user/project/src/main.rs",
        "Another line with the Windows path: C:\\Windows\\System32\\Microsoft\\Windows\\Fonts\\Arial.ttf",
    ];

    // 准备SliceInput数据
    let inputs: Vec<SliceInput> = test_texts
        .into_iter()
        .enumerate()
        .map(|(i, text)| SliceInput {
            raw: text.into(),
            offset: i * 100,
            line_number: i + 1,
            file_metadata: None,
        })
        .collect();

    // 测试并行处理
    println!("测试并行处理...");
    let start = Instant::now();
    let slices = slicer.process_parallel(inputs);
    let duration = start.elapsed();

    println!("并行处理完成，耗时: {:?}", duration);
    println!("生成的切片数量: {}", slices.len());

    // 打印切片信息
    for slice in &slices {
        println!(
            "切片 ID: {}, 类型: {:?}, 标记: {:?}",
            slice.id, slice.slice_type, slice.flags
        );
        println!("  内容: {}", slice.text);
    }

    // 等待字典管理器处理完成
    std::thread::sleep(std::time::Duration::from_millis(500));

    // 打印字典信息
    let dict = dict_manager.snapshot();
    println!("\n字典信息:");
    println!(
        "路径/目录字典大小: {}",
        dict.paths.len() + dict.directories.len()
    );
    println!("宏字典大小: {}", dict.macros.len());
    println!("编译命令字典大小: {}", dict.flags.len());

    // 打印路径字典内容
    println!("\n路径字典内容:");
    for (token, path) in &dict.paths {
        println!("  {} -> {}", path, token);
    }

    // 打印宏字典内容
    println!("\n宏字典内容:");
    for (token, macro_str) in &dict.macros {
        println!("  {} -> {}", macro_str, token);
    }

    // 打印编译命令字典内容
    println!("\n编译命令字典内容:");
    for (token, command) in &dict.flags {
        println!("  {} -> {}", command, token);
    }

    println!("\n测试完成!");
}
