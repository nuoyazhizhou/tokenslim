//! dictionary_manager 测试模块（P1-03 / P3-05 回归）

#[cfg(test)]
mod tests {
    use crate::core::dictionary_manager::DictionaryManager;
    use std::sync::Arc;

    /// P1-03 回归：线程本地 ID 批发缓存跨实例污染——主线程先用 manager A
    /// 消耗 ID，再在同一线程用新建的 manager B 取 ID，B 的第一个编号必须
    /// 从 1 起（旧实现会发放 A 的余量段，两个不同路径拿到同一 `$Pn`）。
    #[test]
    fn new_manager_ids_start_from_one_on_same_thread() {
        let a = DictionaryManager::new();
        let t_a = a.get_or_add_path("/very/long/path/alpha/beta/gamma");
        assert!(t_a.starts_with("$P"), "A 应产出路径 token: {t_a}");

        let b = DictionaryManager::new();
        let t_b = b.get_or_add_path("/very/long/path/delta/epsilon/zeta");
        assert_eq!(
            t_b, "$P1",
            "新实例在同线程上的第一个 ID 必须从 1 开始（P1-03）"
        );
    }

    /// P1-03 回归：8 线程并发向同一 manager 登记路径，token ↔ path 必须
    /// 一一对应（无碰撞、无覆盖）。
    #[test]
    fn concurrent_path_registration_has_no_token_collision() {
        let manager = Arc::new(DictionaryManager::new());
        let handles: Vec<_> = (0..8)
            .map(|worker| {
                let m = manager.clone();
                std::thread::spawn(move || {
                    for i in 0..200 {
                        m.get_or_add_path(&format!("/data/worker{}/batch/job/item/{}", worker, i));
                    }
                })
            })
            .collect();
        for h in handles {
            h.join().unwrap();
        }

        assert_eq!(manager.path_dict.len(), 1600, "1600 条路径应全部登记");
        let mut seen_paths = std::collections::HashSet::new();
        for entry in manager.path_dict.iter() {
            assert!(
                seen_paths.insert(entry.value().0.clone()),
                "token 碰撞导致路径被覆盖（P1-03 核心缺陷）"
            );
        }
        assert_eq!(seen_paths.len(), 1600);
    }

    /// P3-05 回归：命令字典 ID 与 path/package/macro 统一走批发缓存，编号连续。
    #[test]
    fn compile_commands_use_sequential_ids() {
        let manager = DictionaryManager::new();
        manager.add_compile_commands(vec![
            "cargo build --release --example demo_app".to_string(),
            "cargo test --release --all-features here".to_string(),
        ]);
        let mut tokens: Vec<String> = manager
            .command_dict
            .iter()
            .map(|e| e.key().clone())
            .collect();
        tokens.sort();
        assert_eq!(
            tokens,
            vec!["$C1".to_string(), "$C2".to_string()],
            "命令 token 应连续编号（P3-05 统一批发）"
        );
    }

    /// P2-07 回归：8 线程并发登记同一组 100 个包名，`package_dict` 必须恰好
    /// 100 条（旧 check-then-act 实现下同一包名可能被两个线程各登记一次，
    /// 生成两个不同 `$PKn`——既浪费 ID 又让字典膨胀）。
    #[test]
    fn concurrent_package_registration_is_atomic() {
        let manager = Arc::new(DictionaryManager::new());
        let handles: Vec<_> = (0..8)
            .map(|_| {
                let m = manager.clone();
                std::thread::spawn(move || {
                    for i in 0..100 {
                        m.get_or_add_package(&format!("com.example.pkg.module{:04}", i));
                    }
                })
            })
            .collect();
        for h in handles {
            h.join().unwrap();
        }

        assert_eq!(manager.package_dict.len(), 100, "100 个包名应恰好登记 100 条");
    }

    /// P2-07 回归：8 线程并发登记同一组 100 条宏文本，`macro_dict` 必须恰好
    /// 100 条（与 package 同一 check-then-act 竞态面，entry() 化后原子）。
    #[test]
    fn concurrent_macro_registration_is_atomic() {
        let manager = Arc::new(DictionaryManager::new());
        let handles: Vec<_> = (0..8)
            .map(|_| {
                let m = manager.clone();
                std::thread::spawn(move || {
                    for i in 0..100 {
                        m.get_or_add_macro(&format!("some_macro_message_payload_{:04}", i));
                    }
                })
            })
            .collect();
        for h in handles {
            h.join().unwrap();
        }

        assert_eq!(manager.macro_dict.len(), 100, "100 条宏文本应恰好登记 100 条");
    }
}
