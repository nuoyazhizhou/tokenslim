extern crate tokenslim as tokenslim_crate;

use pyo3::prelude::*;
use pyo3::types::PyModule;
use tokenslim_crate::cli::get_plugins;
use tokenslim_crate::core::compression::CompressionOutput;
use tokenslim_crate::core::compression_pipeline::{CompressionPipeline, PipelineConfig};
use tokenslim_crate::core::metrics::{MetricsCollector, MetricsConfig};
use tokenslim_crate::core::rehydration_pipeline::{RehydrationConfig, RehydrationPipeline};

/// Python 绑定 `compress`：以默认配置构建压缩流水线，对输入文本执行 Token 压缩，并将 `CompressionOutput` 序列化为 JSON 字符串返回。
#[pyfunction]
/// Python 绑定：用完整插件链压缩文本，返回序列化为 JSON 的 `CompressionOutput`。
fn compress(text: String) -> PyResult<String> {
    let config = PipelineConfig::default();
    let mut pipeline = CompressionPipeline::new(
        config,
        get_plugins(),
        MetricsCollector::new(MetricsConfig::default()),
    );

    let output = pipeline.compress_str(&text).map_err(|e| {
        PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!("Compression error: {e}"))
    })?;

    serde_json::to_string(&output).map_err(|e| {
        PyErr::new::<pyo3::exceptions::PyValueError, _>(format!("Serialize error: {e}"))
    })
}

/// Python 绑定 `decompress`：解析 `CompressionOutput` JSON，以重水合流水线还原原始文本并返回。
#[pyfunction]
/// Python 绑定：反序列化 JSON 为 `CompressionOutput`，经还原流水线恢复原始文本。
fn decompress(output_json: String) -> PyResult<String> {
    let output: CompressionOutput = serde_json::from_str(&output_json).map_err(|e| {
        PyErr::new::<pyo3::exceptions::PyValueError, _>(format!("Deserialize error: {e}"))
    })?;

    let pipeline = RehydrationPipeline::new(
        output.dictionary.clone(),
        get_plugins(),
        RehydrationConfig::default(),
    );

    pipeline.rehydrate(&output).map_err(|e| {
        PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!("Decompression error: {e}"))
    })
}

/// 注册 Python 绑定模块：导出 `compress`/`decompress` 函数。
#[pymodule]
fn tokenslim(_py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(compress, m)?)?;
    m.add_function(wrap_pyfunction!(decompress, m)?)?;
    Ok(())
}
