#!/usr/bin/env python3
"""
TokenSlim Output Analyzer
分析压缩输出与原始文本的对比，找出优化空间
"""

import json
import os
import sys
from pathlib import Path
from collections import defaultdict
import re

class TokenAnalyzer:
    def __init__(self, data_dir, output_dir):
        self.data_dir = Path(data_dir)
        self.output_dir = Path(output_dir)
        self.results = []
        
    def load_json(self, filepath):
        with open(filepath, 'r', encoding='utf-8') as f:
            return json.load(f)
    
    def extract_text_from_tokens(self, tokens):
        """从 tokens 中提取所有文本"""
        texts = []
        for token in tokens:
            if 'Text' in token:
                texts.append(token['Text'])
            elif 'Path' in token:
                texts.append(token['Path'])
        return texts
    
    def find_long_tokens(self, tokens, min_length=50):
        """找出比较长的 token"""
        long_tokens = []
        for token in tokens:
            if 'Text' in token:
                text = token['Text']
                if len(text) >= min_length:
                    long_tokens.append({
                        'text': text,
                        'length': len(text),
                        'type': 'Text'
                    })
        return sorted(long_tokens, key=lambda x: x['length'], reverse=True)
    
    def analyze_compression_ratio(self, original_file, compressed_file):
        """分析压缩比"""
        # 读取原始文件
        with open(self.data_dir / original_file, 'r', encoding='utf-8') as f:
            original_text = f.read()
        
        # 读取压缩后的 JSON
        compressed_data = self.load_json(self.output_dir / compressed_file)
        tokens = compressed_data.get('tokens', [])
        
        # 计算原始大小
        original_size = len(original_text)
        
        # 计算压缩后大小
        compressed_size = sum(
            len(t.get('Text', '')) + len(t.get('Path', '')) 
            for t in tokens 
            if isinstance(t, dict)
        )
        
        # 统计字典
        dictionary = compressed_data.get('dictionary', {})
        dict_size = sum(len(k) + len(v) for k, v in dictionary.items()) if dictionary else 0
        
        # 计算 token 数量
        token_count = len(tokens)
        
        # 查找字典 token
        dict_tokens = []
        for token in tokens:
            if isinstance(token, dict):
                for key, value in token.items():
                    if key.startswith('$'):
                        dict_tokens.append(key)
        
        return {
            'original_size': original_size,
            'compressed_size': compressed_size,
            'dict_size': dict_size,
            'token_count': token_count,
            'compression_ratio': compressed_size / original_size if original_size > 0 else 1.0,
            'dict_tokens_used': len(dict_tokens),
            'unique_dict_tokens': len(set(dict_tokens))
        }
    
    def analyze_file(self, original_name, json_name):
        """分析单个文件"""
        print(f"\n{'='*60}")
        print(f"分析: {original_name}")
        print(f"{'='*60}")
        
        # 1. 基本压缩比分析
        ratio_info = self.analyze_compression_ratio(original_name, json_name)
        
        print(f"\n📊 压缩统计:")
        print(f"  原始大小: {ratio_info['original_size']:,} bytes")
        print(f"  压缩后大小: {ratio_info['compressed_size']:,} bytes")
        print(f"  Token 数量: {ratio_info['token_count']}")
        print(f"  压缩比: {ratio_info['compression_ratio']:.2%}")
        print(f"  字典 token 使用次数: {ratio_info['dict_tokens_used']}")
        print(f"  唯一字典 token: {ratio_info['unique_dict_tokens']}")
        
        # 2. 加载压缩后的数据
        compressed_data = self.load_json(self.output_dir / json_name)
        tokens = compressed_data.get('tokens', [])
        
        # 3. 找出最长的 token
        long_tokens = self.find_long_tokens(tokens, min_length=80)
        
        if long_tokens:
            print(f"\n🔍 最长的 {min(10, len(long_tokens))} 个 Token:")
            for i, token in enumerate(long_tokens[:10], 1):
                print(f"  {i}. [{token['length']} chars] {token['text'][:100]}...")
        
        # 4. 检查是否使用了字典 token
        has_dict = ratio_info['unique_dict_tokens'] > 0
        if not has_dict:
            print(f"\n⚠️ 警告: 没有使用任何字典 token！")
            print(f"   建议: 检查 DictionaryEngine 是否正确启用")
        
        return {
            'file': original_name,
            **ratio_info,
            'long_tokens': long_tokens[:5]
        }
    
    def generate_report(self):
        """生成完整报告"""
        # 匹配测试数据文件和输出 JSON 文件
        test_files = [
            ('android_build_failure.txt', 'android_build_failure.json'),
            ('android_build_success.txt', 'android_build_success.json'),
            ('gcc_build_failure-1.txt', 'gcc_build_failure-1.json'),
            ('gcc_build_failure-2.txt', 'gcc_build_failure-2.json'),
            ('gcc_build_failure-3.txt', 'gcc_build_failure-3.json'),
            ('gcc_build_failure-4.txt', 'gcc_build_failure-4.json'),
            ('gcc_build_success.txt', 'gcc_build_success.txt.json'),
            ('gcc_build_utf8.txt', 'gcc_build_utf8.json'),
            ('gcc_coverity_success-1.txt', 'gcc_coverity_success-1.json'),
            ('gcc_coverity_success-2.txt', 'gcc_coverity_success-2.json'),
            ('ios_build_failure.txt', 'ios_build_failure.json'),
            ('ios_build_success.txt', 'ios_build_success.json'),
            ('jenkins_build_failure.txt', 'jenkins_build_failure.json'),
            ('maven_java_build_failure.txt', 'maven_java_build_failure.json'),
            ('maven_java_build_success.txt', 'maven_java_build_success.json'),
            ('nodejs_build_failure.txt', 'nodejs_build_failure.json'),
        ]
        
        all_results = []
        
        for original, json_file in test_files:
            json_path = self.output_dir / json_file
            if json_path.exists():
                result = self.analyze_file(original, json_file)
                all_results.append(result)
            else:
                print(f"\n⚠️ 文件不存在: {json_file}")
        
        # 生成汇总
        print(f"\n{'='*60}")
        print("📈 总体统计")
        print(f"{'='*60}")
        
        total_original = sum(r['original_size'] for r in all_results)
        total_compressed = sum(r['compressed_size'] for r in all_results)
        total_tokens = sum(r['token_count'] for r in all_results)
        
        print(f"总原始大小: {total_original:,} bytes")
        print(f"总压缩后大小: {total_compressed:,} bytes")
        print(f"总体压缩比: {total_compressed/total_original:.2%}" if total_original > 0 else "N/A")
        print(f"总 Token 数量: {total_tokens}")
        
        # 找出压缩效果最差的文件
        worst = max(all_results, key=lambda x: x['compression_ratio'])
        print(f"\n📉 压缩效果最差: {worst['file']} ({worst['compression_ratio']:.2%})")
        
        # 找出使用了最多字典 token 的文件
        best_dict = max(all_results, key=lambda x: x['dict_tokens_used'])
        print(f"📚 字典使用最多: {best_dict['file']} ({best_dict['dict_tokens_used']} 次)")
        
        return all_results


def main():
    analyzer = TokenAnalyzer(
        data_dir='tests/data',
        output_dir='tests/output'
    )
    
    results = analyzer.generate_report()
    
    # 保存详细结果到文件
    output_file = 'tests/output/analysis_report.json'
    with open(output_file, 'w', encoding='utf-8') as f:
        json.dump(results, f, ensure_ascii=False, indent=2)
    
    print(f"\n✅ 详细报告已保存到: {output_file}")


if __name__ == '__main__':
    main()
