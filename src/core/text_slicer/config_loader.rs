//! 框架检测配置加载器
//!
//! # 功能说明
//! 从 config/frameworks 目录动态加载 JSON 配置文件

use super::types::*;
use crate::utils::i18n::t2;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

const E_FRAMEWORK_CONFIG_READ_DIR: &str = "E_FRAMEWORK_CONFIG_READ_DIR";
const E_FRAMEWORK_CONFIG_READ_ENTRY: &str = "E_FRAMEWORK_CONFIG_READ_ENTRY";
const E_FRAMEWORK_CONFIG_READ_FILE: &str = "E_FRAMEWORK_CONFIG_READ_FILE";
const E_FRAMEWORK_CONFIG_PARSE_JSON: &str = "E_FRAMEWORK_CONFIG_PARSE_JSON";

/// 配置文件格式（与 FrameworkDetectionConfig 对应）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrameworkConfigFile {
    pub name: String,
    pub slice_type: SliceType,
    pub priority: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub rules: Vec<DetectionRuleFile>,
}

/// 配置文件中的规则格式
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DetectionRuleFile {
    Any {
        patterns: Vec<FeaturePattern>,
    },
    All {
        patterns: Vec<FeaturePattern>,
    },
    Combo {
        required: Vec<FeaturePattern>,
        optional: Vec<FeaturePattern>,
    },
}

impl From<DetectionRuleFile> for DetectionRule {
    /// 将文件格式的检测规则（DetectionRuleFile）转换为运行时规则（DetectionRule）。
    fn from(rule: DetectionRuleFile) -> Self {
        match rule {
            DetectionRuleFile::Any { patterns } => DetectionRule::Any(patterns),
            DetectionRuleFile::All { patterns } => DetectionRule::All(patterns),
            DetectionRuleFile::Combo { required, optional } => {
                DetectionRule::Combo { required, optional }
            }
        }
    }
}

impl From<FrameworkConfigFile> for FrameworkDetectionConfig {
    /// 将 `FrameworkConfigFile` 反序列化结构转换为运行期 `FrameworkDetectionConfig`：名称以 `Box::leak` 长期持有，检测规则逐条映射。
    fn from(file: FrameworkConfigFile) -> Self {
        FrameworkDetectionConfig {
            name: Box::leak(file.name.into_boxed_str()),
            slice_type: file.slice_type,
            rules: file.rules.into_iter().map(|r| r.into()).collect(),
            priority: file.priority,
        }
    }
}

/// 配置加载器
pub struct ConfigLoader {
    config_dir: PathBuf,
}

