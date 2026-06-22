#[cfg(test)]
mod tests {
    use crate::core::compression::Token;
    use crate::core::dedup_engine::{DedupConfig, DedupEngine};
    use crate::core::dictionary_engine::DictionaryEngine;
    use crate::core::plugin_dispatcher::Plugin;
    use crate::core::text_slicer::{Slice, SliceType};
    use crate::plugins::kubernetes_docker_plugin::KubernetesDockerPlugin;
    use std::borrow::Cow;

    fn read_sample(file_name: &str) -> String {
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let path = std::path::Path::new(manifest_dir)
            .join("samples")
            .join("kubernetes_docker_plugin")
            .join(file_name);
        std::fs::read_to_string(&path).unwrap_or_default()
    }

    fn compress_text(plugin: &KubernetesDockerPlugin, text: &str) -> String {
        let slice = Slice {
            id: 1,
            text: Cow::Borrowed(text),
            slice_type: SliceType::LogBlock,
            offset: 0,
            line_start: 1,
            line_end: text.lines().count().max(1),
            file_metadata: None,
            flags: Default::default(),
        };
        let mut dict = DictionaryEngine::new();
        let mut dedup = DedupEngine::new(DedupConfig::default());
        let arena = bumpalo::Bump::new();
        let result = plugin.compress(&slice, &mut dict, &mut dedup, &arena);
        result
            .tokens
            .iter()
            .filter_map(|t| match t {
                Token::Text(s) => Some(s.as_ref()),
                _ => None,
            })
            .collect::<String>()
    }

    #[test]
    fn generate_kubernetes_docker_showcase_report() {
        let plugin = KubernetesDockerPlugin::new();
        let cases = [
            ("case_001_kubectl_pod.log", "Kubectl Pod"),
            ("case_002_docker_ps.log", "Docker PS"),
            ("case_003_kubectl_logs.log", "Kubectl Logs"),
            ("case_004_docker_build.log", "Docker Build"),
            ("case_005_kubectl_error.log", "Kubectl错误"),
            ("case_006_noise.log", "带噪声"),
            ("case_007_kubectl_deploy.log", "Kubectl Deploy"),
            ("case_008_empty.log", "空输入"),
            ("case_009_docker_error.log", "Docker错误"),
            ("case_010_mixed.log", "混合场景"),
            ("case_011_no_compress.log", "不压缩场景"),
            ("case_012_kubectl_describe.log", "Kubectl Describe"),
            ("case_013_real_pod_hash.log", "Real Pod Hash"),
            ("case_014_cloudwatch_json.log", "CloudWatch JSON"),
            (
                "case_015_looks_k8s_but_nginx_access.log",
                "Nginx access false positive guard",
            ),
            (
                "case_016_docker_buildkit_failure.log",
                "Docker BuildKit failure",
            ),
            (
                "case_017_docker_buildx_multi_platform.log",
                "Docker buildx multi-platform",
            ),
            (
                "case_018_docker_compose_up_build.log",
                "Docker Compose up build",
            ),
            (
                "case_019_docker_compose_failure.log",
                "Docker Compose failure",
            ),
            (
                "case_020_kubectl_rollout_success.log",
                "Kubectl rollout success",
            ),
            (
                "case_021_kubectl_rollout_timeout.log",
                "Kubectl rollout timeout",
            ),
            (
                "case_022_kubectl_apply_summary.log",
                "Kubectl apply summary",
            ),
            ("case_023_kubectl_diff.log", "Kubectl diff"),
            (
                "case_024_kubectl_logs_previous_crashloop.log",
                "Kubectl logs previous crashloop",
            ),
        ];

        let mut all_output = String::new();
        all_output.push_str(&"=".repeat(80));
        all_output.push_str("\n  Kubernetes/Docker AI Compact Showcase\n");
        all_output.push_str(&"=".repeat(80));
        all_output.push_str("\n\n");

        for (file_name, title) in cases {
            let raw = read_sample(file_name);

            if raw.is_empty() {
                all_output.push_str(&format!(
                    "[SKIP] {} - file not found or empty: {}\n\n",
                    title, file_name
                ));
                continue;
            }

            let case_id = file_name.trim_end_matches(".log");
            let original_lines = raw.lines().count();
            let original_bytes = raw.len();
            let compacted = compress_text(&plugin, &raw);
            let compact_lines = if compacted.is_empty() {
                0
            } else {
                compacted.lines().count()
            };
            let compact_bytes = compacted.len();
            let compression_ratio = if original_bytes > 0 {
                (1.0 - compact_bytes as f64 / original_bytes as f64) * 100.0
            } else {
                0.0
            };

            all_output.push_str(&"-".repeat(80));
            all_output.push_str(&format!("\nCase {} - {} ({})\n", case_id, title, file_name));
            all_output.push_str(&"-".repeat(80));
            all_output.push_str(&format!(
                "\nOriginal: {} lines, {} bytes | Compact: {} lines, {} bytes | Compression: {:.1}%\n",
                original_lines, original_bytes, compact_lines, compact_bytes, compression_ratio
            ));

            all_output.push_str("-- Case text --\n");
            all_output.push_str(&"-".repeat(80));
            all_output.push_str("\n");
            all_output.push_str(&raw);
            if !all_output.ends_with('\n') {
                all_output.push('\n');
            }

            all_output.push_str("-- Compact Output (full) --\n");
            all_output.push_str(&"-".repeat(80));
            all_output.push_str("\n");
            all_output.push_str(&compacted);
            if !all_output.ends_with('\n') {
                all_output.push('\n');
            }
        }

        std::fs::write(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("target")
                .join("kubernetes_docker_compact_showcase_report.txt"),
            &all_output,
        )
        .unwrap();
    }
}
