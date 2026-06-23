# scripts/translate_messages_fields.py
# 用 deep-translator 调 Google Translate，把 reference locale 的缺 key
# 翻译到目标 locale。仅翻译 i18n::t() 真正会渲染的纯文本字段；
# 跳过含 {placeholders} 的模板（保留原样，待人工调整）。
#
# 用法：
#   pip install deep-translator
#   python scripts/translate_messages_fields.py --dry-run          # 预览
#   python scripts/translate_messages_fields.py --apply            # 实际写入
#   python scripts/translate_messages_fields.py --apply --only-langs es,fr,ja
#
# 设计：
#   - reference 默认 zh-CN（项目权威源）
#   - 只补缺失 key（不覆盖人工翻译）
#   - 翻译失败时：保留 zh-CN 值并在 stderr 警告，绝不让 i18n::t() 拿到空串
#   - 自带限速：每条 sleep 0.2s，避开 Google 免费端点 QPS 限制
import argparse
import json
import re
import sys
import time
from pathlib import Path


def load(path: Path) -> dict:
    """以 utf-8-sig 读取 JSON 文件（兼容 BOM）。"""
    with path.open("r", encoding="utf-8-sig") as f:
        return json.load(f)


def save(path: Path, bundle: dict) -> None:
    """原子写：先写 .tmp 再 rename，避免中途崩溃丢原文件。"""
    tmp = path.with_suffix(path.suffix + ".tmp")
    with tmp.open("w", encoding="utf-8") as f:
        # 与项目历史 messages.*.json 一致：4 空格缩进、不排序、保留 Unicode
        json.dump(bundle, f, ensure_ascii=False, indent=4)
        f.write("\n")
    tmp.replace(path)


# 含 {placeholder} 的模板字符串 — 不翻译占位符，保留 zh-CN 原值
_TEMPLATE_RE = re.compile(r"\{[a-zA-Z0-9_]+\}")


def is_translatable(text: str) -> bool:
    """纯文本才翻译；含占位符的模板保留原样。"""
    return isinstance(text, str) and bool(text.strip()) and not _TEMPLATE_RE.search(text)


def make_translator():
    """惰性导入 deep-translator，缺包时给清晰报错。"""
    try:
        from deep_translator import GoogleTranslator  # type: ignore
    except ImportError as exc:  # pragma: no cover
        raise SystemExit(
            "缺少依赖 deep-translator。安装：\n"
            "    pip install deep-translator\n"
            f"原始错误：{exc}"
        )
    return GoogleTranslator


def to_deepl_code(lang: str) -> str:
    """把 zh-TW / pt-BR 这种 'locale' 映射到 Google Translator 的 'lang code'。"""
    mapping = {
        "zh-CN": "zh-CN",
        "zh-TW": "zh-TW",
        "en": "en",
        "ja": "ja",
        "ko": "ko",
        "fr": "fr",
        "de": "de",
        "es": "es",
        "ru": "ru",
        "ar": "ar",
        "pt": "pt",
    }
    return mapping.get(lang, lang.split("-")[0])


