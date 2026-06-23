<p align="center">
  <h1 align="center">TokenSlim</h1>
  <p align="center">
    Motor de compresión de tokens en Rust de alto rendimiento para entradas de LLM.<br>
    Basado en plugins · 50%–95% de ahorro de tokens · Diagnósticos con exportación IA · CLI / Server / IDE / SDK
  </p>
</p>

<p align="center">
  <a href="https://github.com/nuoyazhizhou/tokenslim/actions/workflows/build-release.yml"><img src="https://img.shields.io/github/actions/workflow/status/nuoyazhizhou/tokenslim/build-release.yml?branch=main&logo=github&style=flat-square" alt="Build Status"></a>
  <a href="https://www.npmjs.com/package/tokenslim"><img src="https://img.shields.io/npm/v/tokenslim?logo=npm&style=flat-square" alt="npm version"></a>
  <a href="https://pypi.org/project/tokenslim/"><img src="https://img.shields.io/pypi/v/tokenslim?logo=python&style=flat-square" alt="PyPI version"></a>
  <a href="https://github.com/nuoyazhizhou/tokenslim/blob/main/LICENSE"><img src="https://img.shields.io/github/license/nuoyazhizhou/tokenslim?style=flat-square" alt="License"></a>
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


## Míralo en Acción

### Uso diario en el mundo real — `tokenslim gain`

Así se ve `tokenslim gain` después de meses de uso diario en comandos de git:

```
$ tokenslim gain

TokenSlim Cumulative Savings Report
====================================

Usage Statistics:
  Total runs:          7,244
  Input tokens:        13.2M
  Output tokens:       9.4M
  Tokens saved:        3.9M
  Overall compression: 29.3%

Estimated Savings:
  Tokens saved:        3,883,551 tokens
       claude-4.8:     $19.42 USD ($5.00/1M)
       gpt-5.5:        $19.42 USD ($5.00/1M)
       gemini-3.1-pro: $7.77 USD  ($2.00/1M)
```

> 💡 `tokenslim gain` rastrea **cada compresión** que ejecutas y muestra los ahorros acumulados. Los números anteriores son del flujo de trabajo diario de un solo desarrollador; los ahorros de tu equipo se multiplicarán a partir de aquí.

### La compresión varía según el tipo de entrada

No todas las entradas se comprimen por igual, y eso es de esperar. Los registros altamente repetitivos y estructurados se comprimen mucho más que el contenido denso en información como los diffs de git:

<table>
<tr>
<th>Tipo de Entrada</th>
<th>Reducción Típica</th>
<th>Por qué</th>
</tr>
<tr>
<td>🔨 Registros de compilación (cargo, gcc, gradle)</td>
<td align="center"><strong>70–95%</strong></td>
<td>Repetición masiva: marcas de tiempo, líneas de progreso, salida de rutina</td>
</tr>
<tr>
<td>🌐 Registros de acceso web (Nginx, Apache)</td>
<td align="center"><strong>80–93%</strong></td>
<td>Estructura repetitiva: IPs, rutas, códigos de estado, user agents</td>
</tr>
<tr>
<td>🤖 Registros de CI/CD (GitHub Actions, Jenkins)</td>
<td align="center"><strong>70–92%</strong></td>
<td>Pasos de configuración, instalación de dependencias, salida de plantilla</td>
</tr>
<tr>
<td>☁️ Registros en la nube (AWS, GCP, Azure)</td>
<td align="center"><strong>60–90%</strong></td>
<td>JSON estructurado con campos repetitivos y metadatos</td>
</tr>
<tr>
<td>🔀 Salida de VCS (git log, git diff)</td>
<td align="center"><strong>20–40%</strong></td>
<td>Denso en información; menos redundancia que eliminar</td>
</tr>
</table>

> El rango general es del **20–95%** dependiendo de qué tan repetitiva y estructurada sea tu entrada. Usa `tokenslim gain` para rastrear tus ahorros reales a lo largo del tiempo.

**Antes** — `git status` (22 líneas, ~680 caracteres):
```
$ git status
On branch master
Changes to be committed:
  (use "git restore --staged <file>..." to unstage)
        modified:   .gitignore
        modified:   src/core/dictionary_engine/test.rs
        modified:   src/plugins/cloud_log_plugin/test.rs

Changes not staged for commit:
  (use "git add <file>..." to update what will be committed)
  (use "git restore <file>..." to discard changes in working directory)
        modified:   Cargo.toml
        modified:   resources/messages.zh-CN.json
        modified:   src/bin/tokenslim-server.rs
        modified:   src/core/plugin_config_loader/mod.rs

Untracked files:
  (use "git add <file>..." to include in what will be committed)
        tests/server_webui_e2e.rs
        webui/
```

