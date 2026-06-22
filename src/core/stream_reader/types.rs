//! stream reader 类型定义
//!
//! # 类型概述
//!
//! 本模块定义了 stream reader 模块所需的核心数据类型。
//! 这些类型包括结构体、枚举、 trait 等，用于表示该模块的数据结构和配置信息。

use std::borrow::Cow;
use std::fs::Permissions;
use std::path::PathBuf;
use std::time::SystemTime;

/// 字符编码类型
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CharsetEncoding {
    Utf8,
    Utf16Le,
    Utf16Be,
    Utf32Le,
    Utf32Be,
    Gb2312,
    Gbk,
    Big5,
    Latin1,
    ShiftJis,
    // 韩文编码
    EucKr, // EUC-KR (KS X 1001)
    Cp949, // CP949 (EUC-KR 超集，Windows 韩文)
    // 西里尔文编码
    Windows1251, // Windows-1251 (西里尔文)
    Koi8R,       // KOI8-R (俄语)
    Iso8859_5,   // ISO-8859-5 (西里尔文)
    // 阿拉伯语编码
    Windows1256, // Windows-1256 (阿拉伯语)
    Iso8859_6,   // ISO-8859-6 (阿拉伯语)
    // 希伯来语编码
    Windows1255, // Windows-1255 (希伯来语)
    Iso8859_8,   // ISO-8859-8 (希伯来语)
    // 其他西欧编码
    Windows1252, // Windows-1252 (西欧)
    Unknown,
}

impl std::fmt::Display for CharsetEncoding {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            CharsetEncoding::Utf8 => "UTF-8",
            CharsetEncoding::Utf16Le => "UTF-16 LE",
            CharsetEncoding::Utf16Be => "UTF-16 BE",
            CharsetEncoding::Utf32Le => "UTF-32 LE",
            CharsetEncoding::Utf32Be => "UTF-32 BE",
            CharsetEncoding::Gb2312 => "GB2312",
            CharsetEncoding::Gbk => "GBK",
            CharsetEncoding::Big5 => "Big5",
            CharsetEncoding::Latin1 => "Latin-1 (ISO-8859-1)",
            CharsetEncoding::ShiftJis => "Shift-JIS",
            // 韩文
            CharsetEncoding::EucKr => "EUC-KR",
            CharsetEncoding::Cp949 => "CP949 (EUC-KR 超集)",
            // 西里尔文
            CharsetEncoding::Windows1251 => "Windows-1251 (西里尔文)",
            CharsetEncoding::Koi8R => "KOI8-R (俄语)",
            CharsetEncoding::Iso8859_5 => "ISO-8859-5 (西里尔文)",
            // 阿拉伯语
            CharsetEncoding::Windows1256 => "Windows-1256 (阿拉伯语)",
            CharsetEncoding::Iso8859_6 => "ISO-8859-6 (阿拉伯语)",
            // 希伯来语
            CharsetEncoding::Windows1255 => "Windows-1255 (希伯来语)",
            CharsetEncoding::Iso8859_8 => "ISO-8859-8 (希伯来语)",
            // 其他西欧
            CharsetEncoding::Windows1252 => "Windows-1252 (西欧)",
            CharsetEncoding::Unknown => "Unknown",
        };
        write!(f, "{}", s)
    }
}

/// 文件类型（由 StreamReader 判断）
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileType {
    /// 纯文本文件
    Text,
    /// 二进制文件
    Binary,
    /// 符号链接 (Unix/Windows junction)
    Symlink,
    /// Windows 快捷方式 (.lnk)
    Shortcut,
    /// 未知 / 无法确定
    Unknown,
}

impl std::fmt::Display for FileType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            FileType::Text => "PlainText",
            FileType::Binary => "Binary",
            FileType::Symlink => "Symlink",
            FileType::Shortcut => "WindowsShortcut (.lnk)",
            FileType::Unknown => "Unknown",
        };
        write!(f, "{}", s)
    }
}

