<p align="center">
  <h1 align="center">TokenSlim</h1>
  <p align="center">
    Moteur de compression de tokens Rust haute performance pour les entrées LLM.<br>
    À base de plugins · 50 %–95 % d'économie de tokens · Diagnostics avec export IA · CLI / Server / IDE / SDK
  </p>
</p>

<p align="center">
  <a href="#quest-ce-que-tokenslim">Qu'est-ce que TokenSlim ?</a> ·
  <a href="#pourquoi-tokenslim">Pourquoi</a> ·
  <a href="#fonctionnalités">Fonctionnalités</a> ·
  <a href="#installation">Installation</a> ·
  <a href="#utilisation">Utilisation</a> ·
  <a href="#plugins">Plugins</a> ·
  <a href="#intégrations">Intégrations</a> ·
  <a href="#licence">Licence</a>
</p>

<p align="center">
  <a href="./README.md">English</a> · <a href="./README.zh-CN.md">简体中文</a> · <a href="./README.ja.md">日本語</a> · <a href="./README.ko.md">한국어</a> · <a href="./README.es.md">Español</a> · <strong>Français</strong> · <a href="./README.de.md">Deutsch</a> · <a href="./README.ar.md">العربية</a>
</p>

---

## Qu'est-ce que TokenSlim ?

TokenSlim est un moteur de compression de texte haute performance, à base de plugins, écrit en Rust. Sa mission principale est de **réduire drastiquement le coût en tokens des entrées LLM** et de permettre de faire tenir des logs longs et bruités du monde réel (pipelines de build, exécutions CI, access logs web, traces de bases de données, logs cloud, sorties VCS, stack traces, etc.) dans la fenêtre de contexte du LLM — sans perdre les signaux de diagnostic dont le modèle a besoin.

Sur des entrées hautement structurées et répétitives (logs de compilateur, sortie de build, logs CI, access logs, etc.), TokenSlim offre typiquement une réduction de **50 %–90 %** tout en préservant 100 % de l'information originale. Dans son mode **AI Export**, conçu spécifiquement pour la consommation par un LLM, la réduction atteint **90 %–95 %** avec un débruitage contextuel qui conserve la fenêtre d'erreur/warning dont le modèle a besoin pour raisonner.

Au-delà de la compression, TokenSlim embarque des outils de diagnostic d'environnement (commandes `workspace`, `encoding`, `rule`, `env`) qui auto-détectent l'OS, le shell, la page de code, la configuration d'encodage Python/Node/JDK, signalent le risque de mojibake et émettent des correctifs actionnables. Combiné à une chaîne de fallback de décodage de sous-processus (UTF-8 d'abord, puis candidats de pages de code), il reste fiable dans des environnements multilingues mixtes.

## Pourquoi TokenSlim ?

### 1. De vraies économies
Le coût d'API LLM est dominé par le nombre de tokens d'entrée. TokenSlim le coupe de 50 %–95 % :

- **Facture API réduite** — 50 %–95 % de tokens d'entrée en moins.
- **AI Export contextuel (`--ai-export`)** — supprime les lignes routinières, garde la fenêtre d'erreur/warning dont le modèle a réellement besoin ; réduit les hallucinations sur des entrées bruitées.
- **Contexte effectif plus long** — même fenêtre de contexte, plus de signal réel.
- **Prefill plus rapide** — des entrées plus courtes signifient généralement un prefill plus rapide et un TTFT plus bas.

### 2. Performance de grade industriel
- **Pipeline zero-copy** — construit sur Rust `Cow<'a, str>`, traitement parallèle par blocs avec `rayon` et allocation arène `Bump`. Traite 100 Mo de log industriel en **~250 ms**, soit ~400 Mo/s de débit.
- **Réordonnancement global déterministe** — un tracker de cibles de build en streaming corrige l'entrelacement désordonné produit par `make -jN` / `Ninja`. Deux builds parallèles identiques produisent toujours le même ordre de pile d'erreurs.
- **Mode sidecar** — serveur API REST haut débit, embarquable dans des flux IDE / CI / Agent avec zéro overhead de démarrage.

