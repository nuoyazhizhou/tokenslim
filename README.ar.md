<div dir="rtl" align="right">

<p align="center">
  <h1 align="center">TokenSlim</h1>
  <p align="center">
    محرك ضغط رموز عالي الأداء مكتوب بـ Rust لمدخلات نماذج اللغة الكبيرة (LLM).<br>
    قائم على الإضافات · توفير 50%–95% من الرموز · تشخيصات التصدير بالذكاء الاصطناعي · CLI / Server / IDE / SDK
  </p>
</p>

<p align="center">
  <a href="#ما-هو-tokenslim">ما هو TokenSlim؟</a> ·
  <a href="#لماذا-tokenslim">لماذا؟</a> ·
  <a href="#الميزات">الميزات</a> ·
  <a href="#التثبيت">التثبيت</a> ·
  <a href="#الاستخدام">الاستخدام</a> ·
  <a href="#الإضافات">الإضافات</a> ·
  <a href="#التكاملات">التكاملات</a> ·
  <a href="#الترخيص">الترخيص</a>
</p>

<p align="center">
  <a href="./README.md">English</a> · <a href="./README.zh-CN.md">简体中文</a> · <a href="./README.ja.md">日本語</a> · <a href="./README.ko.md">한국어</a> · <a href="./README.es.md">Español</a> · <a href="./README.fr.md">Français</a> · <a href="./README.de.md">Deutsch</a> · <strong>العربية</strong>
</p>

---

## ما هو TokenSlim؟

TokenSlim هو محرك ضغط نصوص عالي الأداء قائم على الإضافات، مكتوب بـ Rust. مهمته الأساسية هي **خفض تكلفة الرموز لمدخلات نماذج LLM بشكل جذري**، وجعل من الممكن احتواء سجلات طويلة وضوضائية من العالم الحقيقي (خطوط أنابيب البناء، تشغيل CI، سجلات وصول الويب، آثار قواعد البيانات، سجلات السحابة، مخرجات VCS، آثار المكدس، إلخ) في نافذة سياق LLM — دون فقدان الإشارات التشخيصية التي يحتاجها النموذج.

على المدخلات عالية البنية والمتكررة (سجلات المترجم، مخرجات البناء، سجلات CI، سجلات الوصول، إلخ)، يقدم TokenSlim عادةً تقليلاً بنسبة **50%–90%** مع الحفاظ على 100% من المعلومات الأصلية. في وضع **AI Export** المصمم خصيصًا لاستهلاك LLM، يصل التخفيض إلى **90%–95%** مع إزالة ضوضاء مدركة للسياق تحتفظ بنافذة الخطأ/التحذير التي يحتاجها النموذج للاستدلال.

إلى جانب الضغط، يأتي TokenSlim بأدوات تشخيص البيئة (أوامر `workspace` و`encoding` و`rule` و`env`) التي تكتشف تلقائيًا نظام التشغيل والصدفة وصفحة الرموز وإعدادات ترميز Python/Node/JDK، وتضع علامة على خطر Mojibake وتصدر إصلاحات قابلة للتنفيذ. مدمجًا مع سلسلة احتياطية لفك ترميز العمليات الفرعية (UTF-8 أولاً، ثم مرشحات صفحات الرموز)، يبقى موثوقًا في البيئات متعددة اللغات.

## لماذا TokenSlim؟

### 1. توفير حقيقي للأموال
تتأثر تكلفة واجهة LLM بشكل كبير بعدد رموز الإدخال. TokenSlim يخفضها بنسبة 50%–95%:

- **فواتير API أقل** — 50%–95% رموز إدخال أقل.
- **تصدير AI مدرك للسياق (`--ai-export`)** — يزيل السطور الروتينية، ويحتفظ بنافذة الخطأ/التحذير التي يحتاجها النموذج فعليًا؛ يقلل من الهلوسة في المدخلات الضوضائية.
- **سياق فعال أطول** — نفس نافذة السياق، إشارة حقيقية أكثر.
- **Prefill أسرع** — المدخلات الأقصر تعني عادةً prefill أسرع للنموذج وTTFT أقل.

### 2. أداء بدرجة صناعية
- **خط أنابيب بدون نسخ (zero-copy)** — مبني على Rust `Cow<'a, str>`، ومعالجة كتل متوازية باستخدام `rayon`، وتخصيص ساحة `Bump`. يعالج 100 ميجابايت من سجلات بدرجة صناعية في **~250 مللي ثانية**، أي ما يقارب 400 ميجابايت/ثانية من الإنتاجية.
- **إعادة ترتيب عالمية حتمية** — متتبع أهداف بناء متدفق يصلح التداخل غير المرتب الذي تنتجه `make -jN` / `Ninja`. بنائان متوازيان متطابقان ينتجان دائمًا نفس ترتيب مكدس الخطأ.
- **وضع Sidecar** — خادم REST API عالي الإنتاجية، قابل للتضمين في سير عمل IDE / CI / Agent بدون أي تكلفة بدء تشغيل.

