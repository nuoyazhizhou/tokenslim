<p align="center">
  <h1 align="center">TokenSlim</h1>
  <p align="center">
    LLM 입력을 위한 고성능 Rust 토큰 압축 엔진。<br>
    플러그인 기반 · 50%–95% 토큰 절감 · AI 진단 내보내기 · CLI / Server / IDE / SDK
  </p>
</p>

<p align="center">
  <a href="#tokenslim이란">TokenSlim이란?</a> ·
  <a href="#왜-tokenslim인가">왜 TokenSlim인가</a> ·
  <a href="#주요-기능">주요 기능</a> ·
  <a href="#설치">설치</a> ·
  <a href="#사용법">사용법</a> ·
  <a href="#플러그인">플러그인</a> ·
  <a href="#통합">통합</a> ·
  <a href="#라이선스">라이선스</a>
</p>

<p align="center">
  <a href="./README.md">English</a> · <a href="./README.zh-CN.md">简体中文</a> · <a href="./README.ja.md">日本語</a> · <strong>한국어</strong> · <a href="./README.es.md">Español</a> · <a href="./README.fr.md">Français</a> · <a href="./README.de.md">Deutsch</a> · <a href="./README.ar.md">العربية</a>
</p>

---

## TokenSlim이란?

TokenSlim은 Rust로 작성된 고성능 플러그인 기반 텍스트 압축 엔진입니다. 핵심 사명은 **LLM 입력의 토큰 비용을 극적으로 줄이는 것**이며, 길고 노이즈가 많은 실제 로그(빌드 파이프라인, CI 실행, 웹 액세스 로그, 데이터베이스 트레이스, 클라우드 로그, VCS 출력, 스택 트레이스 등)를 모델이 필요로 하는 진단 신호를 잃지 않고 LLM 컨텍스트 윈도우에 맞출 수 있게 합니다.

고도로 구조화되고 반복적인 입력(컴파일러 로그, 빌드 출력, CI 로그, 액세스 로그 등)에서 TokenSlim은 일반적으로 원본 정보의 100%를 보존하면서 **50%–90%** 의 감소를 제공합니다. LLM 소비 전용으로 설계된 **AI Export** 모드에서는 감소율이 **90%–95%** 에 달하며, 모델이 추론해야 하는 오류/경고 컨텍스트를 유지하는 문맥 인식 노이즈 제거를 수행합니다.

압축 외에도 TokenSlim에는 환경 진단 도구(`workspace`, `encoding`, `rule`, `env` 명령)가 함께 제공되어 OS, 셸, 코드 페이지, Python/Node/JDK 인코딩 구성을 자동 감지하고, mojibake 위험을 플래그하며, 실행 가능한 수정 사항을 출력합니다. 서브프로세스 디코딩 폴백 체인(UTF-8 우선, 코드 페이지 후보 다음)과 결합되어 다국어 혼합 환경에서도 안정성을 유지합니다.

## 왜 TokenSlim인가?

### 1. 진짜 비용 절감
LLM API 비용은 입력 토큰 수에 의해 좌우됩니다. TokenSlim은 이를 50%–95% 줄입니다:

- **API 비용 절감** — 입력 토큰 50%–95% 감소.
- **문맥 인식 AI Export (`--ai-export`)** — 일상적인 줄을 제거하고 모델이 실제로 필요로 하는 오류/경고 윈도우를 유지. 노이즈가 많은 입력에서 환각을 줄임.
- **더 긴 유효 컨텍스트** — 동일한 컨텍스트 윈도우에서 더 많은 실제 신호.
- **더 빠른 프리필** — 입력이 짧을수록 일반적으로 모델 프리필이 빨라지고 TTFT가 낮아짐.

### 2. 산업 등급 성능
- **제로 카피 파이프라인** — Rust `Cow<'a, str>`, `rayon` 병렬 블록 처리, `Bump` 아레나 할당 위에 구축. 산업 등급 로그 100MB를 **약 250ms**, 약 400MB/s 처리량으로 처리.
- **결정론적 글로벌 재정렬** — 스트리밍 빌드 대상 추적기가 `make -jN` / `Ninja`가 생성한 순서가 맞지 않는 인터리빙을 수정. 동일한 병렬 빌드 두 번이 항상 동일한 오류 스택 순서를 생성.
- **사이드카 모드** — 고처리량 REST API 서버, 시작 오버헤드 없이 IDE / CI / Agent 워크플로에 임베드 가능.

