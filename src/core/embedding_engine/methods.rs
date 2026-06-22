use crate::core::embedding_engine::types::{ClassifierConfig, SemanticClassifier, SignatureVector};
#[cfg(feature = "ml")]
use crate::utils::i18n::t1;
use candle_core::{DType, Device, Tensor};
use candle_nn::VarBuilder;
use candle_transformers::models::bert::{BertModel, Config};
use hf_hub::{api::sync::Api, Repo, RepoType};
use std::error::Error;
use tokenizers::Tokenizer;

#[cfg(feature = "ml")]
impl SemanticClassifier {
    /// 初始化分类器：从 HuggingFace 下载/加载模型和分词器。
    ///
    /// 默认使用 CPU 进行推理以获得最大兼容性。
    pub fn new(config: ClassifierConfig) -> Result<Self, Box<dyn Error>> {
        let device = Device::Cpu; // Default to CPU for maximum compatibility

        let api = Api::new()?;
        let repo = api.repo(Repo::with_revision(
            config.model_id.clone(),
            RepoType::Model,
            config.revision.clone(),
        ));

        // 1. Load Tokenizer
        let tokenizer_filename = repo.get("tokenizer.json")?;
        let tokenizer = Tokenizer::from_file(tokenizer_filename)
            .map_err(|e| format!("E_EMBEDDING_LOAD_TOKENIZER:{e}"))?;

        // 2. Load Config
        let config_filename = repo.get("config.json")?;
        let config_json = std::fs::read_to_string(config_filename)?;
        let bert_config: Config = serde_json::from_str(&config_json)?;

        // 3. Load Weights
        let weights_filename = repo.get("model.safetensors")?;
        let vb = unsafe {
            VarBuilder::from_mmaped_safetensors(&[weights_filename], DType::F32, &device)?
        };

        // 4. Initialize Model
        let model = BertModel::load(vb, &bert_config)?;

        Ok(Self {
            config,
            tokenizer,
            model,
            signatures: Vec::new(),
            device,
        })
    }

    /// 从 JSON 配置文件批量加载插件语义指纹。
    pub fn load_signatures_from_file<P: AsRef<std::path::Path>>(
        &mut self,
        path: P,
    ) -> Result<(), Box<dyn Error>> {
        let content = std::fs::read_to_string(path)?;
        let json: serde_json::Value = serde_json::from_str(&content)?;

        if let Some(sigs) = json.get("signatures").and_then(|v| v.as_array()) {
            for sig in sigs {
                let plugin_name = sig
                    .get("plugin")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
                if let Some(examples) = sig.get("examples").and_then(|v| v.as_array()) {
                    for ex in examples {
                        if let Some(text) = ex.as_str() {
                            let _ = self.add_signature(plugin_name, text);
                        }
                    }
                }
            }
        }

        log::info!(
            "{}",
            t1("core_embedding_signatures_loaded", self.signatures.len())
        );
        Ok(())
    }

    /// 为特定插件添加一条语义特征样本。
    pub fn add_signature(
        &mut self,
        plugin_name: &str,
        example_text: &str,
    ) -> Result<(), Box<dyn Error>> {
        let embedding = self.get_embedding(example_text)?;
        self.signatures.push(SignatureVector {
            plugin_name: plugin_name.to_string(),
            embedding,
        });
        Ok(())
    }

    /// 计算给定文本字符串的语义嵌入向量（Embedding）。
    ///
    /// 流程：分词 -> 前向传播 -> 均值池化 -> L2 归一化。
    pub fn get_embedding(&self, text: &str) -> Result<Tensor, Box<dyn Error>> {
        let tokens = self
            .tokenizer
            .encode(text, true)
            .map_err(|e| format!("E_EMBEDDING_TOKENIZATION:{e}"))?;

        let token_ids = tokens.get_ids();
        let token_ids_tensor = Tensor::new(token_ids, &self.device)?.unsqueeze(0)?;
        let token_type_ids =
            Tensor::new(vec![0u32; token_ids.len()], &self.device)?.unsqueeze(0)?;

        // Forward pass
        let ys = self
            .model
            .forward(&token_ids_tensor, &token_type_ids, None)?;

        // Mean pooling: average over the sequence length dimension
        let (_n_batch, n_tokens, _hidden_size) = ys.dims3()?;
        let embeddings = (ys.sum(1)? / (n_tokens as f64))?;

        // L2 Normalize
        let norm = embeddings.sqr()?.sum_all()?.sqrt()?;
        let normalized = (embeddings / norm)?;

        Ok(normalized)
    }

    /// 对未知文本切片进行语义分类。
    ///
    /// 通过计算输入文本与已知插件特征库之间的余弦相似度，选出最可能的插件。
    pub fn classify(&self, text: &str) -> Option<String> {
        let embedding = match self.get_embedding(text) {
            Ok(e) => e,
            Err(_) => return None,
        };

        let mut best_plugin = None;
        let mut max_similarity = 0.0f32;

        for sig in &self.signatures {
            // Cosine Similarity: dot product since vectors are normalized
            if let Ok(res) = embedding.matmul(&sig.embedding.t().ok()?) {
                if let Ok(vec2) = res.to_vec2::<f32>() {
                    let dot_product = vec2[0][0];
                    if dot_product > max_similarity {
                        max_similarity = dot_product;
                        best_plugin = Some(sig.plugin_name.clone());
                    }
                }
            }
        }

        if max_similarity >= self.config.confidence_threshold {
            best_plugin
        } else {
            None
        }
    }
}
