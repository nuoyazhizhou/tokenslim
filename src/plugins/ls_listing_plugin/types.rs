//! ls_listing 插件类型定义。

/// 列式目录清单压缩插件（P3-206② 方案 A）。
///
/// 认领 `aws s3 ls [--recursive]` / `gsutil ls -l` / `ls -l` 一族的
/// 「`YYYY-MM-DD HH:MM:SS <右对齐尺寸> <路径>`」列式清单输出。
pub struct LsListingPlugin {
    pub(crate) name: &'static str,
    pub(crate) priority: u8,
}
