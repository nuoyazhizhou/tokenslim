<p align="center">
  <h1 align="center">TokenSlim</h1>
  <p align="center">
    Hochperformante Rust-Token-Komprimierungs-Engine für LLM-Eingaben.<br>
    Plugin-basiert · 50%–95% Token-Einsparung · KI-Export-Diagnose · CLI / Server / IDE / SDK
  </p>
</p>

<p align="center">
  <a href="#was-ist-tokenslim">Was ist TokenSlim?</a> ·
  <a href="#warum-tokenslim">Warum?</a> ·
  <a href="#funktionen">Funktionen</a> ·
  <a href="#installation">Installation</a> ·
  <a href="#verwendung">Verwendung</a> ·
  <a href="#plugins">Plugins</a> ·
  <a href="#integrationen">Integrationen</a> ·
  <a href="#lizenz">Lizenz</a>
</p>

<p align="center">
  <a href="./README.md">English</a> · <a href="./README.zh-CN.md">简体中文</a> · <a href="./README.ja.md">日本語</a> · <a href="./README.ko.md">한국어</a> · <a href="./README.es.md">Español</a> · <a href="./README.fr.md">Français</a> · <strong>Deutsch</strong> · <a href="./README.ar.md">العربية</a>
</p>

---

## Was ist TokenSlim?

TokenSlim ist eine hochperformante, plugin-basierte Text-Komprimierungs-Engine, geschrieben in Rust. Ihre zentrale Mission ist es, **die Token-Kosten von LLM-Eingaben drastisch zu senken** und es zu ermöglichen, lange, verrauschte reale Logs (Build-Pipelines, CI-Läufe, Web-Access-Logs, Datenbank-Traces, Cloud-Logs, VCS-Ausgaben, Stack-Traces usw.) in das Kontextfenster eines LLM zu bekommen — ohne die diagnostischen Signale zu verlieren, die das Modell benötigt.

Bei hochgradig strukturierten, sich wiederholenden Eingaben (Compiler-Logs, Build-Ausgabe, CI-Logs, Access-Logs usw.) liefert TokenSlim typischerweise eine Reduktion von **50%–90%** unter Beibehaltung von 100% der Originalinformation. Im **AI Export**-Modus, der speziell für den LLM-Konsum entwickelt wurde, erreicht die Reduktion **90%–95%** mit kontextbezogener Entrauschung, die das Fehler-/Warnungs-Fenster erhält, das das Modell zum Schlussfolgern benötigt.

Über die Komprimierung hinaus liefert TokenSlim Umgebungsdiagnose-Werkzeuge (`workspace`-, `encoding`-, `rule`-, `env`-Befehle), die automatisch OS, Shell, Codepage, Python/Node/JDK-Encoding-Konfiguration erkennen, Mojibake-Risiko flaggen und umsetzbare Korrekturen ausgeben. Kombiniert mit einer Subprozess-Decoding-Fallback-Kette (zuerst UTF-8, dann Codepage-Kandidaten) bleibt es in mehrsprachigen Umgebungen zuverlässig.

## Warum TokenSlim?

### 1. Echtes Geld gespart
Die LLM-API-Kosten werden von der Eingabe-Token-Anzahl dominiert. TokenSlim schneidet sie um 50%–95%:

- **Niedrigere API-Rechnungen** — 50%–95% weniger Eingabe-Tokens.
- **Kontextbewusster AI Export (`--ai-export`)** — entfernt Routine-Zeilen, behält das Fehler-/Warnungs-Fenster, das das Modell tatsächlich benötigt; reduziert Halluzinationen bei verrauschten Eingaben.
- **Längerer effektiver Kontext** — gleiches Kontextfenster, mehr echtes Signal.
- **Schnellerer Prefill** — kürzere Eingaben bedeuten normalerweise schnelleren Modell-Prefill und niedrigeren TTFT.

### 2. Performance in Industriequalität
- **Zero-Copy-Pipeline** — gebaut auf Rust `Cow<'a, str>`, parallele Block-Verarbeitung mit `rayon` und `Bump`-Arena-Allokation. Verarbeitet 100 MB Industriellogs in **~250 ms**, ca. 400 MB/s Durchsatz.
- **Deterministische globale Neuordnung** — ein Streaming-Build-Target-Tracker korrigiert die unsortierte Verschachtelung, die `make -jN` / `Ninja` erzeugt. Zwei identische parallele Builds erzeugen immer dieselbe Fehler-Stack-Reihenfolge.
- **Sidecar-Modus** — hochdurchsatzstarker REST-API-Server, einbettbar in IDE / CI / Agent-Workflows ohne Start-Overhead.