### 3. 데이터 기반 추출
- **래딕스 트라이 경로 추출** — TokenSlim은 줄별로 슬라이스하지 않습니다. 100MB의 입력을 스캔한 후 프로젝트 전체 래딕스 트라이를 메모리에 구축하고 핫 브랜치(가중치 > 10)에서만 디렉토리 사전(`$D`)을 출력하여 파편화된 토큰을 제거.
- **의미론적 마커** — Android, iOS, GCC, MSVC, 링커를 위한 환경 인식 치환.
- **전체 빌드 생태계 감지** — C/C++, Rust, Go, Java, Android, iOS/Xcode, MSVC, Swift 및 주요 링커, 문맥 인식 접기와 오류 중복 제거 포함.

## 주요 기능

- **3가지 런타임**
  - **CLI** — 스크립트 가능한 배치 처리
  - **Server** — 전체 생태계 통합을 위한 장기 실행 REST API
  - **SDK** — Java, Python(PyO3), Node.js
- **플러그인 생태계** (실제 LLM 트래픽을 지배하는 입력을 다루는 60+ 플러그인)
  - **모바일** — `android_gradle`, `xcode_log`
  - **일반 개발** — `gcc_log`, `java_stack`, `python_traceback`, `dotnet`, `rust_go`, `maven`, `gradle`, `node_error`, `nodejs`, `php_ruby`, `unity_unreal`
  - **구조화된 데이터** — `json`, `yaml`, `xml_html`, `ndjson`, `protobuf`
  - **빌드 산출물** — `artifact_summary` (SARIF / JUnit XML), 테스트 상태, SARIF level/rule/location/tool의 의미론적 보존
  - **클라우드 및 운영** — `cloud_log` (AWS / GCP / Azure / Alibaba / OCI / Tencent / Huawei / Cloudflare), `web_log` (Nginx / Apache / ingress / Envoy / CloudFront / IIS / ALB / Cloudflare), `db_log` (PostgreSQL / MySQL / MongoDB / Redis), `syslog`
  - **CI/CD** — `ci_log` (GitHub Actions / GitLab CI / Jenkins / Azure Pipelines / CircleCI / Buildkite / 로컬 `act` / TeamCity / Travis CI)
  - **VCS** — git / svn / hg / p4 / cvs / bzr / fossil / darcs용 통합 `vcs_plugin`, 추가로 `git_diff`, `smart_code` (AST 레벨), `smart_path`
- **환경 진단** — `workspace`, `encoding`, `rule`, `env` 서브명령이 mojibake 위험을 감지하고 수정 레시피를 출력.
- **AI 네이티브 출력 모드**
  - `--ai-export` — 문맥 인식 노이즈 제거, 오류/경고 윈도우 유지
  - `--ai-signal` — 손실이 있지만 고신호, 의사 결정에 가장 관련 있는 필드 보존
- **플러그인 인트로스펙션** — `tokenslim explain-plugin`과 `tokenslim run --explain-route`가 라우트 선택, 폴백, 신뢰도, 대안을 설명하고 감사용 오분류를 재생.

## 설치

### 소스에서 빌드 (Rust toolchain ≥ 1.75)

```bash
git clone https://github.com/nuoyazhizhou/tokenslim.git
cd tokenslim
cargo build --release
```

바이너리는 `./target/release/tokenslim` (Windows에서는 `tokenslim.exe`)에 위치합니다.

### 미리 빌드된 바이너리

