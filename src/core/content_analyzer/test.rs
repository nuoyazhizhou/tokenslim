//! content analyzer 测试模块
//!
//! # 测试概述
//!
//! 本模块包含 content analyzer 模块的单元测试和集成测试。
//! 测试覆盖了主要功能和边界情况。

#[cfg(test)]
mod tests {
    use crate::core::content_analyzer::ContentAnalyzer;

    /// 确定性摘要锚点：VSTest 运行头（`Test run for <...>.dll`）命中 → document_category 定 Dotnet。
    #[test]
    fn test_document_category_dotnet_anchor() {
        use crate::core::content_classifier::Category;
        let dotnet = include_str!("../../../samples/dotnet_plugin/case_005_test_results.log");
        let analyzer = ContentAnalyzer::new();
        assert_eq!(analyzer.document_category(dotnet), Some(Category::Dotnet));
    }

    /// 反向防误伤：非 dotnet 高识别样本（pulumi）不应被 dotnet 摘要锚点认领。
    #[test]
    fn test_document_category_dotnet_anchor_no_false_positive() {
        use crate::core::content_classifier::Category;
        let pulumi = include_str!("../../../samples/pulumi_plugin/case_001_preview.log");
        let analyzer = ContentAnalyzer::new();
        assert_ne!(
            analyzer.document_category(pulumi),
            Some(Category::Dotnet),
            "pulumi 不应被 dotnet 摘要锚点误判"
        );
    }

    /// 确定性摘要锚点：Rust/Cargo 编译错误签名（`error[E0xxx]:`）命中 → document_category 定 Cargo。
    #[test]
    fn test_document_category_cargo_error_anchor() {
        use crate::core::content_classifier::Category;
        let cargo =
            include_str!("../../../classifier_holdout/bayesian/cargo/case_001_mystery_crate.log");
        let analyzer = ContentAnalyzer::new();
        assert_eq!(analyzer.document_category(cargo), Some(Category::Cargo));
    }

    /// 反向防误伤：gcc/clang 无编号 `error:`（非 `error[E…]:`）不应被 cargo 错误锚点认领。
    #[test]
    fn test_document_category_cargo_error_anchor_no_false_positive() {
        use crate::core::content_classifier::Category;
        let gcc =
            include_str!("../../../classifier_holdout/bayesian/gcc/case_001_no_match_call.log");
        let analyzer = ContentAnalyzer::new();
        assert_ne!(
            analyzer.document_category(gcc),
            Some(Category::Cargo),
            "gcc 无编号 error: 不应被 cargo 错误锚点误判"
        );
    }
}
