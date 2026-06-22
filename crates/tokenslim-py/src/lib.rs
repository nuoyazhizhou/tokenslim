extern crate tokenslim as tokenslim_crate;

use pyo3::prelude::*;
use pyo3::types::PyModule;
use tokenslim_crate::cli::get_plugins;
use tokenslim_crate::core::compression::CompressionOutput;
use tokenslim_crate::core::compression_pipeline::{CompressionPipeline, PipelineConfig};
use tokenslim_crate::core::metrics::{MetricsCollector, MetricsConfig};
use tokenslim_crate::core::rehydration_pipeline::{RehydrationConfig, RehydrationPipeline};

#[pyfunction]
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

#[pyfunction]
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

/// TokenSlim Python Bindings
#[pymodule]
fn tokenslim(_py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(compress, m)?)?;
    m.add_function(wrap_pyfunction!(decompress, m)?)?;
    Ok(())
}