### 3. Datengetriebene Extraktion
- **Radix-Trie-Pfadextraktion** — TokenSlim schneidet nicht zeilenweise. Nach dem Scannen von 100 MB Eingabe baut es einen projektweiten Radix-Trie im Speicher und gibt Wörterbuchverzeichnisse (`$D`) nur auf heißen Branches (Gewicht > 10) aus, wodurch fragmentarische Tokens eliminiert werden.
- **Semantische Marker** — umgebungsbewusste Substitutionen für Android, iOS, GCC, MSVC und Linker.
- **Erkennung des gesamten Build-Ökosystems** — C/C++, Rust, Go, Java, Android, iOS/Xcode, MSVC, Swift und wichtige Linker, mit kontextbewusster Faltung und Fehler-Deduplizierung.

## Funktionen

- **Drei Runtimes**
  - **CLI** — skriptfähige Stapelverarbeitung
  - **Server** — langlebige REST-API für vollständige Ökosystem-Integration
  - **SDKs** — Java, Python (PyO3), Node.js
- **Plugin-Ökosystem** (60+ Plugins, die die gängigsten LLM-Eingabequellen abdecken)
  - **Mobile** — `android_gradle`, `xcode_log`
  - **Allgemeine Entwicklung** — `gcc_log`, `java_stack`, `python_traceback`, `dotnet`, `rust_go`, `maven`, `gradle`, `node_error`, `nodejs`, `php_ruby`, `unity_unreal`
  - **Strukturierte Daten** — `json`, `yaml`, `xml_html`, `ndjson`, `protobuf`
  - **Build-Artefakte** — `artifact_summary` (SARIF / JUnit XML), mit semantischer Erhaltung von Test-Status, SARIF level/rule/location/tool
  - **Cloud & Ops** — `cloud_log` (AWS / GCP / Azure / Alibaba / OCI / Tencent / Huawei / Cloudflare), `web_log` (Nginx / Apache / ingress / Envoy / CloudFront / IIS / ALB / Cloudflare), `db_log` (PostgreSQL / MySQL / MongoDB / Redis), `syslog`
  - **CI/CD** — `ci_log` (GitHub Actions / GitLab CI / Jenkins / Azure Pipelines / CircleCI / Buildkite / lokales `act` / TeamCity / Travis CI)
  - **VCS** — einheitlicher `vcs_plugin` für git / svn / hg / p4 / cvs / bzr / fossil / darcs, plus `git_diff`, `smart_code` (AST-Ebene), `smart_path`
- **Umgebungsdiagnose** — die Unterbefehle `workspace`, `encoding`, `rule`, `env` erkennen Mojibake-Risiko und geben Korrekturrezepte aus.
- **KI-native Ausgabemodi**
  - `--ai-export` — kontextbewusste Entrauschung, behält das Fehler-/Warnungs-Fenster
  - `--ai-signal` — verlustbehaftet aber hochsignalent, behält die entscheidungsrelevantesten Felder
- **Plugin-Introspektion** — `tokenslim explain-plugin` und `tokenslim run --explain-route` erklären Routenauswahl, Fallbacks, Konfidenz, Alternativen und spielen Fehlklassifikationen für Audits ab.

## Installation

### Aus Quellcode (Rust-Toolchain ≥ 1.75)

```bash
git clone https://github.com/nuoyazhizhou/tokenslim.git
cd tokenslim
cargo build --release
```

Das Binary landet unter `./target/release/tokenslim` (bzw. `tokenslim.exe` unter Windows).

### Vorgefertigte Binaries

