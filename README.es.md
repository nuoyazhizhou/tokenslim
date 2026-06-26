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

#### Docker

```bash
# Imagen oficial (multi-arquitectura: linux/amd64 + linux/arm64)
docker run -d -p 10086:10086 ghcr.io/nuoyazhizhou/tokenslim:latest

# Con autenticación por API Key
docker run -d -p 10086:10086 -e TOKENSLIM_API_KEY=my-secret ghcr.io/nuoyazhizhou/tokenslim:latest

# Con autenticación JWT
docker run -d -p 10086:10086 \
  -e TOKENSLIM_AUTH_MODE=jwt \
  -e TOKENSLIM_JWT_SECRET=my-secret \
  -e TOKENSLIM_API_KEY=my-key \
  ghcr.io/nuoyazhizhou/tokenslim:latest
```

#### Autenticación JWT

TokenSlim Server admite tres modos de autenticación:

| Modo | Descripción |
|---|---|
| `static` (predeterminado) | API Key tradicional mediante `Authorization: Bearer <key>` |
| `jwt` | Intercambiar API Key por token JWT mediante `POST /auth/token`, luego usar JWT |
| `none` | Sin autenticación (solo desarrollo) |

```bash
# Obtener un token JWT
curl -X POST http://127.0.0.1:10086/auth/token \
  -H "Authorization: Bearer YOUR_API_KEY"
# {"token":"eyJ...","expires_in":3600,"token_type":"Bearer"}

# Renovar antes del vencimiento
curl -X POST http://127.0.0.1:10086/auth/refresh \
  -H "Authorization: Bearer YOUR_CURRENT_JWT"
```

#### WebSocket — Canal bidireccional de compresión

El endpoint `/ws/compress` proporciona un canal bidireccional persistente:

- **Tramas Binary** → datos sin procesar → comprimidos → respuesta en trama Binary
- **Tramas Text** → comandos de control JSON:
  - `{"action":"flush"}` — comprimir inmediatamente y limpiar buffer
  - `{"action":"reset"}` — limpiar buffer y reiniciar sesión
  - `{"plugin":"<name>"}` — cambiar plugin de compresión

#### Gestión de configuración de plugins

```bash
tokenslim config plugin status                       # Ver estado de todos los plugins
tokenslim config plugin disable gcc_log_plugin       # Desactivar un plugin
tokenslim config plugin enable gcc_log_plugin        # Activar un plugin
tokenslim config plugin list-params gcc_log_plugin   # Ver parámetros configurables
tokenslim config plugin set gcc_log_plugin convert_timestamps false
tokenslim config plugin reset                        # Restablecer toda la configuración de plugins
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

## Reordenamiento de logs (Log Reordering)

![Log Reordering: BEFORE vs AFTER](docs/webui-screenshots/reorder-before-after.png)

Las herramientas de build paralelo (`make -jN`, `ninja`, Bazel, MSBuild, …) entrelazan los logs de varios objetivos en un orden **no determinista** que rompe cualquier diff / caché / comparación de regresión. TokenSlim incluye un **reordenador global determinista** que procesa el log en streaming, rastrea el objetivo de build activo y emite las líneas en un orden estable agrupado por objetivo.

```bash
# Integrado: el flag --reorder fuerza el reordenador y cae a modo serie
tokenslim -i build.log -o output.json --reorder

# Herramienta autónoma: diff log→log puro (Jenkins / CI) sin pipeline completo
cargo build --release --bin log_reorder
./target/release/log_reorder -i messy_build.log -o sorted_build.log --deterministic -n -p
#   --deterministic  : agrupa líneas por módulo / objetivo de build
#   -n  (--normalize) : ordena flags desordenados, enmascara direcciones & hashes
#   -p  (--shorten-paths) : acorta /home/userA/workspace/... a los últimos 3 segmentos
```

El mismo motor está expuesto vía `POST /compress` (campo `reorder: true`), la casilla «Habilitar reorden» de la WebUI y los SDK de Python / Node.

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

## 🛡️ Gobernanza de Agentes de IA y Sandbox Anti-Desviación (AI Agent Governance & Anti-Drift Sandbox)

El problema más difícil en la generación autónoma de código por IA es **"evitar que el Agente de Codificación escriba código y luego escriba sus propios pruebas simuladas (mock tests) auto-complacientes (basura entra, basura sale)"**, y **"evitar que las refactorizaciones posteriores introduzcan una desviación silenciosa del objetivo (regresiones de comportamiento)."**

En un ecosistema complejo con más de **105k+ LOC de código fuente central, más de 60 complementos (plugins) y más de 1000 casos de prueba físicos**, TokenSlim se mantiene robusto no mediante la depuración manual, sino a través de un **Sandbox de Calidad** automatizado de circuito cerrado que domestica el comportamiento de generación de código de la IA:

1. **Extracción de Intenciones e Inyección de Documentación de Código ([`extract_plugin_design.py`](scripts/extract_plugin_design.py))**: Escanea el código fuente del parser, aprovecha los LLM para extraer los contratos de diseño centrales (`design_intent`/`keep_signals`) y **los inyecta automáticamente de nuevo en `mod.rs` como comentarios de documentación `//!` a nivel de módulo**. Esto obliga a los futuros programadores (tanto humanos como IA) a respetar los límites del diseño como la única fuente de verdad.
2. **Sincronización Automática de Traducción Multilingüe ([`translate_messages_fields.py`](scripts/translate_messages_fields.py))**: Verifica de forma cruzada los archivos de traducción para chino e inglés, alineando automáticamente las entradas faltantes mediante traducción LLM para evitar la pérdida de claves.
3. **Auditoría de Calidad de Casos Físicos ([`audit_sample_case_quality.py`](scripts/audit_sample_case_quality.py))**: El Micro-Juez (Micro-Judge). Valida el realismo y la alineación de los casos de registros físicos sin procesar frente al contrato de diseño extraído, filtrando los casos falsos o sintetizados por la IA. Escanea las carpetas de muestras y **genera automáticamente líneas de código para añadir a `showcase.rs`** si hay nuevos registros físicos no registrados.
4. **Auditoría de Fidelidad de Compresión ([`audit_case_metrics.py`](scripts/audit_case_metrics.py))**: El Meso-Juez (Meso-Judge). Garantiza la alineación entre la **configuración en `showcase.rs`**, los **archivos físicos en `samples/`** y los **informes generados en `target/`**. Verifica las compuertas deterministas G1-G4 (asegurando que los errores críticos y los anclajes de comando nunca se pierdan) y utiliza LLM para cotejar la compresión con el contrato de diseño.
5. **Congelación de Estado y Prevención de Regresiones (State Freeze)**: Una vez auditado, el resultado se bloquea con un hash SHA256. Si los cambios futuros de la IA rompen la salida esperada, el pipeline de CI/CD **rechaza y bloquea instantáneamente el lanzamiento**, evitando la desviación silenciosa.
6. **Gobernanza de Salud Global ([`audit_all_case_metrics.py`](scripts/audit_all_case_metrics.py))**: El Macro-Juez (Macro-Judge). Coordina auditorías paralelas en los más de 60 complementos en CI/CD, compilando una matriz de salud global (`audit_health.md`) para finalizar el bloqueo de calidad definitivo.

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