impl ConfigLoader {
    /// 创建配置加载器（使用默认目录）。
    ///
    /// P1-09 修复：`config_dir` 统一落点为 `<root>/config`（此前三条策略都
    /// 额外 `.join("frameworks")`，而 [`load_all_configs`](Self::load_all_configs)
    /// 又各拼一次 `frameworks`/`languages`，实际查找 `config/frameworks/frameworks`
    /// 等不存在目录，35 个配置文件 100% 静默未加载）。
    pub fn new() -> Self {
        // 策略 1：优先检查当前工作目录的 config
        let cwd_config = PathBuf::from("./config");
        if cwd_config.exists() {
            return ConfigLoader {
                config_dir: cwd_config,
            };
        }

        // 策略 2：检查可执行文件所在目录的 config
        let exe_dir = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|p| p.to_path_buf()))
            .unwrap_or_else(|| PathBuf::from("."));

        let exe_config = exe_dir.join("config");
        if exe_config.exists() {
            return ConfigLoader {
                config_dir: exe_config,
            };
        }

        // 策略 3：检查可执行文件上级目录的 config（开发环境）
        let dev_config = exe_dir.join("../../config");
        ConfigLoader {
            config_dir: dev_config,
        }
    }

    /// 创建配置加载器（指定目录）
    pub fn with_dir<P: AsRef<Path>>(config_dir: P) -> Self {
        ConfigLoader {
            config_dir: config_dir.as_ref().to_path_buf(),
        }
    }

    /// 加载所有框架配置（包括 frameworks 和 languages）。
    ///
    /// P1-09/P2-25：两个目录**都不存在**时升级为 `log::error`——旧实现静默
    /// 返回空表，配置整体丢失时用户零感知（正是 P1-09 能潜伏至今的原因）。
    pub fn load_all_configs(&self) -> Result<Vec<FrameworkDetectionConfig>, String> {
        let mut configs = Vec::new();

        // 加载 frameworks 目录
        let frameworks_dir = self.config_dir.join("frameworks");
        // 加载 languages 目录
        let languages_dir = self.config_dir.join("languages");

        if !frameworks_dir.exists() && !languages_dir.exists() {
            log::error!(
                "E_FRAMEWORK_CONFIG_DIR_MISSING:{:?} (frameworks/ and languages/ both absent;                  framework/language detection will run with ZERO configs)",
                self.config_dir
            );
        }

        if frameworks_dir.exists() {
            configs.append(&mut self.load_configs_from_dir(&frameworks_dir)?);
        }

        if languages_dir.exists() {
            configs.append(&mut self.load_configs_from_dir(&languages_dir)?);
        }

        // 按优先级排序（优先级高的先检测）
        configs.sort_by(|a, b| b.priority.cmp(&a.priority));

        Ok(configs)
    }

    /// 从指定目录加载所有配置文件
    fn load_configs_from_dir(&self, dir: &Path) -> Result<Vec<FrameworkDetectionConfig>, String> {
        let mut configs = Vec::new();

        let entries =
            fs::read_dir(dir).map_err(|e| format!("{E_FRAMEWORK_CONFIG_READ_DIR}:{dir:?}:{e}"))?;

        for entry in entries {
            let entry = entry.map_err(|e| format!("{E_FRAMEWORK_CONFIG_READ_ENTRY}:{e}"))?;
            let path = entry.path();

            // 只处理 JSON 文件
            if path.extension().and_then(|s| s.to_str()) != Some("json") {
                continue;
            }

            // 加载配置文件
            match self.load_config(&path) {
                Ok(config) => {
                    log::info!(
                        "{}",
                        t2(
                            "core_framework_config_loaded",
                            &config.name,
                            format!("{:?}", path)
                        )
                    );
                    configs.push(config);
                }
                Err(e) => {
                    log::warn!(
                        "{}",
                        t2(
                            "core_framework_config_load_failed",
                            format!("{:?}", path),
                            e
                        )
                    );
                }
            }
        }

        Ok(configs)
    }

    /// 加载单个配置文件
    pub fn load_config<P: AsRef<Path>>(&self, path: P) -> Result<FrameworkDetectionConfig, String> {
        let path_ref = path.as_ref();
        let content = fs::read_to_string(path_ref)
            .map_err(|e| format!("{E_FRAMEWORK_CONFIG_READ_FILE}:{path_ref:?}:{e}"))?;

        let config_file: FrameworkConfigFile = serde_json::from_str(&content)
            .map_err(|e| format!("{E_FRAMEWORK_CONFIG_PARSE_JSON}:{path_ref:?}:{e}"))?;

        Ok(config_file.into())
    }
}

impl Default for ConfigLoader {
    /// ConfigLoader 默认实现：等价于 new()，自动探测配置目录。
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod p1_09_tests {
    use super::ConfigLoader;

    /// P1-09 回归：正确基准目录下 frameworks/ 与 languages/ 配置全量加载
    /// （旧实现 `new()` 落点到 `config/frameworks` 后又各拼一次子目录，
    /// 实际查找 `config/frameworks/frameworks`——35 个配置 100% 静默未加载）。
    #[test]
    fn loads_configs_from_frameworks_and_languages_dirs() {
        let dir = std::env::temp_dir().join(format!("tokenslim_p109_{}", std::process::id()));
        let fw = dir.join("frameworks");
        let lang = dir.join("languages");
        std::fs::create_dir_all(&fw).unwrap();
        std::fs::create_dir_all(&lang).unwrap();

        let fw_conf = super::FrameworkConfigFile {
            name: "vue".to_string(),
            slice_type: crate::core::text_slicer::SliceType::Line,
            priority: 10,
            description: None,
            rules: vec![],
        };
        std::fs::write(
            fw.join("vue.json"),
            serde_json::to_string(&fw_conf).unwrap(),
        )
        .unwrap();
        let lang_conf = super::FrameworkConfigFile {
            name: "rust-lang".to_string(),
            slice_type: crate::core::text_slicer::SliceType::Line,
            priority: 5,
            description: None,
            rules: vec![],
        };
        std::fs::write(
            lang.join("rust.json"),
            serde_json::to_string(&lang_conf).unwrap(),
        )
        .unwrap();

        let loader = ConfigLoader::with_dir(&dir);
        let configs = loader.load_all_configs().unwrap();
        std::fs::remove_dir_all(&dir).ok();
        assert_eq!(
            configs.len(),
            2,
            "frameworks + languages 各 1 个配置应全量加载（P1-09）"
        );
    }

    /// P1-09/P2-25 守护：两目录皆不存在时返回空表但记 error（不再纯静默）。
    /// 此处只验证 Fail-Soft 行为保持（返回 Ok 空表）。
    #[test]
    fn missing_dirs_fail_soft_with_empty_result() {
        let dir = std::env::temp_dir().join(format!("tokenslim_p109b_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let loader = ConfigLoader::with_dir(&dir);
        let configs = loader.load_all_configs().unwrap();
        std::fs::remove_dir_all(&dir).ok();
        assert!(configs.is_empty());
    }
}