### 3. استخراج قائم على البيانات
- **استخراج المسار باستخدام Radix-Trie** — لا يقسم TokenSlim سطرًا بسطر. بعد مسح 100 ميجابايت من المدخلات، يبني radix-trie على مستوى المشروع في الذاكرة ولا يُصدر قواميس الدليل (`$D`) إلا على الفروع الساخنة (الوزن > 10)، مما يقضي على الرموز المجزأة.
- **علامات دلالية** — بدائل مدركة للبيئة لـ Android وiOS وGCC وMSVC والـ linkers.
- **كشف منظومة البناء الكاملة** — C/C++، Rust، Go، Java، Android، iOS/Xcode، MSVC، Swift، والـ linkers الرئيسية، مع طي مدرك للسياق وإزالة تكرار الأخطاء.

## الميزات

- **ثلاثة أوقات تشغيل**
  - **CLI** — معالجة دفعات قابلة للبرمجة
  - **Server** — واجهة REST API طويلة العمر لتكامل المنظومة الكاملة
  - **SDKs** — Java، Python (PyO3)، Node.js
- **منظومة الإضافات** (60+ إضافة تغطي مصادر إدخال LLM الأكثر شيوعًا)
  - **الجوال** — `android_gradle`، `xcode_log`
  - **التطوير العام** — `gcc_log`، `java_stack`، `python_traceback`، `dotnet`، `rust_go`، `maven`، `gradle`، `node_error`، `nodejs`، `php_ruby`، `unity_unreal`
  - **البيانات المهيكلة** — `json`، `yaml`، `xml_html`، `ndjson`، `protobuf`
  - **مخرجات البناء** — `artifact_summary` (SARIF / JUnit XML)، مع الحفاظ الدلالي على حالة الاختبار، SARIF level/rule/location/tool
  - **السحابة والعمليات** — `cloud_log` (AWS / GCP / Azure / Alibaba / OCI / Tencent / Huawei / Cloudflare)، `web_log` (Nginx / Apache / ingress / Envoy / CloudFront / IIS / ALB / Cloudflare)، `db_log` (PostgreSQL / MySQL / MongoDB / Redis)، `syslog`
  - **CI/CD** — `ci_log` (GitHub Actions / GitLab CI / Jenkins / Azure Pipelines / CircleCI / Buildkite / `act` محلي / TeamCity / Travis CI)
  - **VCS** — `vcs_plugin` موحد لـ git / svn / hg / p4 / cvs / bzr / fossil / darcs، بالإضافة إلى `git_diff`، `smart_code` (مستوى AST)، `smart_path`
- **تشخيص البيئة** — الأوامر الفرعية `workspace` و`encoding` و`rule` و`env` تكتشف خطر Mojibake وتصدر وصفات إصلاح.
- **أوضاع إخراج أصلية للذكاء الاصطناعي**
  - `--ai-export` — إزالة ضوضاء مدركة للسياق، تحتفظ بنافذة الخطأ/التحذير
  - `--ai-signal` — مع فقد ولكن بإشارة عالية، تحتفظ بأكثر الحقول صلة باتخاذ القرار
- **تأمل الإضافات** — `tokenslim explain-plugin` و`tokenslim run --explain-route` يشرحان اختيار المسار والاحتياطات والثقة والبدائل، ويعيدان تشغيل التصنيفات الخاطئة للتدقيق.

## التثبيت

### من المصدر (Rust toolchain ≥ 1.75)

```bash
git clone https://github.com/nuoyazhizhou/tokenslim.git
cd tokenslim
cargo build --release
```

يقع الملف التنفيذي في `./target/release/tokenslim` (أو `tokenslim.exe` على Windows).

### الملفات التنفيذية الجاهزة

