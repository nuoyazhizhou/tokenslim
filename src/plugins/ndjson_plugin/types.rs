//! NDJSON 插件类型定义
//!
//! # 类型概述
//!
//! 本模块定义了 NDJSON 插件所需的核心数据类型，包括：
//! - Go test -json 事件类型
//! - 测试结果聚合结构
//! - 插件配置

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

/// NDJSON 插件配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NdjsonConfig {
    /// 是否启用 Go test 特定优化
    #[serde(default = "default_true")]
    pub go_test_mode: bool,

    /// 最大输出行数（超过则截断）
    #[serde(default = "default_max_output_lines")]
    pub max_output_lines: usize,

    /// 是否显示详细的测试输出
    #[serde(default = "default_false")]
    pub show_test_output: bool,
}

fn default_true() -> bool {
    true
}

fn default_false() -> bool {
    false
}

fn default_max_output_lines() -> usize {
    10
}

impl Default for NdjsonConfig {
    fn default() -> Self {
        Self {
            go_test_mode: true,
            max_output_lines: 10,
            show_test_output: false,
        }
    }
}

/// NDJSON 压缩插件主结构
pub struct NdjsonPlugin {
    pub(crate) name: &'static str,
    pub(crate) priority: u8,
    pub(crate) ndjson_detect_pattern: Arc<Regex>,
    pub(crate) go_test_pattern: Arc<Regex>,
    pub config: NdjsonConfig,
}

/// Go test -json 事件类型
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoTestEvent {
    /// 时间戳
    #[serde(rename = "Time")]
    pub time: Option<String>,

    /// 动作类型：run, pause, cont, pass, fail, skip, output, bench
    #[serde(rename = "Action")]
    pub action: String,

    /// 包名
    #[serde(rename = "Package")]
    pub package: Option<String>,

    /// 测试名称
    #[serde(rename = "Test")]
    pub test: Option<String>,

    /// 输出内容（仅 action=output 时有值）
    #[serde(rename = "Output")]
    pub output: Option<String>,

    /// 耗时（秒）
    #[serde(rename = "Elapsed")]
    pub elapsed: Option<f64>,
}

/// 测试结果
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TestResult {
    Pass,
    Fail,
    Skip,
}

/// 单个测试的聚合信息
#[derive(Debug, Clone)]
pub struct TestInfo {
    pub name: String,
    pub result: TestResult,
    pub elapsed: Option<f64>,
    pub output: Vec<String>,
}

/// 单个包的聚合信息
#[derive(Debug, Clone)]
pub struct PackageInfo {
    pub name: String,
    pub tests: HashMap<String, TestInfo>,
    pub passed: usize,
    pub failed: usize,
    pub skipped: usize,
}

impl PackageInfo {
    pub fn new(name: String) -> Self {
        Self {
            name,
            tests: HashMap::new(),
            passed: 0,
            failed: 0,
            skipped: 0,
        }
    }

    /// 添加或更新测试信息
    pub fn update_test(&mut self, test_name: String, result: TestResult, elapsed: Option<f64>) {
        // 检查测试是否已存在
        let is_new = !self.tests.contains_key(&test_name);

        // 获取或创建测试信息
        let test_info = self
            .tests
            .entry(test_name.clone())
            .or_insert_with(|| TestInfo {
                name: test_name,
                result: result.clone(),
                elapsed,
                output: Vec::new(),
            });

        if is_new {
            // 新测试，直接添加计数
            match result {
                TestResult::Pass => self.passed += 1,
                TestResult::Fail => self.failed += 1,
                TestResult::Skip => self.skipped += 1,
            }
        } else {
            // 已存在的测试，检查结果是否改变
            let old_result = test_info.result.clone();
            if old_result != result {
                // 减去旧的计数
                match old_result {
                    TestResult::Pass => {
                        if self.passed > 0 {
                            self.passed -= 1;
                        }
                    }
                    TestResult::Fail => {
                        if self.failed > 0 {
                            self.failed -= 1;
                        }
                    }
                    TestResult::Skip => {
                        if self.skipped > 0 {
                            self.skipped -= 1;
                        }
                    }
                }

                // 添加新的计数
                match result {
                    TestResult::Pass => self.passed += 1,
                    TestResult::Fail => self.failed += 1,
                    TestResult::Skip => self.skipped += 1,
                }
            }
        }

        // 更新测试信息
        test_info.result = result;
        test_info.elapsed = elapsed;
    }

    /// 添加测试输出
    pub fn add_test_output(&mut self, test_name: &str, output: String) {
        if let Some(test_info) = self.tests.get_mut(test_name) {
            test_info.output.push(output);
        }
    }
}
