<p align="center">
  <h1 align="center">TokenSlim</h1>
  <p align="center">
    Motor de compresión de tokens en Rust de alto rendimiento para entradas de LLM.<br>
    Basado en plugins · 50%–95% de ahorro de tokens · Diagnósticos con exportación IA · CLI / Server / IDE / SDK
  </p>
</p>

<p align="center">
  <a href="#qué-es-tokenslim">¿Qué es TokenSlim?</a> ·
  <a href="#por-qué-tokenslim">¿Por qué?</a> ·
  <a href="#características">Características</a> ·
  <a href="#instalación">Instalación</a> ·
  <a href="#uso">Uso</a> ·
  <a href="#plugins">Plugins</a> ·
  <a href="#integraciones">Integraciones</a> ·
  <a href="#licencia">Licencia</a>
</p>

<p align="center">
  <a href="./README.md">English</a> · <a href="./README.zh-CN.md">简体中文</a> · <a href="./README.ja.md">日本語</a> · <a href="./README.ko.md">한국어</a> · <strong>Español</strong> · <a href="./README.fr.md">Français</a> · <a href="./README.de.md">Deutsch</a> · <a href="./README.ar.md">العربية</a>
</p>

---

## ¿Qué es TokenSlim?

TokenSlim es un motor de compresión de texto basado en plugins, escrito en Rust y de alto rendimiento. Su misión principal es **reducir drásticamente el coste de tokens de las entradas de LLM** y hacer posible encajar logs largos y ruidosos del mundo real (pipelines de build, ejecuciones de CI, access logs web, trazas de bases de datos, logs en la nube, salida de VCS, stack traces, etc.) en las ventanas de contexto del LLM, sin perder las señales de diagnóstico que el modelo necesita.

Sobre entradas altamente estructuradas y repetitivas (logs de compilador, salida de build, logs de CI, access logs, etc.), TokenSlim normalmente entrega una reducción del **50%–90%** preservando el 100% de la información original. En su modo **AI Export**, diseñado específicamente para consumo de LLM, la reducción alcanza el **90%–95%** con eliminación de ruido consciente del contexto que mantiene la ventana de error/warning que el modelo necesita para razonar.

Más allá de la compresión, TokenSlim incluye herramientas de diagnóstico de entorno (comandos `workspace`, `encoding`, `rule`, `env`) que auto-detectan OS, shell, página de códigos, configuración de encoding de Python/Node/JDK, señalan el riesgo de mojibake y emiten correcciones accionables. Combinado con una cadena de fallback de decodificación de subprocesos (UTF-8 primero, candidatos a página de códigos después), se mantiene fiable en entornos multilingües mezclados.

## ¿Por qué TokenSlim?

### 1. Ahorro real de dinero
El coste de la API de LLM está dominado por el conteo de tokens de entrada. TokenSlim lo recorta en un 50%–95%:

- **Menor factura de API** — 50%–95% menos tokens de entrada.
- **AI Export consciente del contexto (`--ai-export`)** — elimina líneas rutinarias, mantiene la ventana de error/warning que el modelo realmente necesita; reduce alucinaciones en entradas ruidosas.
- **Contexto efectivo más largo** — misma ventana de contexto, más señal real.
- **Prefill más rápido** — entradas más cortas suelen significar prefill del modelo más rápido y menor TTFT.

### 2. Rendimiento de grado industrial
- **Pipeline zero-copy** — construido sobre Rust `Cow<'a, str>`, procesamiento paralelo por bloques con `rayon` y asignación `Bump` arena. Procesa 100 MB de log industrial en **~250 ms**, ~400 MB/s de rendimiento.
- **Reordenamiento global determinista** — un tracker de targets de build en streaming corrige la intercalación desordenada que produce `make -jN` / `Ninja`. Dos builds paralelos idénticos siempre producen el mismo orden de stack de error.
- **Modo sidecar** — servidor API REST de alto rendimiento, embebible en flujos de IDE / CI / Agent con cero overhead de arranque.