[Releases](https://github.com/nuoyazhizhou/tokenslim/releases) 페이지에서 다운로드.

### 설정 (선택)

모든 런타임 설정은 환경 변수를 통해 이루어집니다. [`.env.example`](./.env.example)을 `.env`로 복사하고 로컬 값을 채우세요. `.env`는 기본적으로 git에서 무시됩니다. 추적되는 것은 예제 템플릿뿐입니다.

대부분의 사용자는 `RUST_LOG=info` (상세 추적은 `debug`)만 필요합니다. LLM 감사 관련 변수(`OPENAI_API_KEY`, `OPENAI_BASE_URL`, `OPENAI_MODEL`)는 `scripts/audit_*.py --llm-audit`을 실행할 때만 필요합니다. 이것이 없으면 감사는 lint 전용 모드로 저하됩니다.

### 에디터 / IDE 통합

- **VS Code** — `vscode-extension/` 참조
- **Chrome** — `chrome-extension/` 참조
- **JetBrains** — `jetbrains-plugin/` 참조

### SDK

- **Node.js / TypeScript** — `npm i tokenslim-sdk` (소스: [`packages/sdk-nodejs/`](./packages/sdk-nodejs/))
- **Python** — [`sdk/python/tokenslim_sdk.py`](./sdk/python/tokenslim_sdk.py) (단일 파일 클라이언트)
- **Java 11+** — [`sdk/java/TokenSlimClient.java`](./sdk/java/TokenSlimClient.java)

> 📖 [5분 Quickstart](./docs/guides/QUICKSTART.md) · [SDK 사용 가이드](./docs/guides/SDK_USAGE.md) · [사용자 가이드](./docs/guides/USER_GUIDE.md)

## 사용법

### CLI

```bash
# 빌드 로그 압축
./target/release/tokenslim -i build.log -o output.json --reorder

# AI 친화적 노이즈 제거 진단 리포트
./target/release/tokenslim decompress -i output.json -o ai_report.txt --ai-export

# 고신호 손실 모드 (오류 윈도우 + 핵심 메타데이터 보존)
./target/release/tokenslim decompress -i output.json -o ai_signal.txt --ai-signal

# 정적 규칙 검증 (단일 파일)
./target/release/tokenslim --verify-rule tests/fixtures/static_rule/sample_rule.toml \
  --verify-fixture tests/fixtures/static_rule/sample_fixture.log \
  --verify-expected tests/fixtures/static_rule/sample_expected.txt

# 정적 규칙 검증 (배치, 디렉토리 모드)
./target/release/tokenslim --verify-rule tests/fixtures/static_rule/sample_rule.toml \
  --verify-fixture tests/fixtures/static_rule \
  --verify-expected tests/fixtures/static_rule

# 프로젝트 부트스트랩 및 셸 훅
./target/release/tokenslim init
./target/release/tokenslim workspace
./target/release/tokenslim --dry-run workspace --inject
./target/release/tokenslim workspace --inject
./target/release/tokenslim hooks install
./target/release/tokenslim hooks status
./target/release/tokenslim hooks uninstall
```

### Server (사이드카)

```bash
./target/release/tokenslim-server
# 127.0.0.1:<port>에서 리슨, /health, /compress, /decompress 제공
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

## 플러그인

TokenSlim은 실제 LLM 트래픽을 지배하는 입력을 다루는 **60+ 플러그인** 과 함께 제공됩니다. 각 플러그인은 데이터 기반(`config/plugins/` 아래의 JSON / TOML 설정)이고 디스패치는 라우트 기반이므로, 새 소스 형식 추가는 대부분의 경우 설정 변경만으로 가능합니다.

전체 레지스트리는 [`config/plugins/`](./config/plugins/)에서 확인하거나 다음을 실행하세요:

```bash
./target/release/tokenslim plugins list
./target/release/tokenslim explain-plugin --explain-command "cargo build"
```

## 통합

| 표면 | 경로 | 상태 |
|---|---|---|
| CLI | `src/bin/tokenslim-server.rs`, `src/cli/` | Stable |
| REST Server | `src/bin/tokenslim-server.rs` | Stable |
| VS Code | `vscode-extension/` | Stable |
| Chrome | `chrome-extension/` | Stable |
| JetBrains | `jetbrains-plugin/` | Stable |
| Python SDK | `crates/tokenslim-py/` | Stable |
| Node.js SDK | `packages/sdk-nodejs/` (npm: `tokenslim-sdk@0.1.0`) | Stable |
| Java SDK | `sdk/java/` | Stable |

## 아키텍처

TokenSlim은 계층화된 파이프라인을 따릅니다:

1. **라우트 디스패처** — 명령 / 콘텐츠 시그니처로 플러그인을 선택.
2. **플러그인 체인** — 각 플러그인이 추출, 접기, 의미론적 치환을 소유.
3. **압축 코어** — 래딕스 트라이 경로 추출, 사전 레이어링, 글로벌 중복 제거.
4. **재수화** — 라운드트립 안전, 압축된 형식에서 원본 입력을 완전히 복구 가능.
5. **AI Export / Signal** — LLM 소비를 위한 문맥 인식 후처리.

전체 설계는 `docs/development/ARCHITECTURE.md`를 참조하세요.

## 기여

기여를 환영합니다. 더 큰 변경 사항은 먼저 Issue를 열어 논의해 주세요. 작은 수정과 새 플러그인 설정은 바로 PR로 가도 됩니다.

```bash
# 테스트 실행
cargo test

# 샘플로 실행
./target/release/tokenslim -i samples/web_log_plugin/case_001_access.log -o out.json --reorder
```

## 라이선스

[MIT](./LICENSE)