**Después** — `tokenslim git status` (8 líneas, ~280 caracteres — misma información, cero pérdidas):
```
git status
BR:master
M .gitignore
M src/core/dictionary_engine/test.rs
M src/plugins/cloud_log_plugin/test.rs
M Cargo.toml
M resources/messages.zh-CN.json
M src/bin/tokenslim-server.rs
M src/core/plugin_config_loader/mod.rs
? tests/server_webui_e2e.rs
? webui/
```

> Todo desarrollador ejecuta `git status` docenas de veces al día. TokenSlim elimina las sugerencias repetitivas, unifica los marcadores de estado y ofrece la misma información con **~60% menos de tokens**, y esto se acumula en miles de interacciones con LLMs.
\n## ¿Por qué TokenSlim?

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
tokenslim -i build.log -o output.json --reorder

# Reporte diagnóstico desruidizado para IA
tokenslim decompress -i output.json -o ai_report.txt --ai-export

# Modo con pérdida de alta señal (mantiene ventana de error + metadata clave)
tokenslim decompress -i output.json -o ai_signal.txt --ai-signal

# Validación de regla estática (archivo único)
tokenslim --verify-rule tests/fixtures/static_rule/sample_rule.toml \
  --verify-fixture tests/fixtures/static_rule/sample_fixture.log \
  --verify-expected tests/fixtures/static_rule/sample_expected.txt

# Validación de regla estática (batch, modo directorio)
tokenslim --verify-rule tests/fixtures/static_rule/sample_rule.toml \
  --verify-fixture tests/fixtures/static_rule \
  --verify-expected tests/fixtures/static_rule

# Bootstrap del proyecto y hooks de shell
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
# Escucha en 127.0.0.1:<port>, expone /health, /compress, /decompress
```


#### Web UI

El sidecar incluye una interfaz de usuario de una sola página integrada para la compresión interactiva y el seguimiento en vivo de registros. Todos los recursos estáticos del frontend están **compilados directamente en el ejecutable binario**. Ya sea que se instale a través de npm o pip, se ejecuta de inmediato desde cualquier directorio sin configuración alguna.

![TokenSlim Web UI — inicio (zh-CN)](docs/webui-screenshots/01-home-zh.png)

##### Ejecutar

```bash
# Ejecutar desde cualquier directorio (sirve la Web UI integrada automáticamente)
tokenslim-server

# Modo de desarrollo frontend (sirve desde un directorio físico para hot-reloading)
TOKENSLIM_WEBUI_DIR=./webui tokenslim-server

# Elegir puerto y dirección de enlace
TOKENSLIM_PORT=10086 TOKENSLIM_HOST=127.0.0.1 tokenslim-server

# Desactivar autenticación para pruebas locales (predeterminado: apagado sin variable de entorno)
# TOKENSLIM_API_KEY=changeme tokenslim-server
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
tokenslim plugins list
tokenslim explain-plugin --explain-command "cargo build"
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

## Puertas de Calidad y Pipeline de Auditoría

TokenSlim mantiene cero pérdida semántica y alta confiabilidad a través de un estricto pipeline de auditoría de 4 pasos basado en datos. Cada cambio de parser o regla debe pasar estas puertas de calidad automatizadas:

1. **Puerta de Calidad de Muestra (`audit_sample_case_quality.py`)**: Valida que los casos de entrada en bruto (ej. registros de CI, seguimientos de pila) sean realistas, estén correctamente etiquetados y tengan un alto valor de diagnóstico antes de que comiencen las pruebas.
2. **Fidelidad Semántica y Puerta de Métricas (`audit_case_metrics.py`)**: Compara las entradas originales con sus salidas comprimidas. Aplica políticas estrictas (como Anchor Guard y Anti-Amnesia) para asegurar que la tasa de compresión mejore sin perder ningún contexto de error crítico. Los casos aprobados se "congelan" criptográficamente.
3. **Control de Salud Global (`audit_all_case_metrics.py`)**: Se ejecuta simultáneamente en los más de 60 complementos, actuando como la puerta final de CI. Falla la compilación si un solo complemento introduce una regresión de compresión o viola la fidelidad semántica.
4. **Sincronización de Matriz de Capacidades (`generate_plugin_capability_index.py`)**: Reconstruye automáticamente el índice global de enrutamiento de complementos en función de los casos congelados, asegurando que el enrutador dinámico esté siempre perfectamente sincronizado con las capacidades reales probadas.

## Contribuir

Las contribuciones son bienvenidas. Por favor, abre primero un issue para discutir cambios mayores; las correcciones pequeñas y nuevas configs de plugins pueden ir directamente a un PR.

```bash
# Ejecutar tests
cargo test

# Ejecutar con una muestra
tokenslim -i samples/web_log_plugin/case_001_access.log -o out.json --reorder
```

## Licencia

[MIT](./LICENSE)
