use candle_core::{Device, Tensor};
use candle_transformers::models::bert::BertModel;

/// Classifier settings
#[derive(Debug, Clone)]
pub struct ClassifierConfig {
    pub model_id: String,
    pub revision: String,
    pub confidence_threshold: f32,
}

impl Default for ClassifierConfig {
    fn default() -> Self {
        Self {
            model_id: "sentence-transformers/all-MiniLM-L6-v2".to_string(),
            revision: "refs/pr/21".to_string(),
            confidence_threshold: 0.85,
        }
    }
}

/// A pre-computed vector representing a plugin's signature log
pub struct SignatureVector {
    pub plugin_name: String,
    pub embedding: Tensor,
}

/// The core AI Semantic Classifier
pub struct SemanticClassifier {
    pub(crate) config: ClassifierConfig,
    pub(crate) tokenizer: tokenizers::Tokenizer,
    pub(crate) model: BertModel,
    pub(crate) signatures: Vec<SignatureVector>,
    pub(crate) device: Device,
}
