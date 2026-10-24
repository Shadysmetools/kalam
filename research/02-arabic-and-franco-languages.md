# 02 — Arabic-Script and Franco/Arabizi Programming Languages: Survey, Failure Analysis, and Design Recommendations

> Research date: 2026-10-24. GitHub star counts and last-push dates were pulled live from the GitHub API on that date. Claims marked **(vendor claim)** or **(unverified)** were not independently confirmed. Every row gives the evidence used.

## 0. Executive summary

- **More than 40 Arabic-script or Arabic-dialect languages exist. None became a production language.** The most-starred Arabic language on GitHub is Ramsey Nasser's **Qalb/قلب** (800 stars), and it was explicitly an *art piece*. The next is **Noor** (144 stars, dormant since 2018). Every other project is under 80 stars. Most stopped after 1–3 years at "hello world + loops + functions", with no package manager, no FFI, and no editor support.
- **Franco/Arabizi languages are almost all jokes or toys from 2016 or later.** Examples: Egyptian **Franco** (2016, flex/bison), **Ta3mya++** (2021), **JulieLang** (2026, self-described "shitpost transpiler"), Jordanian **NashmiC** (2026, the most complete one), Moroccan **LFI3A** and **DarijaScript**, and Tunisian **Naftah**. **No serious Egyptian Franco language exists.** That niche is open.
- **The pattern that works elsewhere** is one semantic core, several *keyword skins*, and borrowing a large ecosystem:
  - **Hedy** accepts keywords in 47+ languages. English is always valid alongside them, and both Western and Arabic-Indic digits work.
  - **1C:Enterprise** accepts both Russian and English keywords and has a large captive business ecosystem.
  - **Excel** stores formulas in a canonical form and displays localized function names.
  - Transpile-to-Python projects such as Ya, lughat-althuban and JugaadLang get "you can build anything Python can" for free.
- **The technical problems are real but solvable:**
  - bidi reordering of neutral tokens
  - Trojan-Source bidi control characters
  - Arabic-Indic vs Western digits
  - tatweel, hamza and alef variants creating "different" identifiers that look the same (verified in CPython below)
  - **Franco tokens starting with a digit** (`3adi`, `7awel`), which break ordinary lexers. NashmiC registers `3adi` as a keyword, but its lexer sends any leading digit to the number scanner, so by reading the source the keyword looks unreachable.
- **Recommendation:**
  - Use one AST and semantics with three skins (English / فصحى / Egyptian Franco).
  - Round-trip losslessly through a CST that keeps comments and whitespace.
  - Use a canonical Franco spelling that **never starts with a digit**, plus a closed alias table.
  - Accept English keywords in every skin.
  - Fold Arabic orthographic variants for keywords and identifiers.
  - Ban bidi controls.
  - Compile to a large existing ecosystem (JS/TS first, as `03-implementation-strategy.md` recommends) instead of building a standard library from scratch.

---

## 1. Arabic-script programming languages: catalogue

Legend: **Status** = Active (push in the last 6 months) / Dormant (no push in more than 2 years) / Dead (site gone or never released). Stars are from GitHub on 2026-10-24.

### 1.1 Historical and well-known projects