/// BOM 类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bom {
    Utf8,
    Utf16Le,
    Utf16Be,
    Utf32Le,
    Utf32Be,
}

impl std::fmt::Display for Bom {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Bom::Utf8 => "UTF-8 BOM",
            Bom::Utf16Le => "UTF-16 LE BOM",
            Bom::Utf16Be => "UTF-16 BE BOM",
            Bom::Utf32Le => "UTF-32 LE BOM",
            Bom::Utf32Be => "UTF-32 BE BOM",
        };
        write!(f, "{}", s)
    }
}

/// 文件元数据（丰富版本）
#[derive(Debug, Clone)]
pub struct FileMetadata {
    /// 文件路径
    pub path: Option<PathBuf>,
    /// 文件大小（字节）
    pub size: u64,
    /// 文件类型（文本/二进制/符号链接/快捷方式）
    pub file_type: FileType,
    /// 字符编码
    pub encoding: CharsetEncoding,
    /// BOM 标记
    pub bom: Option<Bom>,
    /// 文件所在的文件系统格式（如 NTFS, FAT32, ext4）
    pub fs_type: Option<String>,
    /// 生成该文件的操作系统（猜测，通过换行符等启发式判断）
    pub origin_os: Option<String>,
    /// 创建时间
    pub created: Option<SystemTime>,
    /// 修改时间
    pub modified: Option<SystemTime>,
    /// 访问时间
    pub accessed: Option<SystemTime>,
    /// 权限
    pub permissions: Option<Permissions>,
    /// 文件所有者（Unix: username, Windows: SID/account）
    pub owner: Option<String>,
}

/// StreamReader 错误类型
#[derive(Debug, thiserror::Error)]
pub enum StreamError {
    #[error("E_STREAM_IO:{0}")]
    Io(#[from] std::io::Error),
    #[error("E_STREAM_UTF8:{0}")]
    Utf8(#[from] std::str::Utf8Error),
    #[error("E_STREAM_BINARY_DETECTED")]
    BinaryDetected,
    #[error("E_STREAM_INVALID_PATH")]
    InvalidPath,
    #[error("E_STREAM_UNSUPPORTED_ENCODING")]
    UnsupportedEncoding,
    #[error("E_STREAM_LINE_TOO_LONG")]
    TooLongLine,
    #[error("E_STREAM_INVALID_ARGUMENT")]
    InvalidArgument,
}

/// 输入片段，由 StreamReader 产生，供 TextSlicer 消费
#[derive(Debug)]
pub struct SliceInput<'a> {
    pub raw: Cow<'a, str>,
    pub offset: usize,
    pub line_number: usize,
    pub file_metadata: Option<&'a FileMetadata>,
}

/// StreamReader 主结构
pub struct StreamReader<'a> {
    pub(crate) inner: Inner<'a>,
    pub(crate) metadata: Option<FileMetadata>,
}

pub enum Inner<'a> {
    Mmap(memmap2::Mmap),
    Buffer(Vec<u8>),
    Bytes(&'a [u8]),
}

/// 逐行迭代器
pub struct LineIterator<'a> {
    pub(crate) data: &'a [u8],
    pub(crate) current_offset: usize,
    pub(crate) current_line_number: usize,
    pub(crate) file_metadata: Option<&'a FileMetadata>,
}

impl<'a> Iterator for LineIterator<'a> {
    type Item = SliceInput<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.current_offset >= self.data.len() {
            return None;
        }

        let start = self.current_offset;
        let rest = &self.data[start..];

        // 使用 memchr 进行 SIMD 加速的换行符搜索
        let mut end;
        let mut next_offset;

