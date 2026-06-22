use std::borrow::Cow;
use tokenslim::core::stream_reader::SliceInput;
use tokenslim::core::text_slicer::{SliceMode, SlicerConfig, TextSlicer};

fn main() {
    println!("测试 TextSlicer 切片标记功能");
    println!("====================================");

    // 测试文本 1: 包含路径
    let test_text1 = "/home/user/project/src/main.rs:10: error: undefined variable 'x'";
    // 测试文本 2: 包含宏
    let test_text2 = "gcc -O2 -Wall -I/usr/include -c main.c -o main.o";
    // 测试文本 3: 包含编译命令
    let test_text3 = "[2024-01-01T12:00:00Z] /usr/bin/gcc -c source.c -o source.o";
    // 测试文本 4: 包含堆栈跟踪
    let test_text4 = "at com.example.App.main(App.java:10)";
    // 测试文本 5: 包含日志头
    let test_text5 = "[2024-01-01T12:00:00Z] INFO: Application started";

    let test_cases = vec![
        ("包含路径", test_text1),
        ("包含宏", test_text2),
        ("包含编译命令", test_text3),
        ("包含堆栈跟踪", test_text4),
        ("包含日志头", test_text5),
    ];

    let config = SlicerConfig {
        mode: SliceMode::Line,
        skip_empty_lines: true,
    };

    let slicer = TextSlicer::new(config);

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

        println!("标记结果:");
        println!("  has_paths: {}", slice.flags.has_paths);
        println!("  has_macros: {}", slice.flags.has_macros);
        println!(
            "  has_compile_commands: {}",
            slice.flags.has_compile_commands
        );
        println!("  has_stack_trace: {}", slice.flags.has_stack_trace);
        println!("  has_log_headers: {}", slice.flags.has_log_headers);
    }

    println!("\n测试完成！");
}