### 3. Extraction pilotée par les données
- **Extraction de chemins par trie de radix** — TokenSlim ne découpe pas ligne par ligne. Après avoir scanné 100 Mo d'entrée, il construit un trie de radix global au projet en mémoire et n'émet des dictionnaires de répertoire (`$D`) que sur les branches chaudes (poids > 10), éliminant les tokens fragmentaires.
- **Marqueurs sémantiques** — substitutions conscientes de l'environnement pour Android, iOS, GCC, MSVC et les linkers.
- **Détection complète de l'écosystème de build** — C/C++, Rust, Go, Java, Android, iOS/Xcode, MSVC, Swift et les principaux linkers, avec pliage contextuel et déduplication d'erreurs.

## Fonctionnalités

- **Trois runtimes**
  - **CLI** — traitement par lots scriptable
  - **Server** — API REST longue durée pour intégration complète de l'écosystème
  - **SDKs** — Java, Python (PyO3), Node.js
- **Écosystème de plugins** (60+ plugins couvrant les sources d'entrée LLM les plus courantes)
  - **Mobile** — `android_gradle`, `xcode_log`
  - **Développement général** — `gcc_log`, `java_stack`, `python_traceback`, `dotnet`, `rust_go`, `maven`, `gradle`, `node_error`, `nodejs`, `php_ruby`, `unity_unreal`
  - **Données structurées** — `json`, `yaml`, `xml_html`, `ndjson`, `protobuf`
  - **Artefacts de build** — `artifact_summary` (SARIF / JUnit XML), avec préservation sémantique de l'état de test, SARIF level/rule/location/tool
  - **Cloud & ops** — `cloud_log` (AWS / GCP / Azure / Alibaba / OCI / Tencent / Huawei / Cloudflare), `web_log` (Nginx / Apache / ingress / Envoy / CloudFront / IIS / ALB / Cloudflare), `db_log` (PostgreSQL / MySQL / MongoDB / Redis), `syslog`
  - **CI/CD** — `ci_log` (GitHub Actions / GitLab CI / Jenkins / Azure Pipelines / CircleCI / Buildkite / `act` local / TeamCity / Travis CI)
  - **VCS** — `vcs_plugin` unifié pour git / svn / hg / p4 / cvs / bzr / fossil / darcs, plus `git_diff`, `smart_code` (niveau AST), `smart_path`
- **Diagnostic d'environnement** — les sous-commandes `workspace`, `encoding`, `rule`, `env` détectent le risque de mojibake et émettent des recettes de correction.
- **Modes de sortie natifs IA**
  - `--ai-export` — débruitage contextuel, conserve la fenêtre d'erreur/warning
  - `--ai-signal` — avec perte mais haut signal, préserve les champs les plus pertinents pour la décision
- **Introspection des plugins** — `tokenslim explain-plugin` et `tokenslim run --explain-route` expliquent la sélection de route, les fallbacks, la confiance, les alternatives, et rejouent les erreurs de classification pour audit.

## Installation

### Depuis les sources (Rust toolchain ≥ 1.75)

```bash
git clone https://github.com/nuoyazhizhou/tokenslim.git
cd tokenslim
cargo build --release
```

Le binaire atterrit dans `./target/release/tokenslim` (ou `tokenslim.exe` sous Windows).

### Binaires précompilés

Téléchargez depuis la page [Releases](https://github.com/nuoyazhizhou/tokenslim/releases).

### Configuration (optionnelle)

Toute la configuration runtime passe par des variables d'environnement. Copiez [`.env.example`](./.env.example) vers `.env` et remplissez vos valeurs locales. `.env` est ignoré par git par défaut ; seul le modèle d'exemple est suivi.

La plupart des utilisateurs n'ont besoin que de `RUST_LOG=info` (ou `debug` pour un traçage verbeux). Les variables liées à l'audit LLM (`OPENAI_API_KEY`, `OPENAI_BASE_URL`, `OPENAI_MODEL`) ne sont nécessaires que si vous exécutez `scripts/audit_*.py --llm-audit` — sans elles, les audits se dégradent en mode lint uniquement.

### Intégrations éditeur / IDE

- **VS Code** — voir `vscode-extension/`
- **Chrome** — voir `chrome-extension/`
- **JetBrains** — voir `jetbrains-plugin/`

### SDKs

- **Node.js / TypeScript** — `npm i tokenslim` (source : [`packages/sdk-nodejs/`](./packages/sdk-nodejs/))
- **Python** — voir [`sdk/python/tokenslim_sdk.py`](./sdk/python/tokenslim_sdk.py) (client mono-fichier)
- **Java 11+** — voir [`sdk/java/TokenSlimClient.java`](./sdk/java/TokenSlimClient.java)

> 📖 [Quickstart en 5 minutes](./docs/guides/QUICKSTART.md) · [Guide complet d'utilisation du SDK](./docs/guides/SDK_USAGE.md) · [Guide utilisateur](./docs/guides/USER_GUIDE.md)

## Utilisation

### CLI

```bash
# Compresser un log de build
tokenslim -i build.log -o output.json --reorder

# Rapport diagnostique débruité pour IA
tokenslim decompress -i output.json -o ai_report.txt --ai-export

# Mode avec perte haut signal (conserve la fenêtre d'erreur + métadonnées clés)
tokenslim decompress -i output.json -o ai_signal.txt --ai-signal

# Validation de règle statique (fichier unique)
tokenslim --verify-rule tests/fixtures/static_rule/sample_rule.toml \
  --verify-fixture tests/fixtures/static_rule/sample_fixture.log \
  --verify-expected tests/fixtures/static_rule/sample_expected.txt

# Validation de règle statique (batch, mode répertoire)
tokenslim --verify-rule tests/fixtures/static_rule/sample_rule.toml \
  --verify-fixture tests/fixtures/static_rule \
  --verify-expected tests/fixtures/static_rule

# Bootstrap de projet et hooks shell
tokenslim init
tokenslim workspace
tokenslim --dry-run workspace --inject
tokenslim workspace --inject
tokenslim hooks install
tokenslim hooks status
tokenslim hooks uninstall
```

### Server (Sidecar)

```bash
tokenslim-server
# Écoute sur 127.0.0.1:<port>, expose /health, /compress, /decompress
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
const { compress, decompress } = require("tokenslim");
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

TokenSlim embarque **60+ plugins** couvrant les entrées qui dominent le trafic LLM réel. Chaque plugin est data-driven (config JSON / TOML sous `config/plugins/`) et le dispatch est basé sur la route, donc ajouter un nouveau format de source est, dans la plupart des cas, un simple changement de config.

Parcourez le registre complet sur [`config/plugins/`](./config/plugins/), ou exécutez :

```bash
tokenslim plugins list
tokenslim explain-plugin --explain-command "cargo build"
```

## Intégrations

| Surface | Chemin | Statut |
|---|---|---|
| CLI | `src/bin/tokenslim-server.rs`, `src/cli/` | Stable |
| REST Server | `src/bin/tokenslim-server.rs` | Stable |
| VS Code | `vscode-extension/` | Stable |
| Chrome | `chrome-extension/` | Stable |
| JetBrains | `jetbrains-plugin/` | Stable |
| Python SDK | `crates/tokenslim-py/` | Stable |
| Node.js SDK | `packages/sdk-nodejs/` (npm : `tokenslim@0.1.0`) | Stable |
| Java SDK | `sdk/java/` | Stable |

## Architecture

TokenSlim suit un pipeline en couches :

1. **Route dispatcher** — sélectionne le(s) plugin(s) par signature de commande / contenu.
2. **Chaîne de plugins** — chaque plugin possède extraction, pliage, substitution sémantique.
3. **Cœur de compression** — extraction de chemins par trie de radix, couches de dictionnaires, déduplication globale.
4. **Réhydratation** — round-trip safe, l'entrée originale est entièrement récupérable depuis la forme compressée.
5. **AI Export / Signal** — post-traitement contextuel pour consommation par LLM.

Voir `docs/development/ARCHITECTURE.md` pour le design complet.

## Contribuer

Les contributions sont les bienvenues. Merci d'ouvrir d'abord une issue pour discuter des changements plus importants ; les petites corrections et les nouvelles configs de plugin peuvent aller directement en PR.

```bash
# Exécuter les tests
cargo test

# Exécuter avec un échantillon
tokenslim -i samples/web_log_plugin/case_001_access.log -o out.json --reorder
```

## Licence

[MIT](./LICENSE)
