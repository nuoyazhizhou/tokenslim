//! 日志模板挖掘工具 (Log Miner)
//! 基于 Drain 算法自动发现日志模式并生成 TokenSlim 配置文件和 AI 提示词

use clap::Parser;
use std::fs;
use std::path::PathBuf;
use std::time::Instant;
use tokenslim::core::content_analyzer::drain::{DrainConfig, DrainManager};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// 输入日志文件路径
    #[arg(short, long)]
    input: PathBuf,

    /// 输出配置文件路径 (JSON)
    #[arg(short, long, default_value = "extracted_templates.json")]
    output: PathBuf,

    /// 相似度阈值 (0.0 - 1.0)
    #[arg(long, default_value_t = 0.5)]
    threshold: f32,

    /// 树的最大深度
    #[arg(long, default_value_t = 4)]
    depth: usize,
}

fn main() {
    let args = Args::parse();

    println!("🚀 Starting Log Miner...");
    println!("   Input: {:?}", args.input);
    println!(
        "   Config: threshold={}, depth={}",
        args.threshold, args.depth
    );

    let content = fs::read_to_string(&args.input).expect("Failed to read input file");
    let lines: Vec<&str> = content.lines().filter(|l| !l.trim().is_empty()).collect();

    let config = DrainConfig {
        sim_threshold: args.threshold,
        max_depth: args.depth,
        ..DrainConfig::default()
    };

    let mut manager = DrainManager::new(config);

    let start = Instant::now();
    let ts_re =
        regex::Regex::new(r"^\[?\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}[\.\d]*Z?\]?\s*").unwrap();
    let ts_re_alt = regex::Regex::new(r"^\d{4}/\d{2}/\d{2}\s+\d{2}:\d{2}:\d{2}\s*").unwrap();

    for (i, line) in lines.iter().enumerate() {
        // Strip timestamps to get better templates
        let stripped = ts_re.replace(line, "");
        let stripped = ts_re_alt.replace(&stripped, "");

        manager.add_log_message(&stripped);
        if i > 0 && i % 10000 == 0 {
            println!("   Processed {} lines...", i);
        }
    }
    let duration = start.elapsed();

    let clusters = manager.get_templates();

    println!("\n✅ Mining Complete!");
    println!("   Processed {} lines in {:?}", lines.len(), duration);
    println!("   Discovered {} unique templates", clusters.len());

    // 1. 导出 TokenSlim 插件配置 (TemplateConfig)
    use tokenslim::plugins::template_driven_plugin::types::{
        TemplateConfig, TemplateDrivenPlugin, TemplateRule,
    };

    let rules: Vec<TemplateRule> = clusters
        .iter()
        .filter(|c| c.size > 5) // 只保留出现 5 次以上的模式
        .map(|c| TemplateRule {
            name: format!("cluster_{}", c.id),
            pattern: TemplateDrivenPlugin::build_regex_from_template(&c.template),
            template: Some(c.template.join(" ")),
            confidence: 0.9,
        })
        .collect();

    let plugin_config = TemplateConfig { rules };
    let json = serde_json::to_string_pretty(&plugin_config).expect("Failed to serialize config");
    fs::write(&args.output, json).expect("Failed to write output file");
    println!("   Config saved to: {:?}", args.output);

    // 2. 生成 AI 提示词
    generate_ai_prompt(&clusters);
}

fn generate_ai_prompt(clusters: &[tokenslim::core::content_analyzer::drain::LogCluster]) {
    println!("\n--- 🤖 AI ASSISTANT PROMPT ---");
    println!("Copy the text below to your AI (ChatGPT/Claude) to refine these templates:\n");

    let mut prompt = String::new();
    prompt.push_str(
        "I have used the Drain algorithm to extract log templates from a raw log file. \n",
    );
    prompt.push_str("Below are the top 20 most frequent templates discovered. \n");
    prompt.push_str("Please help me: \n");
    prompt.push_str("1. Identify common variables (like IDs, Timestamps, IPs) and suggest Regex patterns for them.\n");
    prompt.push_str("2. Merge very similar templates into a single generic rule.\n");
    prompt.push_str("3. Format the result as a TokenSlim plugin configuration (JSON).\n\n");
    prompt.push_str("Top Templates:\n");

    // 按频率排序并取前 20
    let mut sorted = clusters.to_vec();
    sorted.sort_by(|a, b| b.size.cmp(&a.size));

    for (i, c) in sorted.iter().take(20).enumerate() {
        let template_str = c.template.join(" ");
        prompt.push_str(&format!("{}. [Freq: {}] {}\n", i + 1, c.size, template_str));
    }

    println!("{}", prompt);
    println!("-----------------------------\n");
}