def main() -> int:
    ap = argparse.ArgumentParser(
        description="Translate missing i18n keys via Google Translate (deep-translator)."
    )
    ap.add_argument("--dir", default="resources",
                    help="messages.<lang>.json 所在目录（默认 resources）")
    ap.add_argument("--reference", default="zh-CN",
                    help="参考语言（默认 zh-CN）")
    ap.add_argument("--only-langs", default=None,
                    help="逗号分隔，只翻译指定 lang code")
    ap.add_argument("--apply", action="store_true",
                    help="实际写入；不加则 dry-run 仅打印")
    ap.add_argument("--sleep", type=float, default=0.2,
                    help="每条翻译后 sleep 秒数（防限流，默认 0.2）")
    ap.add_argument("--overwrite-placeholder", action="store_true",
                    help="当某 lang 的 value 与 reference 完全一致（即占位）时，"
                         "用真翻译覆盖；不影响真实人工翻译。")
    ap.add_argument("--keys", default=None,
                    help="逗号分隔，只翻译指定的 key 列表；与 "
                         "--overwrite-placeholder 配合可避免误伤。")
    args = ap.parse_args()

    base = Path(args.dir)
    if not base.is_dir():
        print(f"ERROR: dir not found: {base}", file=sys.stderr)
        return 1

    ref_path = base / f"messages.{args.reference}.json"
    if not ref_path.is_file():
        print(f"ERROR: reference file not found: {ref_path}", file=sys.stderr)
        return 1
    ref_bundle = load(ref_path)

    GoogleTranslator = make_translator()

    only = set(args.only_langs.split(",")) if args.only_langs else None
    # 提前过滤掉参考 lang 自身
    if only:
        only.discard(args.reference)
    total_filled = 0
    total_skipped = 0
    total_failed = 0
    files_touched: list[Path] = []

    for fp in sorted(base.glob("messages.*.json")):
        lang = fp.stem.removeprefix("messages.")
        if lang == args.reference:
            continue
        if only and lang not in only:
            continue

        bundle = load(fp)
        if args.overwrite_placeholder:
            # 仅当 value 与 ref_value 字符串完全一致时才视为占位
            missing_keys = [k for k, v in ref_bundle.items()
                            if k in bundle and bundle[k] == v]
        else:
            missing_keys = [k for k in ref_bundle if k not in bundle]
        if args.keys:
            key_filter = set(args.keys.split(","))
            missing_keys = [k for k in missing_keys if k in key_filter]
        if not missing_keys:
            print(f"[{lang}] up-to-date  ({len(bundle)} keys)")
            continue

        target = to_deepl_code(lang)
        # 翻译器实例化可能因未知 lang code 抛 LanguageNotSupportedException
        try:
            translator = GoogleTranslator(source="zh-CN", target=target)
        except Exception as exc:  # noqa: BLE001
            print(f"[{lang}] SKIP: Google Translate 不支持 lang code '{target}' "
                  f"({exc.__class__.__name__})，保留 zh-CN 占位", file=sys.stderr)
            # 降级：所有 missing key 填 ref 值，不丢
            for key in missing_keys:
                bundle[key] = ref_bundle[key]
                total_skipped += 1
            if args.apply:
                save(fp, bundle)
                files_touched.append(fp)
            continue

        print(f"[{lang}] {len(missing_keys)} missing key(s), target={target}")

        for key in missing_keys:
            ref_value = ref_bundle[key]
            # 非字符串 / 空串 / 含占位符的模板：保留 zh-CN 原值（不翻译）
            if not is_translatable(ref_value):
                bundle[key] = ref_value
                total_skipped += 1
                continue
            try:
                translated = translator.translate(ref_value)
                if not translated:
                    # 翻译器偶尔返回空串 — 降级用 ref 值，绝不让 i18n 拿到空串
                    translated = ref_value
                    total_failed += 1
                    print(f"  [warn] empty result for {key!r}, kept ref value",
                          file=sys.stderr)
                else:
                    # Google 免费端点偶尔会塞 NBSP (\xa0)，统一清成普通空格
                    translated = translated.replace("\xa0", " ")
                    bundle[key] = translated
                    total_filled += 1
                    print(f"  {key!r}: {ref_value!r}  ->  {translated!r}")
            except Exception as exc:  # noqa: BLE001
                # 联网失败 / 限流：保留 ref 值并继续
                bundle[key] = ref_value
                total_failed += 1
                print(f"  [warn] translate failed for {key!r}: {exc}; kept ref value",
                      file=sys.stderr)
            time.sleep(args.sleep)

        if args.apply:
            save(fp, bundle)
            files_touched.append(fp)
        else:
            print(f"  [dry-run] would update {fp}")

    print()
    print(f"summary: filled={total_filled} skipped={total_skipped} "
          f"failed_kept_ref={total_failed}")
    if args.apply:
        print(f"applied to {len(files_touched)} file(s):")
        for f in files_touched:
            print(f"  {f}")
    else:
        print("dry-run: no file written. Re-run with --apply to commit.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