### 3. Extracción basada en datos
- **Extracción de rutas con trie de radix** — TokenSlim no corta línea por línea. Tras escanear 100 MB de entrada, construye un trie de radix de todo el proyecto en memoria y solo emite diccionarios de directorio (`$D`) en las ramas calientes (peso > 10), eliminando tokens fragmentarios.
- **Marcadores semánticos** — sustituciones conscientes del entorno para Android, iOS, GCC, MSVC y linkers.
- **Detección del ecosistema completo de build** — C/C++, Rust, Go, Java, Android, iOS/Xcode, MSVC, Swift y los principales linkers, con plegado consciente del contexto y deduplicación de errores.

## Características

- **Tres runtimes**
  - **CLI** — procesamiento por lotes scriptable
  - **Server** — API REST de larga vida para integración total con el ecosistema
  - **SDKs** — Java, Python (PyO3), Node.js
- **Ecosistema de plugins** (60+ plugins cubriendo las fuentes de entrada de LLM más comunes)
  - **Móvil** — `android_gradle`, `xcode_log`
  - **Desarrollo general** — `gcc_log`, `java_stack`, `python_traceback`, `dotnet`, `rust_go`, `maven`, `gradle`, `node_error`, `nodejs`, `php_ruby`, `unity_unreal`
  - **Datos estructurados** — `json`, `yaml`, `xml_html`, `ndjson`, `protobuf`
  - **Artefactos de build** — `artifact_summary` (SARIF / JUnit XML), con preservación semántica de estado de test, SARIF level/rule/location/tool
  - **Cloud & ops** — `cloud_log` (AWS / GCP / Azure / Alibaba / OCI / Tencent / Huawei / Cloudflare), `web_log` (Nginx / Apache / ingress / Envoy / CloudFront / IIS / ALB / Cloudflare), `db_log` (PostgreSQL / MySQL / MongoDB / Redis), `syslog`
  - **CI/CD** — `ci_log` (GitHub Actions / GitLab CI / Jenkins / Azure Pipelines / CircleCI / Buildkite / `act` local / TeamCity / Travis CI)
  - **VCS** — `vcs_plugin` unificado para git / svn / hg / p4 / cvs / bzr / fossil / darcs, más `git_diff`, `smart_code` (nivel AST), `smart_path`
- **Diagnóstico de entorno** — los subcomandos `workspace`, `encoding`, `rule`, `env` detectan riesgo de mojibake y emiten recetas de corrección.
- **Modos de salida nativos de IA**
  - `--ai-export` — eliminación de ruido consciente del contexto, mantiene la ventana de error/warning
  - `--ai-signal` — con pérdida pero alta señal, preserva los campos más relevantes para la decisión
- **Introspección de plugins** — `tokenslim explain-plugin` y `tokenslim run --explain-route` explican la selección de ruta, fallbacks, confianza, alternativas y reproducen clasificaciones erróneas para auditoría.

## Instalación

### Desde código fuente (Rust toolchain ≥ 1.75)

```bash
git clone https://github.com/nuoyazhizhou/tokenslim.git
cd tokenslim
cargo build --release
```

El binario queda en `./target/release/tokenslim` (o `tokenslim.exe` en Windows).

### Binarios precompilados

