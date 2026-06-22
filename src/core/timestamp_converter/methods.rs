//! 时间戳转换器方法实现

use super::types::TimestampConverter;

impl TimestampConverter {
    /// 批量转换多行文本中的时间戳。
    ///
    /// 将每一行中的绝对时间戳替换为相对于基准时间的相对时间偏移。
    pub fn convert_lines(&mut self, lines: &[String]) -> Vec<String> {
        lines
            .iter()
            .map(|line| {
                self.convert_line(std::borrow::Cow::Borrowed(line))
                    .into_owned()
            })
            .collect()
    }

    /// 转换整个文本块中的时间戳。
    ///
    /// 会将文本按行分割处理，然后重新合并。
    pub fn convert_text(&mut self, text: &str) -> String {
        text.lines()
            .map(|line| self.convert_line(std::borrow::Cow::Borrowed(line)))
            .collect::<Vec<_>>()
            .join("\n")
    }
}
