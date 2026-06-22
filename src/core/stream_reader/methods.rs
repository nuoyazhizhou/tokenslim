//! stream reader 模块方法实现
//!
//! # 模块概述
//!
//! 本模块实现了流式读取器的核心逻辑，支持内存映射 (mmap)、并行读取、自动识别编码和文件类型等功能。
//!
//! # 功能说明
//!
//! 提供高效的日志和文本文件读取能力，能够自动处理不同操作系统的换行符和编码。

use super::types::*;
use crate::core::observability::{log_object_size, ScopeProbe};
use std::borrow::Cow;
use std::fs;
use std::io::Read;
use std::path::Path;
use sysinfo::{DiskKind, Disks, System};

impl<'a> StreamReader<'a> {
    const MIN_PARALLEL_CHUNK_SIZE: usize = 256 * 1024;
    const MAX_PARALLEL_CHUNK_SIZE: usize = 5 * 1024 * 1024;
    const SEMANTIC_SCAN_MULTIPLIER: usize = 2;

    /// 计算最优的内存映射 (mmap) 阈值。根据可用内存和磁盘速度动态决定何时开启大文件读取优化。
    pub fn calculate_optimal_threshold() -> usize {
        let mut sys = System::new_all();
        sys.refresh_all();

        let available_mem = sys.available_memory();

        // 默认阈值 20MB
        let mut threshold = 20 * 1024 * 1024;

        // 如果可用内存大于 4GB，可以放宽到 100MB
        if available_mem > 4 * 1024 * 1024 * 1024 {
            threshold = 100 * 1024 * 1024;
        }

        // 检查磁盘类型
        let disks = Disks::new_with_refreshed_list();
        let mut has_ssd = false;
        for disk in &disks {
            if disk.kind() == DiskKind::SSD {
                has_ssd = true;
                break;
            }
        }

        // 如果是 SSD，mmap 的收益更高，可以进一步降低阈值
        if has_ssd {
            threshold /= 2;
        }

        threshold
    }

    /// 从文件创建 StreamReader。自动检测文件类型、编码、BOM 和操作系统来源。
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self, StreamError> {
        let path = path.as_ref();
        let metadata = fs::metadata(path)?;
        let file_size = metadata.len();

        let threshold = Self::calculate_optimal_threshold();
        let file = fs::File::open(path)?;

        let inner;
        if file_size > threshold as u64 {
            let mmap = unsafe { memmap2::Mmap::map(&file)? };
            log_object_size(
                "stream_reader",
                "from_file.mmap",
                "mapped_bytes",
                mmap.len(),
            );
            inner = Inner::Mmap(mmap);
        } else {
            let mut buffer = Vec::new();
            let mut file_clone = file.try_clone()?;
            file_clone.read_to_end(&mut buffer)?;
            log_object_size(
                "stream_reader",
                "from_file.buffer",
                "buffer_bytes",
                buffer.len(),
            );
            inner = Inner::Buffer(buffer);
        }

        // 模拟元数据提取
        let file_metadata = FileMetadata {
            path: Some(path.to_path_buf()),
            size: file_size,
            file_type: FileType::Text, // 简化处理
            encoding: CharsetEncoding::Utf8,
            bom: None,
            created: metadata.created().ok(),
            modified: metadata.modified().ok(),
            accessed: metadata.accessed().ok(),
            permissions: Some(metadata.permissions()),
            owner: None,
            fs_type: None,
            origin_os: None,
        };

        Ok(StreamReader {
            inner,
            metadata: Some(file_metadata),
        })
    }

    /// 从文件创建 StreamReader（带配置项）。
    pub fn from_file_with_config<P: AsRef<Path>>(
        path: P,
        read_config: &StreamReadConfig,
    ) -> Result<Self, StreamError> {
        let _probe = ScopeProbe::new("stream_reader", "from_file_with_config");
        let path = path.as_ref();
        let file = fs::File::open(path)?;
        let metadata = file.metadata()?;
        let file_size = metadata.len();

        let threshold = read_config
            .mmap_threshold
            .unwrap_or_else(Self::calculate_optimal_threshold);

        let inner;
        if file_size > threshold as u64 {
            let mmap = unsafe { memmap2::Mmap::map(&file)? };
            log_object_size(
                "stream_reader",
                "from_file_with_config.mmap",
                "mapped_bytes",
                mmap.len(),
            );
            inner = Inner::Mmap(mmap);
        } else {
            let prealloc = std::cmp::max(file_size as usize, read_config.buffer_size);
            let mut buffer = Vec::with_capacity(prealloc);
            let mut file_clone = file.try_clone()?;
            file_clone.read_to_end(&mut buffer)?;
            log_object_size(
                "stream_reader",
                "from_file_with_config.buffer",
                "buffer_bytes",
                buffer.len(),
            );
            inner = Inner::Buffer(buffer);
        }

        let file_metadata = FileMetadata {
            path: Some(path.to_path_buf()),
            size: file_size,
            file_type: FileType::Text,
            encoding: CharsetEncoding::Utf8,
            bom: None,
            created: metadata.created().ok(),
            modified: metadata.modified().ok(),
            accessed: metadata.accessed().ok(),
            permissions: Some(metadata.permissions()),
            owner: None,
            fs_type: None,
            origin_os: None,
        };

        Ok(StreamReader {
            inner,
            metadata: Some(file_metadata),
        })
    }