        if let Some(pos) = memchr::memchr(b'\n', rest) {
            end = start + pos;
            next_offset = end + 1;

            // 处理 CRLF: 如果 \n 前面是 \r，则去掉 \r
            if pos > 0 && rest[pos - 1] == b'\r' {
                end -= 1;
            }
        } else if let Some(pos) = memchr::memchr(b'\r', rest) {
            // 处理可能的 CR-only 或剩余数据
            end = start + pos;

            // 检查后面是否还有 LF (虽然 memchr 没搜到，但为了严谨性)
            if pos + 1 < rest.len() && rest[pos + 1] == b'\n' {
                next_offset = end + 2;
            } else {
                // 判断是否为 CR-only 文件
                let has_lf_in_rest = rest[pos..].iter().any(|&b| b == b'\n');
                if !has_lf_in_rest {
                    next_offset = end + 1;
                } else {
                    // 这种情况理论上由于前面的 memchr(b'\n') 没搜到，不应该发生
                    // 但作为兜底，按普通字符处理继续搜
                    let mut found_end = pos + 1;
                    while found_end < rest.len()
                        && rest[found_end] != b'\n'
                        && rest[found_end] != b'\r'
                    {
                        found_end += 1;
                    }
                    end = start + found_end;
                    next_offset = end;
                    if found_end < rest.len() {
                        next_offset += 1;
                    }
                }
            }
        } else {
            // 最后一行
            end = self.data.len();
            next_offset = end;
        }

        let line_bytes = &self.data[start..end];
        let line_str = match std::str::from_utf8(line_bytes) {
            Ok(s) => Cow::Borrowed(s),
            Err(_) => String::from_utf8_lossy(line_bytes),
        };

        let slice = SliceInput {
            raw: line_str,
            offset: start,
            line_number: self.current_line_number,
            file_metadata: self.file_metadata,
        };

        self.current_offset = next_offset;
        self.current_line_number += 1;

        Some(slice)
    }
}

/// 按块迭代器
pub struct BlockIterator<'a> {
    pub(crate) data: &'a [u8],
    pub(crate) current_offset: usize,
    pub(crate) block_size: usize,
    pub(crate) file_metadata: Option<&'a FileMetadata>,
}

impl<'a> Iterator for BlockIterator<'a> {
    type Item = SliceInput<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.current_offset >= self.data.len() {
            return None;
        }

        let start = self.current_offset;
        let mut end = start + self.block_size;

        if end > self.data.len() {
            end = self.data.len();
        } else {
            // 确保不截断 UTF-8 多字节字符
            while end > start && !is_char_boundary(self.data, end) {
                end -= 1;
            }
            if end == start {
                end = start + self.block_size;
                while end < self.data.len() && !is_char_boundary(self.data, end) {
                    end += 1;
                }
            }
        }

        let block_bytes = &self.data[start..end];
        let block_str = match std::str::from_utf8(block_bytes) {
            Ok(s) => Cow::Borrowed(s),
            Err(_) => String::from_utf8_lossy(block_bytes),
        };

        let slice = SliceInput {
            raw: block_str,
            offset: start,
            line_number: 0,
            file_metadata: self.file_metadata,
        };

        self.current_offset = end;
        Some(slice)
    }
}

fn is_char_boundary(data: &[u8], index: usize) -> bool {
    if index == 0 || index == data.len() {
        return true;
    }
    let b = data[index];
    (b as i8) >= -0x40
}

/// 存储类型
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
pub enum StorageType {
    SSD,
    HDD,
    Unknown,
}

/// 系统信息
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct SystemInfo {
    pub available_memory: u64,     // 可用内存（字节）
    pub storage_type: StorageType, // 存储类型
}

/// StreamReader load config
#[derive(Debug, Clone)]
pub struct StreamReadConfig {
    /// mmap threshold in bytes; None means auto threshold.
    pub mmap_threshold: Option<usize>,
    /// Pre-allocated buffer size for eager read path.
    pub buffer_size: usize,
}

impl Default for StreamReadConfig {
    fn default() -> Self {
        Self {
            mmap_threshold: None,
            buffer_size: 8 * 1024,
        }
    }
}