Download von der [Releases](https://github.com/nuoyazhizhou/tokenslim/releases)-Seite.

### Konfiguration (optional)

Die gesamte Runtime-Konfiguration erfolgt über Umgebungsvariablen. Kopiere [`.env.example`](./.env.example) nach `.env` und fülle deine lokalen Werte ein. `.env` wird standardmäßig von git ignoriert; nur die Beispielvorlage wird verfolgt.

Die meisten Benutzer benötigen nur `RUST_LOG=info` (oder `debug` für ausführliches Tracing). Die LLM-Audit-bezogenen Variablen (`OPENAI_API_KEY`, `OPENAI_BASE_URL`, `OPENAI_MODEL`) sind nur erforderlich, wenn du `scripts/audit_*.py --llm-audit` ausführst — ohne sie fallen die Audits in den Lint-Only-Modus zurück.

### Editor / IDE-Integrationen

- **VS Code** — siehe `vscode-extension/`
- **Chrome** — siehe `chrome-extension/`
- **JetBrains** — siehe `jetbrains-plugin/`

### SDKs

- **Node.js / TypeScript** — `npm i tokenslim-sdk` (Quelle: [`packages/sdk-nodejs/`](./packages/sdk-nodejs/))
- **Python** — siehe [`sdk/python/tokenslim_sdk.py`](./sdk/python/tokenslim_sdk.py) (Single-File-Client)
- **Java 11+** — siehe [`sdk/java/TokenSlimClient.java`](./sdk/java/TokenSlimClient.java)

> 📖 [5-Minuten-Quickstart](./docs/guides/QUICKSTART.md) · [Vollständiger SDK-Verwendungs-Leitfaden](./docs/guides/SDK_USAGE.md) · [Benutzerhandbuch](./docs/guides/USER_GUIDE.md)

## Verwendung

### CLI

```bash
# Build-Log komprimieren
./target/release/tokenslim -i build.log -o output.json --reorder

# KI-freundlicher entrauschter Diagnosebericht
./target/release/tokenslim decompress -i output.json -o ai_report.txt --ai-export

# Verlustbehafteter Hochsignal-Modus (behält Fehler-Fenster + Schlüssel-Metadaten)
./target/release/tokenslim decompress -i output.json -o ai_signal.txt --ai-signal

# Statische Regelvalidierung (einzelne Datei)
./target/release/tokenslim --verify-rule tests/fixtures/static_rule/sample_rule.toml \
  --verify-fixture tests/fixtures/static_rule/sample_fixture.log \
  --verify-expected tests/fixtures/static_rule/sample_expected.txt

# Statische Regelvalidierung (Stapel, Verzeichnismodus)
./target/release/tokenslim --verify-rule tests/fixtures/static_rule/sample_rule.toml \
  --verify-fixture tests/fixtures/static_rule \
  --verify-expected tests/fixtures/static_rule

# Projekt-Bootstrap & Shell-Hooks
./target/release/tokenslim init
./target/release/tokenslim workspace
./target/release/tokenslim --dry-run workspace --inject
./target/release/tokenslim workspace --inject
./target/release/tokenslim hooks install
./target/release/tokenslim hooks status
./target/release/tokenslim hooks uninstall
```

### Server (Sidecar)

```bash
./target/release/tokenslim-server
# Hört auf 127.0.0.1:<port>, stellt /health, /compress, /decompress bereit
```

### SDK

```python
# Python
from tokenslim import compress, decompress
compressed = compress(open("build.log").read())
print(decompress(compressed, mode="ai-export"))
```

```javascript
// Node.js
const { compress, decompress } = require("tokenslim-sdk");
const compressed = compress(fs.readFileSync("build.log", "utf8"));
console.log(decompress(compressed, { mode: "ai-export" }));
```

```java
// Java
TokenSlimClient client = new TokenSlimClient("http://127.0.0.1:8080");
String compressed = client.compress(logText);
String report = client.decompress(compressed, "ai-export");
```

## Plugins

TokenSlim liefert **60+ Plugins**, die die Eingaben abdecken, die den tatsächlichen LLM-Verkehr dominieren. Jedes Plugin ist datengetrieben (JSON / TOML-Konfig unter `config/plugins/`) und das Dispatching ist routenbasiert, sodass das Hinzufügen eines neuen Quellformats in den meisten Fällen nur eine Konfigurationsänderung ist.

Durchsuche das vollständige Verzeichnis unter [`config/plugins/`](./config/plugins/), oder führe aus:

```bash
./target/release/tokenslim plugins list
./target/release/tokenslim explain-plugin --explain-command "cargo build"
```

## Integrationen

| Oberfläche | Pfad | Status |
|---|---|---|
| CLI | `src/bin/tokenslim-server.rs`, `src/cli/` | Stable |
| REST Server | `src/bin/tokenslim-server.rs` | Stable |
| VS Code | `vscode-extension/` | Stable |
| Chrome | `chrome-extension/` | Stable |
| JetBrains | `jetbrains-plugin/` | Stable |
| Python SDK | `crates/tokenslim-py/` | Stable |
| Node.js SDK | `packages/sdk-nodejs/` (npm: `tokenslim-sdk@0.1.0`) | Stable |
| Java SDK | `sdk/java/` | Stable |

## Architektur

TokenSlim folgt einer geschichteten Pipeline:

1. **Route-Dispatcher** — wählt Plugin(s) nach Befehls- / Inhaltssignatur aus.
2. **Plugin-Kette** — jedes Plugin besitzt Extraktion, Faltung, semantische Substitution.
3. **Komprimierungs-Kern** — Radix-Trie-Pfadextraktion, Wörterbuch-Schichtung, globale Deduplizierung.
4. **Rehydrierung** — Round-Trip-sicher, die ursprüngliche Eingabe ist vollständig aus der komprimierten Form wiederherstellbar.
5. **AI Export / Signal** — kontextbewusste Nachbearbeitung für LLM-Konsum.

Siehe `docs/development/ARCHITECTURE.md` für das vollständige Design.

## Beitragen

Beiträge sind willkommen. Bitte öffne zuerst ein Issue, um größere Änderungen zu diskutieren; kleine Korrekturen und neue Plugin-Konfigurationen können direkt in einen PR gehen.

```bash
# Tests ausführen
cargo test

# Mit einer Probe ausführen
./target/release/tokenslim -i samples/web_log_plugin/case_001_access.log -o out.json --reorder
```

## Lizenz

[MIT](./LICENSE)