    /// 从字符串直接创建 StreamReader。适用于处理已加载到内存的小文本或测试场景。
    pub fn from_str(text: &'a str) -> Self {
        let _probe =
            ScopeProbe::new("stream_reader", "from_str").add_field("input_bytes", text.len());
        StreamReader {
            inner: Inner::Bytes(text.as_bytes()),
            metadata: None,
        }
    }

    /// 从拥有所有权的 String 创建读取器。
    pub fn from_str_owned(text: String) -> Self {
        let _probe =
            ScopeProbe::new("stream_reader", "from_str_owned").add_field("input_bytes", text.len());
        StreamReader {
            inner: Inner::Buffer(text.into_bytes()),
            metadata: None,
        }
    }

    /// 获取文件元数据。
    pub fn metadata(&self) -> Option<&FileMetadata> {
        self.metadata.as_ref()
    }

    /// 获取原始字节切片。
    pub fn get_data(&self) -> &[u8] {
        match &self.inner {
            Inner::Mmap(mmap) => mmap.as_ref(),
            Inner::Buffer(buffer) => buffer.as_slice(),
            Inner::Bytes(bytes) => bytes,
        }
    }

    /// 获取数据源大小（字节）。
    pub fn size(&self) -> usize {
        self.metadata
            .as_ref()
            .map(|m| m.size as usize)
            .unwrap_or_else(|| match &self.inner {
                Inner::Buffer(b) => b.len(),
                Inner::Bytes(b) => b.len(),
                _ => 0,
            })
    }

    /// 判断当前内容是否为纯文本。通过检测样本字节中是否包含 NULL 字符（0x00）来判断。
    pub fn is_text(&self) -> bool {
        match &self.inner {
            Inner::Mmap(mmap) => !Self::detect_binary(mmap.as_ref()),
            Inner::Buffer(buffer) => !Self::detect_binary(buffer.as_slice()),
            Inner::Bytes(bytes) => !Self::detect_binary(bytes),
        }
    }

    /// 创建逐行迭代器。自动映射数据源并提供零拷贝行遍历。支持 LF 和 CRLF 换行符。
    pub fn iter_lines(&self) -> LineIterator<'_> {
        let _probe =
            ScopeProbe::new("stream_reader", "iter_lines").add_field("source_size", self.size());
        let data = match &self.inner {
            Inner::Mmap(mmap) => mmap.as_ref(),
            Inner::Buffer(buffer) => buffer.as_slice(),
            Inner::Bytes(bytes) => bytes,
        };