حمّل من صفحة [Releases](https://github.com/nuoyazhizhou/tokenslim/releases).

### الإعداد (اختياري)

تمر جميع إعدادات وقت التشغيل عبر متغيرات البيئة. انسخ [`.env.example`](./.env.example) إلى `.env` واملأ قيمك المحلية. يتم تجاهل `.env` افتراضيًا في git؛ يتم تتبع قالب المثال فقط.

يحتاج معظم المستخدمين فقط إلى `RUST_LOG=info` (أو `debug` لتتبع مطوّل). متغيرات LLM-Audit (`OPENAI_API_KEY` و`OPENAI_BASE_URL` و`OPENAI_MODEL`) مطلوبة فقط إذا شغّلت `scripts/audit_*.py --llm-audit` — بدونها، تتدهور التدقيقات إلى وضع lint فقط.

### تكاملات المحرر / IDE

- **VS Code** — انظر `vscode-extension/`
- **Chrome** — انظر `chrome-extension/`
- **JetBrains** — انظر `jetbrains-plugin/`

### SDKs

- **Node.js / TypeScript** — `npm i tokenslim-sdk` (المصدر: [`packages/sdk-nodejs/`](./packages/sdk-nodejs/))
- **Python** — انظر [`sdk/python/tokenslim_sdk.py`](./sdk/python/tokenslim_sdk.py) (عميل ملف واحد)
- **Java 11+** — انظر [`sdk/java/TokenSlimClient.java`](./sdk/java/TokenSlimClient.java)

> 📖 [دليل البدء السريع في 5 دقائق](./docs/guides/QUICKSTART.md) · [دليل استخدام SDK الكامل](./docs/guides/SDK_USAGE.md) · [دليل المستخدم](./docs/guides/USER_GUIDE.md)

## الاستخدام

### CLI

```bash
# ضغط سجل بناء
./target/release/tokenslim -i build.log -o output.json --reorder

# تقرير تشخيصي منزوع الضوضاء ملائم للذكاء الاصطناعي
./target/release/tokenslim decompress -i output.json -o ai_report.txt --ai-export

# وضع فقد بإشارة عالية (يحتفظ بنافذة الخطأ + البيانات الوصفية الرئيسية)
./target/release/tokenslim decompress -i output.json -o ai_signal.txt --ai-signal

# التحقق من قاعدة ثابتة (ملف واحد)
./target/release/tokenslim --verify-rule tests/fixtures/static_rule/sample_rule.toml \
  --verify-fixture tests/fixtures/static_rule/sample_fixture.log \
  --verify-expected tests/fixtures/static_rule/sample_expected.txt

# التحقق من قاعدة ثابتة (دفعة، وضع دليل)
./target/release/tokenslim --verify-rule tests/fixtures/static_rule/sample_rule.toml \
  --verify-fixture tests/fixtures/static_rule \
  --verify-expected tests/fixtures/static_rule

# تمهيد المشروع وخطافات الصدفة
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
# يستمع على 127.0.0.1:<port>، يعرض /health و/compress و/decompress
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

## الإضافات

يأتي TokenSlim مع **60+ إضافة** تغطي المدخلات التي تهيمن على حركة LLM الحقيقية. كل إضافة قائمة على البيانات (تكوين JSON / TOML تحت `config/plugins/`) والتوجيه قائم على المسار، لذا فإن إضافة تنسيق مصدر جديد هي في معظم الحالات مجرد تغيير في التكوين.

تصفح السجل الكامل في [`config/plugins/`](./config/plugins/)، أو شغّل:

```bash
./target/release/tokenslim plugins list
./target/release/tokenslim explain-plugin --explain-command "cargo build"
```

## التكاملات

| السطح | المسار | الحالة |
|---|---|---|
| CLI | `src/bin/tokenslim-server.rs`, `src/cli/` | Stable |
| REST Server | `src/bin/tokenslim-server.rs` | Stable |
| VS Code | `vscode-extension/` | Stable |
| Chrome | `chrome-extension/` | Stable |
| JetBrains | `jetbrains-plugin/` | Stable |
| Python SDK | `crates/tokenslim-py/` | Stable |
| Node.js SDK | `packages/sdk-nodejs/` (npm: `tokenslim-sdk@0.1.0`) | Stable |
| Java SDK | `sdk/java/` | Stable |

## البنية المعمارية

يتبع TokenSlim خط أنابيب متعدد الطبقات:

1. **موجّه المسار (Route dispatcher)** — يختار الإضافة/الإضافات حسب توقيع الأمر / المحتوى.
2. **سلسلة الإضافات** — كل إضافة تمتلك الاستخراج والطي والاستبدال الدلالي.
3. **نواة الضغط** — استخراج المسار بـ radix-trie، طبقات القاموس، إزالة التكرار العالمية.
4. **إعادة الترطيب (Rehydration)** — آمن round-trip، يمكن استرداد المدخل الأصلي بالكامل من الشكل المضغوط.
5. **AI Export / Signal** — معالجة لاحقة مدركة للسياق لاستهلاك LLM.

راجع `docs/development/ARCHITECTURE.md` للتصميم الكامل.

## المساهمة

نرحب بالمساهمات. يرجى فتح issue أولاً لمناقشة التغييرات الكبيرة؛ يمكن إرسال التصحيحات الصغيرة وإعدادات الإضافات الجديدة مباشرة كـ PR.

```bash
# تشغيل الاختبارات
cargo test

# التشغيل مع عينة
./target/release/tokenslim -i samples/web_log_plugin/case_001_access.log -o out.json --reorder
```

## الترخيص

[MIT](./LICENSE)

</div>