Descarga desde la página de [Releases](https://github.com/nuoyazhizhou/tokenslim/releases).

### Configuración (opcional)

Toda la configuración de runtime va a través de variables de entorno. Copia [`.env.example`](./.env.example) a `.env` y rellena los valores locales. `.env` se ignora en git por defecto; solo se trackea la plantilla de ejemplo.

La mayoría de usuarios solo necesitan `RUST_LOG=info` (o `debug` para trazado verboso). Las variables relacionadas con auditoría LLM (`OPENAI_API_KEY`, `OPENAI_BASE_URL`, `OPENAI_MODEL`) solo son necesarias si ejecutas `scripts/audit_*.py --llm-audit` — sin ellas, las auditorías degradan a modo solo-lint.

### Integraciones con editor / IDE

- **VS Code** — ver `vscode-extension/`
- **Chrome** — ver `chrome-extension/`
- **JetBrains** — ver `jetbrains-plugin/`

### SDKs

- **Node.js / TypeScript** — `npm i tokenslim` (fuente: [`packages/sdk-nodejs/`](./packages/sdk-nodejs/))
- **Python** — ver [`sdk/python/tokenslim_sdk.py`](./sdk/python/tokenslim_sdk.py) (cliente de un solo archivo)
- **Java 11+** — ver [`sdk/java/TokenSlimClient.java`](./sdk/java/TokenSlimClient.java)

> 📖 [Quickstart de 5 minutos](./docs/guides/QUICKSTART.md) · [Guía completa de uso de SDK](./docs/guides/SDK_USAGE.md) · [Guía de usuario](./docs/guides/USER_GUIDE.md)

## Uso

### CLI

```bash
# Comprimir un log de build
./target/release/tokenslim -i build.log -o output.json --reorder

# Reporte diagnóstico desruidizado para IA
./target/release/tokenslim decompress -i output.json -o ai_report.txt --ai-export

# Modo con pérdida de alta señal (mantiene ventana de error + metadata clave)
./target/release/tokenslim decompress -i output.json -o ai_signal.txt --ai-signal

# Validación de regla estática (archivo único)
./target/release/tokenslim --verify-rule tests/fixtures/static_rule/sample_rule.toml \
  --verify-fixture tests/fixtures/static_rule/sample_fixture.log \
  --verify-expected tests/fixtures/static_rule/sample_expected.txt

# Validación de regla estática (batch, modo directorio)
./target/release/tokenslim --verify-rule tests/fixtures/static_rule/sample_rule.toml \
  --verify-fixture tests/fixtures/static_rule \
  --verify-expected tests/fixtures/static_rule

# Bootstrap del proyecto y hooks de shell
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
# Escucha en 127.0.0.1:<port>, expone /health, /compress, /decompress
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

TokenSlim incluye **60+ plugins** cubriendo las entradas que dominan el tráfico real de LLM. Cada plugin es data-driven (config JSON / TOML bajo `config/plugins/`) y el dispatch es por ruta, por lo que añadir un nuevo formato de fuente es, en la mayoría de los casos, un cambio solo de configuración.

Explora el registro completo en [`config/plugins/`](./config/plugins/), o ejecuta:

```bash
./target/release/tokenslim plugins list
./target/release/tokenslim explain-plugin --explain-command "cargo build"
```

## Integraciones

| Superficie | Ruta | Estado |
|---|---|---|
| CLI | `src/bin/tokenslim-server.rs`, `src/cli/` | Stable |
| REST Server | `src/bin/tokenslim-server.rs` | Stable |
| VS Code | `vscode-extension/` | Stable |
| Chrome | `chrome-extension/` | Stable |
| JetBrains | `jetbrains-plugin/` | Stable |
| Python SDK | `crates/tokenslim-py/` | Stable |
| Node.js SDK | `packages/sdk-nodejs/` (npm: `tokenslim@0.1.0`) | Stable |
| Java SDK | `sdk/java/` | Stable |

## Arquitectura

TokenSlim sigue un pipeline en capas:

1. **Route dispatcher** — selecciona plugin(es) por firma de comando / contenido.
2. **Cadena de plugins** — cada plugin posee extracción, plegado y sustitución semántica.
3. **Núcleo de compresión** — extracción de rutas con trie de radix, capas de diccionario, deduplicación global.
4. **Rehidratación** — round-trip safe, la entrada original es totalmente recuperable desde la forma comprimida.
5. **AI Export / Signal** — post-procesamiento consciente del contexto para consumo de LLM.

Ver `docs/development/ARCHITECTURE.md` para el diseño completo.

## Contribuir

Las contribuciones son bienvenidas. Por favor, abre primero un issue para discutir cambios mayores; las correcciones pequeñas y nuevas configs de plugins pueden ir directamente a un PR.

```bash
# Ejecutar tests
cargo test

# Ejecutar con una muestra
./target/release/tokenslim -i samples/web_log_plugin/case_001_access.log -o out.json --reorder
```

## Licencia

[MIT](./LICENSE)