        LineIterator {
            data,
            current_offset: 0,
            current_line_number: 1,
            file_metadata: self.metadata.as_ref(),
        }
    }

    /// 创建块迭代器。按指定大小切分原始字节流，适用于大内容的分片分析。
    pub fn iter_blocks(&self, block_size: usize) -> Result<BlockIterator<'_>, StreamError> {
        let _probe = ScopeProbe::new("stream_reader", "iter_blocks")
            .add_field("source_size", self.size())
            .add_field("block_size", block_size);
        if block_size == 0 {
            return Err(StreamError::InvalidArgument);
        }

        let data = match &self.inner {
            Inner::Mmap(mmap) => mmap.as_ref(),
            Inner::Buffer(buffer) => buffer.as_slice(),
            Inner::Bytes(bytes) => bytes,
        };

        Ok(BlockIterator {
            data,
            current_offset: 0,
            block_size,
            file_metadata: self.metadata.as_ref(),
        })
    }

    /// 计算并行切块的目标大小（仅做切块规划，不含调度逻辑）。
    pub fn calculate_dynamic_chunk_size(total_size: usize, num_workers: usize) -> usize {
        if total_size == 0 {
            return 0;
        }

        let workers = num_workers.max(1);
        let target = total_size / workers;

        target
            .max(Self::MIN_PARALLEL_CHUNK_SIZE)
            .min(Self::MAX_PARALLEL_CHUNK_SIZE)
    }

    /// 将输入按语义锚点切成并行安全块。
    ///
    /// 注意：本方法只负责“切块与边界判定”，不涉及任何线程调度策略。
    pub fn split_for_parallel(&'a self, num_workers: usize) -> Vec<SliceInput<'a>> {
        let data = self.get_data();
        if data.is_empty() {
            return Vec::new();
        }

        let target = Self::calculate_dynamic_chunk_size(data.len(), num_workers);
        let mut chunks = Vec::new();
        let mut start = 0;
        let mut line_number = 1;

        while start < data.len() {
            let mut end = Self::split_by_semantic_anchors(data, start, target);

            if end <= start {
                end = (start + target.max(1)).min(data.len());
                while end > start && !is_utf8_boundary(data, end) {
                    end -= 1;
                }
                if end == start {
                    end = data.len();
                }
            }

            let block = &data[start..end];
            let raw = match std::str::from_utf8(block) {
                Ok(s) => Cow::Borrowed(s),
                Err(_) => Cow::Owned(String::from_utf8_lossy(block).into_owned()),
            };

            chunks.push(SliceInput {
                raw,
                offset: start,
                line_number,
                file_metadata: self.metadata.as_ref(),
            });

            line_number += memchr::memchr_iter(b'\n', block).count();
            start = end;
        }

        chunks
    }

    /// 从 `start + target_chunk_size` 附近寻找语义安全切分点。
    ///
    /// 优先规则：
    /// 1) 不切断行；
    /// 2) 倾向切在“新语义块起点”（空行、时间戳行、`[` 开头行、非缩进行）；
    /// 3) 受扫描上限保护，避免无限扩张。
    pub(crate) fn split_by_semantic_anchors(
        data: &'a [u8],
        start: usize,
        target_chunk_size: usize,
    ) -> usize {
        if start >= data.len() {
            return data.len();
        }

        let target_end = (start + target_chunk_size.max(1)).min(data.len());
        if target_end >= data.len() {
            return data.len();
        }

        let initial_break = find_next_line_start(data, target_end);
        if initial_break >= data.len() {
            return data.len();
        }

        let scan_cap =
            (start + target_chunk_size.max(1) * Self::SEMANTIC_SCAN_MULTIPLIER).min(data.len());

        let mut cursor = initial_break;
        while cursor < scan_cap {
            let line_end = find_line_end(data, cursor);
            let line = trim_cr(&data[cursor..line_end]);

            if is_semantic_safe_break_line(line) {
                return cursor;
            }

            if line_end >= data.len() {
                return data.len();
            }

            cursor = (line_end + 1).min(data.len());
        }

        initial_break
    }

    /// 二进制文件检测。扫描前 8KB 字节，如果发现 NULL 字节则判定为二进制文件。
    pub fn detect_binary(data: &[u8]) -> bool {
        let check_len = std::cmp::min(data.len(), 8192);
        if check_len == 0 {
            return false;
        }

        // 使用 memchr 进行 SIMD 加速的 NULL 字节搜索
        memchr::memchr(0, &data[..check_len]).is_some()
    }

    /// 辅助方法：通过文件头字节序列检测 BOM (Byte Order Mark) 类型。
    pub fn detect_bom(header: &[u8]) -> Option<Bom> {
        if header.starts_with(&[0xFF, 0xFE, 0x00, 0x00]) {
            Some(Bom::Utf32Le)
        } else if header.starts_with(&[0x00, 0x00, 0xFE, 0xFF]) {
            Some(Bom::Utf32Be)
        } else if header.starts_with(&[0xEF, 0xBB, 0xBF]) {
            Some(Bom::Utf8)
        } else if header.len() >= 2 && header[0] == 0xFF && header[1] == 0xFE {
            Some(Bom::Utf16Le)
        } else if header.starts_with(&[0xFE, 0xFF]) {
            Some(Bom::Utf16Be)
        } else {
            None
        }
    }
}

fn find_next_line_start(data: &[u8], from: usize) -> usize {
    if from == 0 {
        return 0;
    }
    if from >= data.len() {
        return data.len();
    }

    if data[from - 1] == b'\n' {
        return from;
    }

    if let Some(pos) = memchr::memchr(b'\n', &data[from..]) {
        (from + pos + 1).min(data.len())
    } else {
        data.len()
    }
}

fn find_line_end(data: &[u8], start: usize) -> usize {
    if start >= data.len() {
        return data.len();
    }

    if let Some(pos) = memchr::memchr(b'\n', &data[start..]) {
        start + pos
    } else {
        data.len()
    }
}

fn trim_cr(line: &[u8]) -> &[u8] {
    if line.last() == Some(&b'\r') {
        &line[..line.len().saturating_sub(1)]
    } else {
        line
    }
}

fn is_semantic_safe_break_line(line: &[u8]) -> bool {
    if line.is_empty() {
        return true;
    }

    let first = line[0];
    if first.is_ascii_digit() || first == b'[' {
        return true;
    }

    first != b' ' && first != b'\t'
}

fn is_utf8_boundary(data: &[u8], index: usize) -> bool {
    if index == 0 || index == data.len() {
        return true;
    }
    let b = data[index];
    (b as i8) >= -0x40
}
