# -*- coding: utf-8 -*-
"""A-2 收紧效果验证：误判率（samples 抽样行）+ 真阳性保留（手工代表性样本）。"""
import json, glob, os, io, collections, random

random.seed(7)


def match(text, p):
    t = p.get("pattern_type"); pat = p.get("pattern") or ""
    if t == "contains":
        return pat in text
    if t == "starts_with":
        return text.strip().startswith(pat)
    if t == "paired_delimiters":
        return pat in text and (p.get("end_delimiter") is None or p.get("end_delimiter") in text)
    return False


def hit(text, cfg):
    for rule in cfg.get("rules", []):
        pats = rule.get("patterns", [])
        if rule.get("type") == "any":
            if any(match(text, p) for p in pats):
                return True
        elif rule.get("type") == "all":
            if pats and all(match(text, p) for p in pats):
                return True
    return False


cfgs = [(os.path.basename(f), json.load(io.open(f, encoding="utf-8")))
        for f in sorted(glob.glob("config/languages/*.json"))]
byname = dict(cfgs)

files = glob.glob("samples/**/*.log", recursive=True) + glob.glob("samples/**/*.txt", recursive=True)
random.shuffle(files)
files = files[:120]
lines = []
for f in files:
    try:
        txt = io.open(f, encoding="utf-8", errors="replace").read()
    except Exception:
        continue
    ls = [l.rstrip("\n") for l in txt.split("\n") if l.strip()]
    if ls:
        lines.extend(random.sample(ls, min(6, len(ls))))

cnt = collections.Counter()
tot = 0
for line in lines:
    for fn, cfg in cfgs:
        if hit(line, cfg):
            cnt[fn] += 1
    if any(hit(line, c) for _, c in cfgs):
        tot += 1
print("收紧后误判: %d/%d = %.1f%%  (改前 81.2%%)" % (tot, len(lines), 100.0 * tot / len(lines)))
for fn, c in cnt.most_common():
    print("  %-18s %6d (%.1f%%)" % (fn, c, 100.0 * c / len(lines)))

print()
tp = {
    "shell.json": ["  for f in *.txt; do", "fi", "    done", "if [ -f x ]; then",
                   "export PATH=/usr/bin:$PATH", "result=$(ls -la)", "#!/bin/bash", "  esac"],
    "python.json": ["from collections import defaultdict", "    for i in range(10):",
                    "with open(path) as f:", "def main():", "import os"],
    "rust.json": ["    let x = vec![1, 2, 3];", "    match err {", "impl Display for Foo {",
                  "fn main() {"],
    "go.json": ["type Config struct {", "func main() {", "package main"],
    "json-config.json": ['{"name": "test", "version": 1}', '  "key": "value",'],
    "yaml.json": ["- name: build", "  - item", "---", "!!python/object", "%YAML 1.2"],
    "toml.json": ['key = "value"', "[section]", "flag = true"],
    "xml-config.json": ["<dependency>", "  <groupId>org.x</groupId>", '<a b="c">', "<!-- comment -->"],
    "ini.json": ["[section]", "key=value", "; comment"],
    "properties.json": ["db.url=jdbc:mysql://localhost"],
    "makefile.json": ["all: build", "\t$(MAKE) install", "else", "endif", ".PHONY: clean"],
    "nginx.json": ["  root /var/www;", "server {", "  proxy_pass http://up;"],
    "java.json": ["public class Main {", "System.out.println(x);"],
    "csharp.json": ["using System;", "public class Foo {", "namespace App {"],
    "javascript.json": ["const x = require('fs');", "function foo() {", "let y = 1;"],
    "groovy.json": ["pipeline {", "  stages {", 'error "failed"', "def x = 1"],
    "scala.json": ["case class Foo(x: Int)", "object Main {", "val x = 1"],
    "kotlin.json": ["fun main() {", "data class User(val name: String)"],
}
print("真阳性保留（MISS=收紧后漏检，需评估）:")
bad = 0
for fn, samples in tp.items():
    cfg = byname[fn]
    for s in samples:
        ok = hit(s, cfg)
        if not ok:
            bad += 1
        print("  %-18s %s %r" % (fn, "OK  " if ok else "MISS", s[:40]))
print()
print("真阳性 MISS 总数: %d" % bad)
