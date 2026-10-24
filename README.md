# Kalam (كلام)

**A statically typed language for full-stack web and SaaS, written in English, Arabic or Egyptian Franco.**

> **Status: design stage.** The v0.1 specification is written. **There is no compiler yet.** The commands below show the planned toolchain. See [`ROADMAP.md`](ROADMAP.md) for the plan to 1.0.

Kalam is for professional developers, MENA first. It compiles to JavaScript first (C/WASM later), uses indentation for blocks, infers types, and is secure by default:

- **SQL injection and XSS are type errors.** `sql"…"` and `html"…"` are checked tagged literals, not strings.
- **Routes are deny-by-default.** Every route must state who can access it.
- **I/O needs capabilities.** Code can touch the network or the disk only when it is handed the right value.
- **No `null`, no exceptions, no `async`.** Kalam uses `Option`, `Result` with `?`, and `panic` for bugs. Suspension is inferred.

Every file is written in one **skin**: English (`.kl`), Arabic (`.ar.kl`) or Egyptian Franco (`.franco.kl`). All three parse to the same canonical form, so `kalam convert` translates between them losslessly.

## كلام

<div dir="rtl">

**لغة برمجة ذات أنواع ثابتة لبناء تطبيقات الويب والبرمجيات الخدمية (SaaS) بالكامل، تكتبها بالإنجليزية أو بالعربية أو بالفرانكو المصري.**

> **الحالة: مرحلة التصميم.** المواصفة v0.1 مكتوبة، ولا يوجد مترجم بعد. الأوامر أدناه تصف الأدوات المخطط لها. راجع خطة الطريق في ملف ROADMAP.md.

كلام موجهة للمطورين المحترفين، والبداية من منطقة الشرق الأوسط وشمال أفريقيا. تُترجَم أولًا إلى جافاسكربت، وتستخدم المسافات البادئة لتحديد الكتل، وتستنتج الأنواع تلقائيًا، وهي آمنة افتراضيًا:

- **حقن SQL وهجمات XSS أخطاء أنواع** يكتشفها المترجم قبل التشغيل.
- **كل مسار في الخادم مغلق افتراضيًا** حتى تحدد من يحق له الوصول.
- **لا وصول للشبكة أو الملفات** إلا بصلاحية صريحة.
- **لا `null` ولا استثناءات ولا `async`.**

كل ملف يُكتب بـ**واجهة** واحدة: الإنجليزية أو العربية أو الفرانكو. الثلاث تتحول إلى الشكل نفسه، فيمكن التحويل بينها دون أي فقد.

</div>

## Hello, world ×3

English (`hello.kl`):

```kalam
fn main():
    print("Hello, world!")
```

العربية (`hello.ar.kl`):

```kalam
دالة رئيسي():
    اطبع("Hello, world!")
```

Franco (`hello.franco.kl`):

```kalam
dala ra2isi():
    ektb("Hello, world!")
```

All three files have the same canonical form (KCF hash). String text is never translated, so the output is identical. GitHub and most Markdown viewers do not apply Kalam's per-token bidi isolation, so the Arabic block may look reordered here. Kalam's own tools display it correctly.

## Planned toolchain

One Rust binary, `kalam` (not built yet):

```sh
kalam new myapp                  # new project, with AGENTS.md for AI agents
kalam run                        # build to JS and run
kalam fmt                        # the one canonical format
kalam convert --to ar hello.kl   # lossless skin conversion
kalam check --json               # diagnostics for editors and agents
kalam explain K0601              # long explanation, in en / ar / franco
```

## Repository

| Path | Contents |
|---|---|
| [`DECISIONS.md`](DECISIONS.md) | Locked founder decisions |
| [`spec/KALAM-SPEC.md`](spec/KALAM-SPEC.md) | The language specification, v0.1 |
| [`spec/keywords.json`](spec/keywords.json) | Keyword and core-name table for the three skins |
| [`spec/llms.txt`](spec/llms.txt) | Compact guide for LLMs writing Kalam |
| [`ROADMAP.md`](ROADMAP.md) | Milestones M0–M10, Beta and 1.0 |
| [`spec/CRITIQUE.md`](spec/CRITIQUE.md) | Completeness review of the spec and plan, with open issues |
| [`compiler/`](compiler/) | Rust workspace scaffold and [`ARCHITECTURE.md`](compiler/ARCHITECTURE.md) (not yet compiled) |
| [`eval/`](eval/) | Scorecard, build-loop procedure and test corpus |
| [`docs/DOCS-PLAN.md`](docs/DOCS-PLAN.md) | Documentation site plan |
| [`research/`](research/) | Research reports 01–06 behind the design |

## License

Open-source core under MIT or Apache-2.0 (decision D9). The license files will be added before the first code is published.
