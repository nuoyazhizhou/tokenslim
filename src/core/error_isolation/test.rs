//! error isolation 测试模块
//!
//! # 测试概述
//!
//! 本模块包含 error isolation 模块的单元测试和集成测试。
//! 测试覆盖了主要功能和边界情况。

#[cfg(test)]
mod tests {
    use crate::core::error_isolation::{ExecutionError, SafeExecutor, SafeExecutorConfig};

    /// 验证 `catch_panic` 在闭包 panic 时返回 `ExecutionError::Panic`
    /// （payload 消息由实现记录到日志，不再静默丢弃——P3-34①）。
    #[test]
    fn catch_panic_maps_panic_to_execution_error() {
        let executor = SafeExecutor::new(SafeExecutorConfig::default());
        let result = executor.catch_panic(|| -> usize {
            panic!("boom with message");
        });
        assert!(matches!(result, Err(ExecutionError::Panic)));
    }

    /// 验证 `catch_panic` 在正常路径透传返回值。
    #[test]
    fn catch_panic_passes_through_normal_result() {
        let executor = SafeExecutor::new(SafeExecutorConfig::default());
        let result: Result<u32, ExecutionError> = executor.catch_panic(|| 41 + 1);
        assert_eq!(result.unwrap(), 42);
    }

    /// 验证 `execute` 带超时在闭包及时完成时返回正常结果。
    #[test]
    fn execute_with_timeout_returns_value_in_time() {
        let executor = SafeExecutor::new(SafeExecutorConfig::default());
        let result = executor.execute(|| 7u32, Some(std::time::Duration::from_secs(2)));
        assert_eq!(result.unwrap(), 7);
    }

    /// 验证 `validate` 对通过/不通过两种校验结果分别返回 Ok 与 `InvalidResult`。
    #[test]
    fn validate_reports_invalid_result() {
        let executor = SafeExecutor::new(SafeExecutorConfig::default());
        assert!(executor.validate(10u32, |v| *v > 5).is_ok());
        assert!(matches!(
            executor.validate(3u32, |v| *v > 5),
            Err(ExecutionError::InvalidResult)
        ));
    }
}