| Language | Creator / Year | Implementation | Status (evidence) | Stars / users | Real products? | Why it stalled |
|---|---|---|---|---|---|---|
| **Sakhr Arabic BASIC** | Al Alamiah / Sakhr (Kuwait), 1987 | MSX-BASIC firmware extension on Yamaha-made MSX machines | Dead (hardware line ended) | Shipped on every Sakhr MSX sold across the Arab world | Hobby programs and games on MSX | Tied to one hardware platform. Died with MSX. [msx.org AX-350](https://msx.org/wiki/Sakhr_AX-350) |
| **قلب Qalb ('alb)** | Ramsey Nasser (Lebanese-American), 2012 | Scheme-like. Interpreter in JS based on Norvig's *Lispy*, PEG.js parser, browser REPL | Dormant: last push 2024-03, art project | **800★ / 84 forks**, the most-starred Arabic language | No. Conceptual art about English bias in computing | By design an art piece. Nasser chose Lisp because it needs almost no Latin punctuation. GitHub and editors could not render the code properly. [repo](https://github.com/nasser/---), [Wikipedia](https://en.wikipedia.org/wiki/Qalb_(programming_language)), [Engadget 2014](https://www.engadget.com/2014-05-07-ramsey-nasser-arabic-programming-language.html), [The Register](https://www.theregister.com/2013/01/25/arabic_programming_language) |
| **ألف Alif** | Idea by **حسن دراكه (Hassan Drakeh)** in 2010. v1 2018, v2 2019, v3 2020 (abandoned 2022). v5 rewrite since 2022 by Mohammed & Abdulrahman Al-Khatib | v1–v3 were educational and shipped as a desktop app on Flathub; their internal design is **(unverified)**. v5 is an interpreter written from scratch in C++ with a Python-like grammar (README) | **Active** (push 2026-09) | alifcommunity/Alif **74★**, older albari54/alif 32★. Telegram community | Educational only so far. v5 states it aims to "compete with world languages" | Each major version was a full rewrite and incompatible with the previous one. There is no ecosystem, so every library is reimplemented. Flathub build is on an EOL runtime. [repo README](https://github.com/alifcommunity/Alif), [aliflang.org](https://aliflang.org), [Flathub](https://flathub.org/zh-Hant/apps/org.aliflang.lang) |
| **كلمات Kalimat** | Mohamed Samy (Egypt), ~2010–2012 | C++ with its own bytecode VM (`smallvm/` in the mirror), IDE, 2D sprites and a physics engine | Dormant/dead. Mirror last pushed 2021 | mobadarah/kalimat-lang 14★, baronleonardo/kalimat 2★ | Kids' games and graphics | Aimed at schools but never got curriculum adoption. Single maintainer. [ArabNet 2012](https://arabnet.me/english/editorials/business/industry/kalimat-first-arabic-programming-language-for-kids), [mirror](https://github.com/mobadarah/kalimat-lang) |
| **عمّوريا Ammoria (Arab)** | SourceForge project, mid-2000s | OOP language with Arabic keywords, LGPLv2. Planned Urdu and Farsi support | Dead | SourceForge only | No | Educational, single team, no ecosystem. [SourceForge](https://sourceforge.net/p/ammoria/), [Arabic page](https://ammoria.sourceforge.net/ar/ar_index.html) |
| **جيم Jeem** | Mohammad Ammar Alsalka, 2006 | Procedural C/Pascal hybrid with simple graphics | **Dead**: the jeemlang.com reference in Wikipedia now points to a "NameBright – Coming Soon" parked page | — | Educational | Domain lapsed, no open source. [Wikipedia list](https://en.wikipedia.org/wiki/Non-English-based_programming_languages) |
| **الخوارزم Al-Khawarizm** | First designed in 2008 | Middle-level systems language, own IDE, Windows and Linux. 2026 repo adds an Arabic assembler | Semi-active (assembler repo 2026) | examples repo 8★ | Claimed professional use **(unverified)** | Closed and IDE-centric, little open tooling. [alkhawarizm.org](https://alkhawarizm.org/), [GitHub](https://github.com/alkhawarizm/examples) |
| **إبداع Ebda3** | Wael Hassan (Aswan University, Egypt), ~2013–2014 | C++/Python-inspired. Powers the "Abtaker" visual tool | Dead | — | Kids' tooling | Crowdfunding-era project. **Notable design detail:** accepts both MSA and Egyptian spellings of إلى/الى, accepts the Arabic comma, and treats hamza strictly (Swidan & Hermans 2023). [Wamda 2014](https://www.wamda.com/en/2014/03/first-arabic-programming-language-for-children-and-beginners-launches-crowdfunding-campaign) |
| **ARLOGO** | — | Arabic UCB Logo | Dead | — | Turtle graphics teaching | Logo itself faded. [Wikipedia list](https://en.wikipedia.org/wiki/Non-English-based_programming_languages) |
| **Loughaty / MyProLang (لغتي)** | Youssef Bassil & Aziz Barbar, ~2009–2012 | GUI plus Arabic "natural-language" syntax resembling C++, proprietary | Dead | Paper only | No | Academic artefact. [survey](https://www.academia.edu/39953345/Arabic_Programming_Language_Evolution_A_Survey) |
| **العنقاء Phoenix** | Youssef Bassil, 2019 | Compiled OOP language with C#-like syntax: preprocessor, scanner, parser, semantic analysis, code generation, linker | Dead (paper only). Inheritance, file I/O, GUI and networking listed as "future work" | — | No | Never released publicly. [arXiv 1907.05871](https://arxiv.org/pdf/1907.05871) |
| **نور Noor** | SimplyAhmazing (Ahmed Abdalla), 2015 | JavaScript with PEG.js, browser IDE. Uses Arabic-Indic digits and colloquial keywords (`لو … سوي … بس`, `امر عامل`) | **Dormant** (last push 2018-11) | **144★** | No. Booleans, arrays and maps were never finished | Side project. The PEG.js tooling gave no line numbers in errors (README). [repo](https://github.com/SimplyAhmazing/noor) |
| **ي Ya** | yalang, 2018 | **Transpiles Arabic to Python** and runs it. `؛` line terminator, Arabic digits. IntelliJ plugin | Dormant (last push 2019-01) | 44★ | *In principle* anything Python can do, because it runs real Python | README tells users to use IntelliJ because other editors could not handle RTL. Library names stayed English. Abandoned. [repo](https://github.com/yalang/ya) |
| **Seen (س)** | abukhadra, 2018 to present | Bilingual Arabic/English. **Transpiles to readable Rust** (plus HTML/CSS), bilingual editor, Arabic↔English translation tools | Active-ish (push 2026-08), "early development" | 20★ | Not yet | The bilingual-plus-big-host-ecosystem idea is right. It is a single-maintainer project and has only been tested on one Mac M1 setup. [README](https://github.com/abukhadra/seen/blob/main/README_en.md) |
| **APL (LLM "compiler")** | Sibaee, Najar, Ghouti, Koubaa (Prince Sultan Univ.), 2024 | GPT-4 translates Arabic code to Python, which is then executed | Research prototype | — | No | **Anti-pattern.** The "compiler" is non-deterministic, paid, and cannot be audited. [arXiv 2403.16087](https://arxiv.org/html/2403.16087v1) |
| **Hedy (Arabic mode)** | Felienne Hermans (Leiden). Arabic work with Alaaeddin Swidan, 2022 to present | Gradual teaching language with 18 levels. Python-based Lark grammars, **transpiles to Python** | **Active** (push 2026-10-24) | **1,694★**. Used in schools in many countries | Teaching only, by design | Not stalled. It is the reference implementation for localized keywords (§4.1). [repo](https://github.com/hedyorg/hedy), [Leiden 2022](https://www.universiteitleiden.nl/en/news/2022/02/more-than-a-billion-people-can-now-learn-to-program-with-hedy) |

### 1.2 The 2020s wave (GitHub, all small)

| Language | Creator / Year | Implementation | Status | Stars | Notes |
|---|---|---|---|---|---|
| **ض Daad** | daadLang, 2026 | Go tree-walking interpreter, Python-style, OOP | Active | 34★ | **Folds letter variants (أ، ا، ء، آ، إ)**, Arabic error messages. [repo](https://github.com/daadLang/daad) |
| **باء Baa** | Omar Aglan, 2023 to present | Native systems compiler in C (own IR/SSA, PE/COFF and ELF output), Arabic digits and punctuation, LSP-style JSON contracts | Active (v0.6.0) | 8★ | The most serious *native* Arabic compiler found. Has no ecosystem. [repo](https://github.com/OmarAglan/Baa) |
| **لغة العرب Alarab_lang** | Saif Hussein, 2026 | C | Active | 46★ | Explicit goal is an Arab tech stack "independent of other languages", which repeats the isolation mistake. [repo](https://github.com/Alarab-Developers/Alarab_lang) |
| **لغة الثعبان lughat-althuban** | GalaxyRuler, 2026 | Arabic-first **Python dialect**: Arabic keywords, **stdlib/library aliases**, **translated tracebacks** | Active | 6★ | The right architecture (§5), small reach. [repo](https://github.com/GalaxyRuler/lughat-althuban) |
| **ABJAD** | Mohammed Ezzedine, 2020 | C# interpreter | Dormant 2022 | 12★ | [repo](https://github.com/mohammed-ezzedine/ABJAD) |
| **Fairuz, Knoz, Ramz, Suda, ص sadlang, Arabi, ArabicCompiler, Firnas, nabdjs, 3EmaScript …** | various, 2020–2026 | Rust, LLVM, Python, JS | Mostly 0–31★ | — | University compiler-course projects and weekend projects. ArabicCompiler (tareq-alomari) 31★. [GitHub search](https://github.com/search?q=%D9%84%D8%BA%D8%A9+%D8%A8%D8%B1%D9%85%D8%AC%D8%A9&type=repositories&s=stars) |
| **Sema / Sefa** | Emad Elsaid (Egypt), 2012 | Arabic CSS and XHTML alternatives | Dormant | 27★ / 8★ | DSLs, not general-purpose. [Sema](https://github.com/emad-elsaid/Sema-lang) |
| **Mujeed** | Blidi Wajdi, 2017 | Arabic Prolog IDE (Java) | Dormant | 6★ | [repo](https://github.com/BlidiWajdi/Mujeed-Arabic-Prolog) |
| **Arabic Lisp (Qalb remix)** | LiaTemplates, 2026 | JS | New | 0★ | [repo](https://github.com/LiaTemplates/qalb) |

**Arabic Scratch:** Scratch translates block text into many languages, Arabic included, and allows Unicode variable names. Translations do not cover variable names. According to Swidan & Hermans, **Scratch silently fails to run with non-English numerals**. There is no separate "Arabic Scratch" language. ([Wikipedia](https://en.wikipedia.org/wiki/Non-English-based_programming_languages), [Hedy localization paper](https://www.hedy.org/research/A_Framework_for_the_Localization_of_Programming_Languages_2023.pdf))

**Names requested but not found:** I searched for **Zamzam, Sukkar/سكر, Aleph (as distinct from Alif), and "Lughati" as a programming language** with queries such as `"zamzam" OR "sukkar" OR "سكر" arabic programming language github`, `Zamzam OR Ammoria OR "Jeem" Arabic programming language`, and GitHub `Lughati` / `zamzam language`. None found a programming language. The closest real matches:
- "Lughati" is **Loughaty/MyProLang**.
- "Aleph" is **Alif**, or the 2018 educational **Alef** mentioned in the [survey](https://www.academia.edu/39953345/Arabic_Programming_Language_Evolution_A_Survey).
- "Yaa" is **Ya (ي)**.
- "ar-py / Arabic Python" covers **Ya**, **lughat-althuban**, `pyarab`, SimplyAhmazing/python-with-arabic-keywords (1★), and the Libyan paper *Design and Implementation of Arabic Python Programming Language Interface* ([IJEIT](https://ijeit.misuratau.edu.ly/index.php/ijeit/article/view/514)).

---

## 2. Franco / Arabizi and dialect languages

| Language | Dialect / Script | Year | Implementation | Stars / Status | Keyword samples | Assessment |
|---|---|---|---|---|---|---|
| **Franco** (AhmedKamal1432) | **Egyptian, Latin script** | 2016 | flex/bison, C-like | 0★, one commit | `lma`, `d3bs` … | University compiler course. Variables limited to one character. [repo](https://github.com/AhmedKamal1432/Franco) |
| **Ta3mya++** 🧆 | **Egyptian Franco** | 2021 | lex/yacc, own VM ("TVM"), Python GUI | 3★, dormant 2021 | `sa7e7` (int), `7a2i2i` (float), `thabet`, `lw`, `basy` (return), `tarqeem` (enum), `bedayah` (main) | Joke-flavoured course project. Shows that **digit-leading words like `7a2i2i` appear naturally in Franco**. [repo](https://github.com/Abdulrahman-Khalid/ta3mya-plus-plus), [sample](https://github.com/Abdulrahman-Khalid/ta3mya-plus-plus/blob/master/docs/methal.ta) |
| **JulieLang** | **Egyptian Franco** | 2026 (v1.0.0, single release) | Python transpiler (`pip install julielang`) | PyPI only. Homepage is a placeholder `yourname/julielang` | `e3mly_keda x betsawy 5 yaa`, `lw` / `lw_8er_keda` / `ma_5las_ba2a`, `tol_ma` (**alias `tool_ma` accepted**), `etb3`, `sa7`/`mynf34`, `fakes`, `de_4o8lana_de` | Self-described "shitpost transpiler". Useful as a corpus of how Egyptians actually spell: `lw`, `etb3`, `8er`, `5las`, `4o8l`. [PyPI JSON](https://pypi.org/pypi/julielang/json) |
| **YaMasrBet3melihaEzay** | Egyptian | 2026 | C++, brainfuck-like | 4★ | — | Esolang joke. [repo](https://github.com/v3lk777-collab/YaMasrBet3melihaEzay-Programming-Language) |
| **البلولوخ Blouloukh** | Egyptian slang, **Arabic script** | 2026 | Python, then C# recursive-descent parser | 1★ | `ياض`, `هاتلي` | Fun project. [repo](https://github.com/amrkh24/Blouloukh-Lang) |
| **arabic-compiler** (Egyptian dialect) | Egyptian, Arabic script | 2026 | Python lexer, parser and semantic analysis, generates C++ | 0★ | — | Course project. [repo](https://github.com/amaneysalheen68-dot/arabic-compiler) |
| **Kemet** | — | 2022 | Python BASIC interpreter | 2★ | `VAR`, `PRINT` | **Not Franco.** Egyptian name, English keywords. [repo](https://github.com/BemwaMalak/Kemet) |
| **NashmiC** 🇯🇴 | **Jordanian Franco** | 2026 | C compiler ("mansaf") → C transpiler, sema/type-checking, formatter, REPL, **VS Code extension**, **tree-sitter grammar**, AUR/deb/Homebrew packages, stdlib (math, strings, files) | 1★ (pushed 2 days after creation) | `iza`/`wala`/`wala_iza`, `tool_ma`, `lakol x fi`, `khalas`, `kammel`, `rajje3`, `khalli`, `thabet`, `dalle`, `haikal`, `ah`/`la`, `itba3`, `i2ra`, `3adi` | The most complete Franco language found, with good tooling breadth. **Lexer issue:** the keyword table registers `3adi` (switch default), `3al_3amyani`, `7utt`, but `lexer.c` sends every leading ASCII digit to `lex_number()`, so by reading the source `3adi` lexes as `INT(3) IDENT(adi)`. [repo](https://github.com/Ziadstr/nashmic), [keywords.c](https://github.com/Ziadstr/nashmic/blob/main/compiler/src/keywords.c) |
| **LFI3A** | Moroccan Darija, Latin | 2025 | C++ tree-walking interpreter | **56★** (the most-starred Franco-style language) | `dir` (let), `kteb` (print), `ila`, `wla` (or), `w` (and), `ma7ad` (while), `kol`, `dalla`, `rje3`, `s7i7`/`ghalat` | Beginner toy, dormant since 2026-01. [repo](https://github.com/Oussama-jabrane/lfi3a) |
| **DarijaScript** | Moroccan Darija, Latin | 2024 to present | Go interpreter, WASM playground, VS Code extension with diagnostics and formatter | 8★, active | `9ayed`, `golih` | Good tooling, toy scope. [repo](https://github.com/rabraghib/DarijaScript) |
| **Naftah نفطه** | Tunisian + MSA, Arabic script | 2024 to present | **Java/JVM, ANTLR** | 11★, active | Dialect-aware keywords | JVM host gives potential Java interop. [repo](https://github.com/Chakib-DAII/Naftah) |
| **Havnix** | Sudanese, Arabic script | 2024 | Python | 5★ | — | [repo](https://github.com/havnix/Havnix-Language) |
| **Garga** | Kuwaiti | 2019 | Swift | 7★, dead | — | Built for one BarCamp talk. [repo](https://github.com/hashemi/garga) |

**Observations:**

1. **Egyptian Franco has no serious language.** Every Egyptian project is a course assignment or a joke. That gap is real.
2. **Projects converge on the same words without coordinating:**
   - `tool_ma` appears in Jordanian NashmiC and Egyptian JulieLang.
   - `rje3`/`rajje3`/`basy` for return.
   - `sa7`/`s7i7`/`sa7e7` for true and int.
   - `thabet` for const.
   - `itba3`/`etb3`/`kteb` for print.

   This is a natural canonical vocabulary to build on.
3. **Spelling varies across every project:**
   - `lw`/`law`/`iza`/`ila` for *if*
   - `etb3`/`itba3`/`kteb`/`ektb` for *print*
   - `8`/`gh` for غ
   - `5`/`kh` for خ
   - `4`/`sh` for ش (JulieLang's `de_4o8lana_de`)

   A design without an alias strategy will frustrate users.
4. **Franco orthography is not standardized.** The documented *compliant-franco* guideline for Egyptian Franco exists, but it is an archived draft ([Codeberg/zff.dev](https://zff.dev/saadnpq/compliant-franco)). Perl's `Encode::Arabic::Franco` says "Franco-Arabic is not standardized" ([MetaCPAN](https://www.metacpan.org/pod/Encode::Arabic::Franco)). In Egyptian Arabizi, `2` stands for both hamza **and** the glottal-stop qāf ([Wikipedia: Arabizi](https://en.wikipedia.org/wiki/Arabic_chat_alphabet)).

---

## 3. Non-Arabic localized languages (comparison set)

| Language | Locale | Year / Impl | Scale | Lesson |
|---|---|---|---|---|
| **Wenyan 文言** | Classical Chinese | 2019. TypeScript, compiles to JS and Python | **20,283★**, but **last push 2023-10** | The most-starred localized language found on GitHub, and still dormant. Stars measure novelty, not adoption. Fully localized grammar and numerals, but **English error messages** (Swidan & Hermans), which hurts the very learners it targets. [repo](https://github.com/wenyan-lang/wenyan) |
| **易语言 Easy Programming Language (EPL)** | Simplified Chinese | ~2000–2004. Proprietary RAD IDE, Windows/Win32 | Widely reported as one of the largest non-English user bases, alongside 1C **(no audited numbers for either)** | It was popular because it was a complete RAD environment for Windows desktop apps. Its reputation was hurt by heavy use for game cheats and malware, which led to **antivirus false positives on any EPL-compiled binary**. A ransomware once infected its core static libraries (supply-chain attack). Closed, Windows-only, isolated. [Wikipedia](https://en.wikipedia.org/wiki/Easy_Programming_Language), [secrss incident report](https://www.secrss.com/articles/6873), [V2EX discussion](https://www.v2ex.com/t/560518) |
| **1C:Enterprise** | Russian (**and English keywords**) | 1990s to present. Business platform | "300,000+ engineers", "1,000+ solutions" **(vendor claim)** | **The strongest commercial success of a non-English language.** It worked because it was captive inside a platform people *had* to use (accounting/ERP), not because of the keywords. Accepting both languages' keywords is the precedent for "skins". [Wikipedia](https://en.wikipedia.org/wiki/1C:Enterprise), [1C-DN](https://1c-dn.com/1c_enterprise/1c_enterprise_script/) |
| **Nadeshiko なでしこ** | Japanese | 2004 to present. v3 in JS | Included in a Japanese junior-high technology textbook | Survived 20 years by targeting office automation, a real job to do. [Mynavi](https://news.mynavi.jp/article/nadeshiko-1/), [docs.rs](https://docs.rs/nadesiko3) |
| **Excel formulas** | 40+ locales | — | Billions of users | **Canonical storage with localized display.** Localized function names and decimal separators, and non-English numerals work. You cannot mix locales in one sheet (Swidan & Hermans §4.7). This is projectional skinning at scale. |
| **Hindi:** JugaadLang, Bharat, Bhailang, OM Lang | Romanized Hindi / Devanagari | 2020s | Small. Bhailang went viral as a joke | **JugaadLang** (romanized Hindi keywords → **Python AST**) is the closest analogue to a Franco skin over a big ecosystem. [PyPI](https://pypi.org/project/jugaadlang/1.1.2/) |
| **Bengali:** ChaScript, Kalpana | Bangla | 2020s | Small | ECMAScript-grammar reuse. [Wikipedia](https://en.wikipedia.org/wiki/Non-English-based_programming_languages) |
| **Persian:** Zangar, Salam, Kharazmi, Farsinet, Kati | Farsi | 2010s–2026 | Salam 64★ (active, **English and Persian**). Zangar = Rust with Persian keywords (fork of French *Rouille*) | Keyword-macro skins over Rust are cheap to build and get little use. [Salam](https://github.com/SalamLang/Salam), [Zangar](https://github.com/ui-ce/zangar) |
| **Multilingual frameworks:** Legesher (50+ languages for Python), UniversalPython, Babylscript (multilingual JS), Citrine (111 languages), ALGOL 68 (multi-language reserved words) | — | — | — | Multi-skin over one core is an old, proven idea. What it has lacked is a production ecosystem and tooling. [Wikipedia](https://en.wikipedia.org/wiki/Non-English-based_programming_languages) |

---

## 4. Lessons from the strongest precedent: Hedy's Arabic mode

From Hedy's `highlighting/highlighting-trad.json`, fetched from the repo on 2026-10-24:

- **English is always accepted.** Every Arabic keyword regex is `<arabic>|<english>`. For example, `if` matches `ـ*اـ*ذـ*اـ*|if`. A learner can copy English examples into an Arabic program and it runs. 1C does the same with Russian and English.
- **Tatweel (kashida, U+0640) is allowed between every letter** (`ـ*`), so `اذا` and `اـــذا` are the same keyword. Qalb also allows this. Ebda3 does not.
- **Digits:** `DIGIT = 0123456789 + ٠١٢٣٤٥٦٧٨٩`, so both systems work everywhere.
- **Translation choices:**
  - `print` → `قول` ("say"), not `اطبع`. Qalb also uses `قول`.
  - `if` → `اذا`, `else` → `وإلا`, `elif` → `وإلا اذا`.
  - `for` → `لكل`, `while` → `بينما`, `repeat` → `كرر`, `return` → `ارجع`.
  - `True`/`False` → `صحيح`/`خاطئ`.
  - `answer` and `def` are **left untranslated**.
- **Comma:** `،` (U+060C) is accepted as the Arabic comma.

Swidan & Hermans (SPLASH-E 2023), *A Framework for the Localization of Programming Languages*, lists **12 localization aspects**:
1. alignment with an existing language
2. keywords
3. variable names
4. productions (word order)
5. numerals
6. characters "without meaning" (tatweel)
7. diacritics
8. alternative keywords (gendered Arabic forms, MSA vs Egyptian spellings)
9. localized punctuation
10. RTL support
11. multilingual programming
12. error messages

Key data point: **71% of Arabic-speaking Libyan students named error messages as a major barrier, vs 2% of Australian students.** ([PDF](https://www.hedy.org/research/A_Framework_for_the_Localization_of_Programming_Languages_2023.pdf))

Supporting evidence: Guo's CHI 2018 survey of 840 non-native speakers found barriers in reading docs, writing code, technical communication, and learning English and programming at the same time ([paper](https://pg.ucsd.edu/publications/non-native-english-speakers-learning-programming_CHI-2018.txt)).

---

## 5. Why Arabic and Franco languages historically fail

| # | Failure mode | Evidence |
|---|---|---|
| 1 | **Ecosystem isolation.** Every language reimplements I/O, strings, HTTP, DB and GUI from zero, so none reaches "build a real product". | Alif's 4 incompatible rewrites. Baa ships its own assembler and stdlib. Alarab_lang states "independence from other languages" as a goal. |
| 2 | **No interop / FFI.** Arabic code cannot call npm, PyPI or Maven, so serious developers can't adopt it. | Only Ya, lughat-althuban, Hedy (→Python), Seen (→Rust), NashmiC (→C) and Naftah (JVM) even try. All are tiny. |
| 3 | **Education-only positioning.** Kids' languages compete with Scratch, which is already translated and free. | Kalimat, Ammoria, Jeem, Ebda3, ARLOGO. Hedy succeeds *as* an education language, but it is backed by a university research group. |
| 4 | **Single maintainer, no institution.** | Noor (dead 2018), Ya (2019), Kalimat, Jeem (domain parked), Phoenix (paper only). |
| 5 | **Tooling gap.** No LSP, highlighter, formatter or debugger, and editors break RTL. | Ya tells users to use IntelliJ. Qalb could not display properly on GitHub or in editors. The Hedy paper shows Sublime and Mathematica reversing and disconnecting Arabic letters (§3.10). |
| 6 | **Novelty and joke framing** (especially Franco). | JulieLang ("shitpost"), Ta3mya++ (falafel), YaMasrBet3melihaEzay (brainfuck), Garga (one talk). |
| 7 | **No path to the global industry.** Parents and students see an Arabic-only language as a career dead end. | Swidan & Hermans §5.1 record consistent "just learn English" push-back. Transfer to English languages is unstudied (§5.3). |
| 8 | **Orthographic friction.** Hamza, alef and ى/ي variants produce "syntax error" on code that looks right. | Ebda3 is diacritic-sensitive (missing hamza → syntax error). Daad had to add variant folding. |
| 9 | **Stars ≠ adoption.** Novelty drives stars, then the project goes dormant. | Wenyan 20k★ (dormant 2023), Qalb 800★ (art). |

---

## 6. Technical problems of Arabic and Franco in source code

### 6.1 RTL / bidi mixing (UAX #9)

- Arabic letters have bidi class **AL**. Arabic-Indic digits are **AN**, Western digits **EN**, and `+` is **ES**. Verified with `unicodedata.bidirectional` in CPython 3.12.
- Operators, brackets, spaces and digits are *weak or neutral*, so their display position depends on the surrounding strong characters. UAX #31 §4.1.1 gives the canonical example: `x + תו == 1` *displays* as if comparing `(x + 1)` with `תו`, but it logically compares `(x + תו)` with `1`. The same happens with any Arabic identifier.
  - **UAX #31 remedy:** LRM (U+200E) and RLM (U+200F) are in `Pattern_White_Space`, so tools may insert them between tokens to fix the display without changing meaning. ALM (U+061C) is **not** in that set; it is a bidi control. ([UAX #31](https://www.unicode.org/reports/tr31/)) Unicode's later **UTS #55 "Unicode Source Code Handling"** recommends that editors isolate tokens when rendering, so no characters need to be stored in the file ([UTS #55](https://unicode.org/reports/tr55/)). Our policy (§7.5) follows `04-security-by-design.md` and uses rendering-side isolation instead of stored marks.
- **Bidi_Mirrored brackets:** in an RTL run, `(` is drawn as `)` and so on. That is correct in a pure-RTL line, but it confuses users when an Arabic line is pasted into an LTR context such as a terminal, a diff tool or chat.
- **Paragraph direction matters.** An Arabic-skin file should be displayed with an RTL base direction, and an English or Franco file with LTR. Terminals generally do no bidi or shaping, so tracebacks print reversed or disconnected letters. That is why Franco and English error output must be first-class.

### 6.2 Trojan Source (CVE-2021-42574) and homoglyphs (CVE-2021-42694)

- Bidi override and isolate controls make the code that is *displayed* differ from the code that is *compiled*. The affected characters are U+202A–U+202E (LRE, RLE, PDF, LRO, RLO) and U+2066–U+2069 (LRI, RLI, FSI, PDI). The attack was demonstrated against C, C++, C#, JS, Java, Rust, Go and Python ([trojansource.codes](https://trojansource.codes/), [The Register](https://www.theregister.com/2021/11/01/trojan_source_language_reversal_unicode/)).
- Responses:
  - Rust added **deny-by-default** lints `text_direction_codepoint_in_literal` and `text_direction_codepoint_in_comment` ([rustc docs](https://doc.rust-lang.org/stable/nightly-rustc/rustc_lint/hidden_unicode_codepoints/static.TEXT_DIRECTION_CODEPOINT_IN_LITERAL.html)).
  - GCC added `-Wbidirectional=unpaired|any` (Red Hat RHSA-2021:4586).
  - GitHub shows a warning banner on files containing them ([changelog](https://github.blog/changelog/2021-10-31-warning-about-bidirectional-unicode-text/)).
- **An Arabic-first language is a higher-value target**, because legitimate RTL text normalizes the presence of bidi-sensitive content. Defences must be *stricter* than in English-only languages (§7.5).

### 6.3 Digits

| Range | Name | Use |
|---|---|---|
| U+0030–0039 | ASCII / "Western Arabic" `0-9` | Egypt for prices, phones and code. Default. |
| U+0660–0669 | Arabic-Indic `٠-٩` | Egypt and the Levant in print and schoolbooks |
| U+06F0–06F9 | Extended Arabic-Indic `۰-۹` (Persian/Urdu) | 4, 5 and 6 have *different glyphs*, but ٠/۰, ١/۱ … are visually near-identical, so they are confusable |

CPython: `int('١٢٣') == 123` works, but the source literal `x = ١٢٣` raises `SyntaxError: invalid character '١'` (verified). Noor, Ya, Qalb, Baa and Hedy accept Arabic-Indic digits. Qalb accepts *only* Arabic digits and uses `،` as a decimal separator, which is non-standard. The proper Arabic decimal separator is U+066B `٫`.

### 6.4 Editors, IDEs, terminals, keyboards

- **Editors:** RTL rendering and caret movement in mixed lines is still uneven. Hedy's paper documents reversed and disconnected letters in Sublime and Mathematica. Ya recommends IntelliJ. A web playground and a VS Code extension with proper `unicode-bidi: plaintext` per line are a minimum.
- **Terminals:** most do no shaping or bidi, so compiler output and stack traces in Arabic display broken. This argues for showing diagnostics in the editor and LSP, plus a Franco or English fallback in the terminal.
- **Keyboard switching cost:** the Arabic skin forces switching between Arabic and Latin layouts for every `{ } [ ] = " _` and Latin library name. This is a daily ergonomic tax and **the main practical reason Egyptians already type Franco on phones.** A Franco skin removes the switching completely, which is a real advantage.

### 6.5 Diacritics, tatweel and letter-form normalization

Verified in CPython 3.12 (identifiers are NFKC-normalized per PEP 3131 / UAX #31):

| Input | Python result | Implication |
|---|---|---|
| `كتب` vs `كـتـب` (tatweel U+0640, category Lm) | **Both valid and different identifiers.** NFKC keeps tatweel. | Visually "the same" word gives two variables. |
| `أكتب` / `اكتب` / `إكتب` | **Three different identifiers.** NFKC does not fold hamza forms. | Users type hamza inconsistently. Common bug source. |
| ى (U+0649) vs ي (U+064A), ة vs ه | Not folded by NFKC | Egyptian writing habitually swaps ى/ي and ة/ه. |
| Farsi ی (U+06CC) vs Arabic ي, Farsi ک vs Arabic ك | Not folded | Persian keyboard layouts produce these, which creates homoglyph identifiers. |
| Presentation forms (U+FExx) | **Are** folded by NFKC | NFKC handles one class only. |
| ZWNJ inside an identifier | Rejected by Python | — |

Tashkeel (harakat U+064B–U+0652) are combining marks and therefore *legal* in identifiers, so `كَتَبَ` ≠ `كتب`.

**Conclusion:** Unicode NFKC is necessary but **not sufficient**. The language must define its own **Arabic identifier folding**. Precedents: Nim's style-insensitive identifiers, Daad's variant folding, and Hedy's tatweel regex.

### 6.6 Unicode identifier rules (UAX #31 / UTS #39)

- Use **XID_Start / XID_Continue** with NFKC, as UAX #31 recommends for security.
- Add a **UTS #39 restriction level**: "Highly Restrictive" (single script per identifier, plus Common/Inherited), with confusable detection (`skeleton()`). This blocks Arabic/Farsi homoglyphs and Latin/Cyrillic tricks.
- UAX #31 explicitly allows *profiles*, meaning adding or removing characters. Our Arabic folding is such a profile.

### 6.7 Franco-specific lexing problems

1. **Digit-leading words.** Egyptian Franco puts digits at the start of words: `3adi` (normal), `7aga` (thing), `2ktb` (write), `7awel` (try), `5alas`. Every mainstream lexer (C, Python, JS, and NashmiC) treats a leading digit as a number. Conflicts with numeric syntax include `1e5`, `0x1F`, `2e3`, and unit suffixes such as `3d` or `5s` if they are ever added.
2. **Spelling variance:** `law`/`lw`/`lau`, `ektb`/`2ktb`/`ekteb`/`uktub`, `gh`/`8`, `kh`/`5`, `sh`/`4`/`ch`, plus vowel dropping.
3. **Cross-skin collisions:** short Franco words are ordinary English identifiers: `law` (legal), `la`, `w`, `ma`, `men`, `fi`, `da`. In NashmiC, `la` = false.
4. **Regional variance:** `9` = ص in the Gulf and Levant, `6` = ط in the Gulf, and `8` = غ in Egypt but ق elsewhere. Egyptians mostly write `s`, `t`, `gh`/`8`.

---

## 7. Recommended architecture

### 7.1 One AST and semantics, three skins

```
            ┌──────────── skin: en  (simplest English)   if / print / func
source ───► │  skin lexer ── skin: ar  (فصحى)           إذا / اطبع / دالة
            └──────────── skin: fr  (Egyptian Franco)    law / ektb / dala
                          │
                          ▼
              Concrete Syntax Tree (lossless: keeps trivia, comments, spelling)
                          │
                          ▼
              Canonical AST  ──►  type-check / lint  ──►  backend (host ecosystem)
```

- **Grammar is shared.** Skins change *lexemes only*: keywords, built-in names, punctuation and digit glyphs. Do **not** allow per-skin word order. That would break lossless translation and multiply grammar maintenance. Hedy and 1C both kept one production order.
- **Skin declaration:** a file extension (`.x`, `.xar`, `.xfr`) **or** a first-line pragma (`#skin ar`). The default is detection by script plus the pragma.
- **English keywords are always accepted in every skin** (Hedy, 1C). This helps copy-paste from docs, Stack Overflow and LLM output, and lets learners mix.
- **Contextual keywords:** after `.`, in named arguments, and in record fields, keywords are plain identifiers, so `contract.law` works in every skin.
- **Raw-identifier escape** (like Rust `r#law` / C# `@law`) lets the translator rename or escape any identifier that collides with a keyword in the target skin.

### 7.2 Lossless translation between skins

- Translate on the **CST**, not the AST. Comments, whitespace, blank lines and string contents stay byte-identical.
- **What gets translated:** keywords, built-in functions and literals (`true`, `none`), punctuation aliases (`،`↔`,`, `؛`↔`;`, `؟`), and digit glyphs (optional, per skin preference).
- **What never gets translated:** user identifiers, string literals, comments, and library and module names. An optional **identifier glossary file** (`names.ar.toml`) can map project identifiers for teaching.
- **Round-trip guarantee:** `fmt(A) → B → A` is byte-identical to `fmt(A)`. Aliases are normalized to the canonical spelling by design, and that is the formatter's job.
- **English keywords inside an Arabic or Franco file count as aliases.** They compile as-is, and `fmt` rewrites them to the file's skin canonical (an opt-out flag can keep them, for teaching mixed code). This makes the round-trip guarantee hold.
- **The keyword map must be 1:1 across skins.** Each concept has exactly one canonical word per skin and no word serves two concepts, so translation never needs context. This is why `else` and `default` are separate rows in §8.
- **Excel model for packages:** publish packages in a canonical form (English skin). Editors, LSP and docs render any skin on demand.
- **LLM bonus:** models write excellent English-skin code. A deterministic skin translator makes that code readable in فصحى or Franco without trusting an LLM as a compiler, which is the APL anti-pattern.

### 7.3 Interop with a big ecosystem (the decisive factor)

- Compile to an existing runtime instead of building a standard library from scratch. `03-implementation-strategy.md` recommends **emitting TypeScript/JavaScript first** (npm, browser, Node) with a native C backend later and a Rust-written compiler. The Arabic-language evidence fits that choice. Every Arabic project that reached "could build real things", even in principle, did it by borrowing a host:
  - **Python** hosts: Ya, lughat-althuban, Hedy, JugaadLang.
  - **C** host: NashmiC. **JVM**: Naftah. **Rust**: Seen.
  - Isolated runtimes (Alif, Baa, Kalimat) never got past the standard-library wall.
- **Library APIs stay in English.** Optionally ship **alias packs** (`ektb` → `print`, `مكتبة.طلبات` → `requests`) as *thin, versioned wrappers*, as lughat-althuban does. Never fork the library.
- **Error messages in all three skins**, mapped from host errors (translated tracebacks, as lughat-althuban does). This is the #1 barrier per the 71% Libyan data point.

### 7.4 Arabic normalization profile (keywords **and** identifiers)

Apply in order. The result is the comparison key. The source text is never rewritten except by the formatter.

1. NFKC.
2. Remove tatweel U+0640.
3. Remove harakat U+064B–U+0652, U+0670 (superscript alef), and Quranic marks U+06D6–U+06ED.
4. Fold أ إ آ ٱ → ا. Fold ى → ي. Fold ة → ه. Fold ؤ → و and ئ → ي (keywords only; optional for identifiers).
5. Fold Farsi ی → ي and ک → ك (or reject them under the confusables rule).
6. Fold digits (identifiers only): Arabic-Indic and Persian → ASCII.

**Collision rule:** if two *distinct* declared identifiers fold to the same key (for example `على` and `علي`), raise a **compile error** ("ambiguous after normalization") instead of silently merging them. The formatter rewrites all uses to the spelling at the declaration site.

### 7.5 Bidi and security policy

- **Hard error:** U+202A–202E, U+2066–2069, **ALM U+061C, LRM U+200E and RLM U+200F** anywhere in code, and raw in comments. In strings they are allowed only as escapes (`\u{202E}`). Same for U+200B, U+FEFF and other invisible format characters in identifiers. This is stricter than UAX #31, which permits LRM/RLM as whitespace. It matches requirement U1 in `04-security-by-design.md`, which takes precedence.
- **Display is fixed by tooling, not by stored characters.** `fmt`, the LSP and the playground render each token in a bidi isolate (UTS #55). Diff and review views show the LTR-token-ordered canonical view.
- **UTS #39 "Highly Restrictive" identifiers:** one script per identifier, plus a confusable-skeleton check across the module.
- **Numerals:** accept ASCII, Arabic-Indic and Persian digits, but **reject mixed systems within one literal** (`1٢3`). Decimal separator is `.` or `٫` (U+066B). Never `،`.
- **Diff and review tooling:** `x fmt --show-bidi` renders hidden characters, and CI fails on any bidi warning.

### 7.6 Franco canonical spelling and alias strategy

**Rules for a canonical Franco keyword:**

1. ASCII lowercase `[a-z0-9_]`. **It never starts with a digit.** Digits used as letters (`2`=ء/ق, `3`=ع, `5`=خ, `7`=ح) are allowed *after* the first character.
2. Prefer the **Cairene** pronunciation and the spelling most common in Egyptian texting (`law`, `ektb`, `sa7`). Avoid regional digits `6`, `8` and `9`. Use `gh`, `t`, `s` instead.
3. Use 3–8 characters and avoid collisions with the top ~2,000 English identifiers where possible (the collision list is checked in CI).
4. Use one underscore at most (`tool_ma`).

**Aliases:**

- Each keyword has a **closed, versioned alias list**: vowel variants, `kh`/`5`, `gh`/`8`, `sh`/`4`, and digit-leading forms such as `2ktb` or `7awel`.
- The lexer handles digit-leading aliases this way:
  - If a run of digits is *immediately* followed by a letter and the whole word is in the alias table, emit the keyword.
  - Otherwise, if it is a valid numeric literal (`1e5`, `0x1f`), emit a number.
  - Otherwise, report the error "numbers can't be glued to letters; did you mean …?".
- A build-time check proves that **no alias matches the numeric-literal grammar**. This is the NashmiC lesson.
- The **formatter (`x fmt`) rewrites aliases to the canonical form**, so repositories converge.
- **Fuzzy "did you mean"** (Damerau-Levenshtein ≤ 2 after Franco folding `5→kh, 7→h, 3→a, 2→a, 8→gh, 4→sh`, vowel-stripping) is used *only* in diagnostics. It never silently accepts input.
- **User identifiers may not start with a digit** in any skin. `3arabeya` must be written `_3arabeya` or `arabeya`. This keeps the lexer decidable and identifiers portable to the host language.

---

## 8. Draft keyword table (35 core keywords)

Canonical spelling is listed first. Aliases are what the lexer also accepts, and the formatter rewrites them to the canonical form. English is accepted in every skin. The Arabic column shows the form after the §7.4 folding; hamza is optional on input.

> **The English column is a placeholder.** `01-english-like-languages.md` proposes its own minimal English keyword set (`type`, `has`, `make`, `each`, `in`, `return`, `when`, `is`, `on`, `if`, `reply`, `none`). Where the two differ, 01 decides the English words. The Arabic and Franco columns should then be re-mapped concept by concept, keeping the 1:1 rule from §7.2.

| # | Concept | English skin | Arabic skin (فصحى), canonical / aliases | Franco skin (Egyptian), canonical / aliases |
|---|---|---|---|---|
| 1 | if | `if` | `إذا` / `اذا`, `لو` | `law` / `lw`, `lau`, `low` |
| 2 | else | `else` | `وإلا` / `والا`, `غير_ذلك` | `walla` / `wala`, `wla`, `8er_keda`, `gher_keda` |
| 3 | else if | `else if` | `وإلا إذا` | `walla law` |
| 4 | while | `while` | `طالما` / `بينما` | `tool_ma` / `tol_ma`, `tolma`, `toul_ma` |
| 5 | for each | `for` | `لكل` | `lekol` / `lkol`, `le_kol`, `likol` |
| 6 | in | `in` | `في` | `fi` / `fe`, `fy` |
| 7 | repeat N times | `repeat` | `كرر` | `karrar` / `karar`, `krr` |
| 8 | break | `break` | `اخرج` / `توقف` | `kfaya` / `kefaya`, `khalas`, `5alas` |
| 9 | continue | `continue` | `تابع` / `استمر` | `kammel` / `kamel`, `kml` |
| 10 | return | `return` | `أرجع` / `ارجع` | `rag3` / `ragga3`, `raga3`, `rg3` |
| 11 | function | `func` | `دالة` / `داله`, `وظيفة` | `dala` / `dalla`, `dalah` |
| 12 | variable (let) | `let` | `ليكن` / `متغير` | `khali` / `khalli`, `5ali`, `5alli` |
| 13 | constant | `const` | `ثابت` | `sabet` / `thabet`, `sabit` |
| 14 | true | `true` | `صح` / `صحيح` | `sa7` / `sah`, `sa7e7` |
| 15 | false | `false` | `خطأ` / `خطا`, `غلط` | `ghalat` / `8alat`, `ghalt` |
| 16 | none / null | `none` | `عدم` / `لا_شيء` | `mafeesh` / `mafish`, `mfesh` |
| 17 | and | `and` | `و` | `we` / `wi`, `wa` (`w` rejected: too collision-prone) |
| 18 | or | `or` | `أو` / `او` | `aw` / `ao` |
| 19 | not | `not` | `ليس` / `لا` | `mesh` / `msh`, `mish` |
| 20 | print | `print` | `اطبع` / `قول` (Hedy, Qalb) | `ektb` / `2ktb`, `ekteb`, `etb3`, `itba3` |
| 21 | input / read | `input` | `اقرأ` / `اقرا`, `ادخل` | `e2ra` / `2ra`, `i2ra` |
| 22 | class / type | `type` | `نوع` / `صنف` | `no3` / `noo3`, `naw3` |
| 23 | self | `self` | `ذات` / `نفسه` | `nafso` / `nafs`, `nafsu` |
| 24 | import | `use` | `استورد` / `استخدم` | `hat` / `hatly`, `hatli` |
| 25 | from | `from` | `من` | `men` / `mn`, `min` |
| 26 | as | `as` | `باسم` / `كـ` | `besm` / `bism`, `b_esm` |
| 27 | export / public | `export` | `صدّر` / `صدر` | `talla3` / `tala3`, `tl3` |
| 28 | try | `try` | `حاول` | `hawel` / `7awel`, `7awl` |
| 29 | catch | `catch` | `التقط` / `امسك` | `emsek` / `emsk`, `imsik`, `msk` |
| 30 | finally | `finally` | `أخيرا` / `في_النهاية` | `fel_akher` / `fel_25er`, `felakher` |
| 31 | throw / raise | `throw` | `ارم` / `ارمِ` | `ermi` / `ermy`, `2rmy`, `irmi` |
| 32 | match / switch | `match` | `طابق` / `حسب` | `hasab` / `7asab` |
| 33 | case | `case` | `حالة` / `حاله` | `hala` / `7ala` |
| 34 | default (in match) | `default` | `افتراضي` / `أي_شيء` | `ay_haga` / `ay_7aga`, `ay7aga`, `3adi` (digit-leading alias, closed list) |
| 35 | await | `await` | `انتظر` | `estanna` / `istanna`, `2stanna` |

**Punctuation aliases (Arabic skin):** `،`=`,`, `؛`=`;` (optional terminator, as Ya does), `؟`=`?`, `٫`=decimal point. Brackets and operators stay ASCII in all skins.

**Example: the same program in three skins**

```text
# en
func greet(name) {
  if name == none { print("hello, stranger") }
  else { print("hello, " + name) }
}
```
```text
#skin ar
دالة رحب(اسم) {
  إذا اسم == عدم { اطبع("أهلا يا غريب") }
  وإلا { اطبع("أهلا يا " + اسم) }
}
```
```text
#skin fr
dala ra7ab(esm) {
  law esm == mafeesh { ektb("ahlan ya 8areeb") }
  walla { ektb("ahlan ya " + esm) }
}
```

Translating between these files changes only keywords (`func/دالة/dala`, `if/إذا/law` …). The identifiers `greet/رحب/ra7ab` and `name/اسم/esm` change only when a glossary file is supplied. Note that `ra7ab` is a valid identifier because it does not *start* with a digit.

---

## 9. Open questions for the design phase

1. **Host ecosystem:** `03-implementation-strategy.md` recommends TS/JS first. The alias-pack strategy for npm APIs still needs to be designed.
1b. **Normalization form conflict:** `01-english-like-languages.md` says identifiers are normalized with **NFC**. This report (§6.5, §7.4) and `04-security-by-design.md` rely on **NFKC** plus Arabic skeleton folding. Decide on one; NFKC is recommended because it folds Arabic presentation forms.
2. **Glossary scope:** should the standard library ship official Arabic and Franco alias packs, or only keywords and errors?
3. **Franco orthography governance:** publish a versioned "Franco keyword spec", perhaps aligned with the *compliant-franco* draft, and run a community survey of 200+ Egyptian developers to choose canonical spellings.
4. **Arabic skin direction:** pure RTL files only, or allow LTR files with Arabic identifiers? Recommended: base direction per file from the skin.
5. **Positioning:** avoid the "kids/education only" trap. Lead with Franco scripting and automation for adults (Nadeshiko's office-automation lesson, 1C's captive-platform lesson), with فصحى for education and government.

---

## 10. Sources

**Language projects:**
- Qalb: https://github.com/nasser/--- · https://en.wikipedia.org/wiki/Qalb_(programming_language) · https://www.engadget.com/2014-05-07-ramsey-nasser-arabic-programming-language.html · https://www.theregister.com/2013/01/25/arabic_programming_language
- Alif: https://github.com/alifcommunity/Alif · https://aliflang.org · https://flathub.org/zh-Hant/apps/org.aliflang.lang
- Kalimat: https://arabnet.me/english/editorials/business/industry/kalimat-first-arabic-programming-language-for-kids · https://github.com/mobadarah/kalimat-lang
- Ebda3 / Abtaker: https://www.wamda.com/en/2014/03/first-arabic-programming-language-for-children-and-beginners-launches-crowdfunding-campaign
- Ammoria: https://sourceforge.net/p/ammoria/
- Phoenix: https://arxiv.org/pdf/1907.05871
- Arabic PL survey: https://www.academia.edu/39953345/Arabic_Programming_Language_Evolution_A_Survey
- APL (LLM compiler): https://arxiv.org/html/2403.16087v1
- Noor: https://github.com/SimplyAhmazing/noor · Ya: https://github.com/yalang/ya · Seen: https://github.com/abukhadra/seen/blob/main/README_en.md
- Daad: https://github.com/daadLang/daad · Baa: https://github.com/OmarAglan/Baa · Alarab_lang: https://github.com/Alarab-Developers/Alarab_lang · lughat-althuban: https://github.com/GalaxyRuler/lughat-althuban · Naftah: https://github.com/Chakib-DAII/Naftah
- Franco and dialect projects:
  - NashmiC: https://github.com/Ziadstr/nashmic
  - LFI3A: https://github.com/Oussama-jabrane/lfi3a
  - DarijaScript: https://github.com/rabraghib/DarijaScript
  - Ta3mya++: https://github.com/Abdulrahman-Khalid/ta3mya-plus-plus
  - Franco (2016): https://github.com/AhmedKamal1432/Franco
  - JulieLang: https://pypi.org/project/julielang/
  - Garga: https://github.com/hashemi/garga
  - Havnix: https://github.com/havnix/Havnix-Language
  - Blouloukh: https://github.com/amrkh24/Blouloukh-Lang
- Hedy: https://github.com/hedyorg/hedy · https://www.universiteitleiden.nl/en/news/2022/02/more-than-a-billion-people-can-now-learn-to-program-with-hedy · https://cs-lectures.itu.dk/lectures/230831-hermans-non-english-programming-with-hedy.html
- Sakhr MSX: https://msx.org/wiki/Sakhr_AX-350
- Al Jazeera blog (Arabic), on why Arabic languages stalled: https://www.aljazeera.net/blogs/2018/8/29/لغات-البرمجة-العربية-عناوين-لا-تجدها

**Comparison languages:**
- Wikipedia list: https://en.wikipedia.org/wiki/Non-English-based_programming_languages
- Wenyan: https://github.com/wenyan-lang/wenyan · https://www.star-history.com/wenyan-lang/wenyan
- 易语言: https://en.wikipedia.org/wiki/Easy_Programming_Language · https://www.secrss.com/articles/6873 · https://www.v2ex.com/t/560518
- 1C: https://en.wikipedia.org/wiki/1C:Enterprise · https://1c-dn.com/1c_enterprise/1c_enterprise_script/
- Nadeshiko: https://news.mynavi.jp/article/nadeshiko-1/ · https://docs.rs/nadesiko3
- JugaadLang: https://pypi.org/project/jugaadlang/1.1.2/ · Salam: https://github.com/SalamLang/Salam · Zangar: https://github.com/ui-ce/zangar

**Research:**
- Swidan & Hermans 2023, *A Framework for the Localization of Programming Languages*: https://www.hedy.org/research/A_Framework_for_the_Localization_of_Programming_Languages_2023.pdf
- Guo 2018, CHI: https://pg.ucsd.edu/publications/non-native-english-speakers-learning-programming_CHI-2018.txt
- Becker 2019, PPIG: https://ppig.org/files/2019-PPIG-30th-becker.pdf

**Unicode and security:**
- UAX #31: https://www.unicode.org/reports/tr31/
- Trojan Source: https://trojansource.codes/ · https://www.theregister.com/2021/11/01/trojan_source_language_reversal_unicode/
- Rust lint: https://doc.rust-lang.org/stable/nightly-rustc/rustc_lint/hidden_unicode_codepoints/static.TEXT_DIRECTION_CODEPOINT_IN_LITERAL.html
- GitHub bidi warning: https://github.blog/changelog/2021-10-31-warning-about-bidirectional-unicode-text/
- Arabizi: https://en.wikipedia.org/wiki/Arabic_chat_alphabet · compliant-franco: https://zff.dev/saadnpq/compliant-franco · Encode::Arabic::Franco: https://www.metacpan.org/pod/Encode::Arabic::Franco

**Primary verification done for this report:**
- GitHub API star and push data (2026-10-24).
- Hedy `highlighting/highlighting-trad.json` keyword regexes.
- NashmiC `compiler/src/lexer.c` and `keywords.c`.
- CPython 3.12 tests of Arabic digits, tatweel, hamza and NFKC behaviour.
