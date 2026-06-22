# encoding_fallback samples

- `case_001_mojibake_chain.log`: classic UTF-8 double-decoding mojibake text.
- `case_002_utf16le_no_bom.hex`: UTF-16LE bytes without BOM.
- `case_003_utf16be_no_bom.hex`: UTF-16BE bytes without BOM.
- `case_004_inline_bom_nul_cr.hex`: text bytes containing inline BOM, NUL, and CR.
- `case_005_gbk_zh.hex`: GBK bytes for `中文`.
- `case_006_utf32le_no_bom.hex`: UTF-32LE bytes without BOM.
- `case_007_utf32be_no_bom.hex`: UTF-32BE bytes without BOM.
- `case_008_binary_like.hex`: binary-like payload used to validate binary guard.
- `case_009_big5_zh_tw.hex`: Big5 bytes for `繁體中文測試`.
- `case_010_cp949_ko.hex`: CP949(EUC-KR path) bytes for `한국어테스트데이터`.
