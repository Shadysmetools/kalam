# 04 — Security by Design: a Security Charter for the New Language

> Research date: 2026-10-24. Scope: language-level, standard-library-level, and package-manager-level security for a general-purpose language aimed at production SaaS, websites and games, with English / Arabic / Franco (Arabizi) keyword skins.
> Requirement under study: **"secure by default, unlike PHP."**

**Sourcing.** Sources are linked inline. Primary sources (vendor security blogs, CISA/NSA, the PHP RFC wiki, Unicode TRs, OWASP, GitHub's changelog) are preferred. Where only secondary coverage was checked, the text says so. Some figures are labelled "(secondary)": they come from press coverage of a primary post and should be checked against the original before anyone quotes them externally.

---

## 0. Executive summary

1. **Language decisions explain most of PHP's reputation, not bad programmers.** Defaults turned untrusted input into code, variables, file paths or objects: `register_globals`, `include $_GET[...]`, `eval`, `unserialize`, string-concatenated SQL and `==` type juggling. Modern PHP removed or tightened most of these. It cannot remove the API surface (`eval`, `unserialize`, `include` of variables, loose `==`) without breaking the ecosystem. **A new language's only real advantage is that it can leave these out from day one.**
2. **Memory safety is the floor.** Microsoft and Chromium each attribute about 70% of serious vulnerabilities to memory-unsafety. Google's Android data shows the share dropping as new code moves to memory-safe languages. CISA, NSA and FBI now formally say not to ship new memory-unsafe code. Compiling to C is fine. Letting C's undefined behaviour leak to users is not.
3. **Injection is a type error.** The most effective modern defence against SQL injection, XSS and command injection is to make "string that is safe in context X" a separate type, so untrusted data cannot reach a sink by accident. Google's safe types, Trusted Types, Go `html/template` and `sqlx` are examples. Our language should build this in, not leave it to libraries.
4. **Supply chain is now OWASP #3.** Install-time scripts, stolen publish tokens and typosquatting are the main attack paths in 2025-26 (Shai-Hulud, xz-utils). The package manager is part of the language's security boundary.
5. **The multi-script, multi-skin design is our largest new attack surface.** Trojan Source (bidi), homoglyphs, Arabic-specific confusables (ي/ی, ك/ک, Arabic vs Persian digits, tatweel, harakat, the Arabic Letter Mark bidi control) and Franco's digits-as-letters all need lexer-level rules from v0.1.

---

## 1. Why PHP got its insecure reputation

### 1.1 Root-cause table

| PHP feature (era) | What went wrong | Attack class (CWE) | Status in modern PHP | What remains |
|---|---|---|---|---|
| `register_globals` (on by default until 4.2, removed 5.4) | Request parameters became global variables, so `?admin=1` could set `$admin` | Variable injection / auth bypass (CWE-473, CWE-454) | **Removed** in 5.4 ([PHP 5.4 incompatible-changes list, mirror](https://www.cs.auckland.ac.nz/references/php/2012/migration54.incompatible.html)) | `extract($_GET)` still exists and repeats the problem |
| `magic_quotes_gpc` (removed 5.4) | Auto-`addslashes()` on input gave a false sense of SQL safety, broke on multibyte charsets (GBK bypass), and corrupted data | SQLi (CWE-89) via wrong-layer escaping | **Removed** in 5.4 ([PHP RFC: magic quotes](https://wiki.php.net/rfc/magicquotes)) | Lesson: escaping must depend on the **output context**, not happen on input |
| `mysql_*` + string concatenation | No parameter binding in the common API, so tutorials taught `"... WHERE id=$_GET[id]"` | SQL injection (CWE-89) | `ext/mysql` **removed** in 7.0. PDO/mysqli have prepared statements | Concatenation into `PDO::query()` is still legal. Nothing in the type system stops it |
| `include`/`require` of a variable plus `allow_url_include` | `include $_GET['page']` led to Local/Remote File Inclusion and RCE | CWE-98 (PHP file inclusion), CWE-22 (path traversal) | `allow_url_include` off by default and deprecated in 7.4 | LFI is still possible with any dynamic include. Log poisoning and `php://filter` chains still turn LFI into RCE |
| `eval`, `assert($string)`, `create_function`, `preg_replace /e` | String-to-code at runtime | Code injection (CWE-94, CWE-95) | `/e` removed in 7.0. `create_function` removed in 8.0. String `assert` removed in 8.0 | `eval` remains. Variable functions `$f()` and `call_user_func($_GET[...])` remain |
| `unserialize()` on user data | Magic methods (`__wakeup`, `__destruct`, `__toString`) form POP gadget chains, giving object injection and then RCE | CWE-502 (deserialization of untrusted data) | `allowed_classes` option (7.0). `phar://` metadata auto-unserialize removed in 8.0 | `unserialize()` defaults to allowing **all** classes. Gadget chains in frameworks are still found (PHPGGC) |
| Loose `==` (type juggling) | `"0e1234" == "0e5678"` is true (both are parsed as 0×10^n). Magic-hash bypasses of `md5($pw) == $stored`. Before 8.0, `0 == "anything"` was true | Auth bypass (CWE-697 incorrect comparison, CWE-1024) | PHP 8.0 "Saner string to number comparisons" RFC: `0 == "foo"` is now false ([RFC](https://wiki.php.net/rfc/string_to_number_comparison)) | **Numeric-string "0e…" comparisons are still true in PHP 8**, because both sides are numeric strings. `in_array`/`switch` are loose by default. `strcmp([], "x")` returns null. `===` must be opted into |
| Default error display | `display_errors` defaulted on in the built-in default, which leaked paths, SQL and stack traces | Information exposure (CWE-209) | `php.ini-production` sets it off | The built-in default without an ini file is still on. Framework debug pages (Ignition, Whoops) are common in production leaks |
| Weak and dynamic typing | Arrays where strings were expected, `null` passed to string functions, `"1abc" + 1` | Logic bugs leading to auth bypass | `declare(strict_types=1)` (7.0), union types, `TypeError` on internal functions (8.0) | `strict_types` is **per file and opt-in**. Coercive mode is the default |
| Superglobals plus no output encoding | `echo $_GET['q']` is a one-line XSS | XSS (CWE-79) | Unchanged in the core. Escaping lives in frameworks (Twig/Blade auto-escape) | Raw `echo` is still the language's primary output primitive |

### 1.2 Data points

- **OWASP Top 10:2025** keeps **Injection at A05** and adds **A03 Software Supply Chain Failures** and **A10 Mishandling of Exceptional Conditions** ([The Register](https://www.theregister.com/2025/11/11/new_owasp_top_ten_broken/), [Qualys](https://blog.qualys.com/qualys-insights/2026/06/15/what-changed-in-owasp-top-10-2025-and-recommendations-for-each-category)). The three classes PHP made easy (injection, misconfiguration and unsafe error handling) are all still on the list.
- **Deserialization (CWE-502)** and **code injection (CWE-94)** both appear on the [CWE Top 25](https://cwe.mitre.org/top25/). Whole exploit toolkits exist for PHP gadget chains ([PHPGGC](https://github.com/ambionics/phpggc)).
- WordPress-plugin CVE feeds (Wordfence, Patchstack) are dominated year after year by XSS, missing authorization/CSRF and SQLi, all in PHP code written against the raw APIs.

### 1.3 The lessons for our language

1. **Never turn data into code, variables, file paths or classes implicitly.** No `register_globals`, `extract`, dynamic `include`, `eval`, or polymorphic native deserialization.
2. **Escaping belongs at the sink and depends on context.** Magic quotes failed because it escaped at the source.
3. **Equality must be strict and typed.** There should be only one equality operator, and it should be a compile error to compare values of different types.
4. **Secure settings cannot be opt-in per file** (`strict_types`) **or per ini file** (`display_errors`). The secure mode is the only mode. An escape hatch must be explicit, local and greppable.
5. **Removing a feature later costs a decade.** PHP needed 4.2 → 5.4 → 7.0 → 8.0 (about 2002-2020) to kill these defaults. Our language must ship without them.

---

## 2. Memory safety

### 2.1 Evidence

| Source | Finding |
|---|---|
| Microsoft MSRC, 2019 ([blog](https://msrc.microsoft.com/blog/2019/07/a-proactive-approach-to-more-secure-code/)) | About **70%** of the CVEs Microsoft assigns each year are memory-safety issues |
| Chromium project ([memory-safety page](https://www.chromium.org/Home/chromium-security/memory-safety/)) | About **70%** of high and critical security bugs are memory-safety bugs, half of them use-after-free |
| Google Android, Sept 2024 ([Google Security Blog](https://security.googleblog.com/2024/09/eliminating-memory-safety-vulnerabilities-Android.html); secondary: [The Register](https://www.theregister.com/2024/09/25/google_rust_safe_code_android/), [The Hacker News](https://thehackernews.com/2024/09/googles-shift-to-rust-programming-cuts.html)) | Memory-safety share of Android vulnerabilities fell from **76% (2019) to 24% (2024)**, mainly by writing **new** code in memory-safe languages, without rewriting old code. Vulnerabilities decay with code age, so "the problem is overwhelmingly with new code." |
| Google Android, Nov 2025 (secondary: [Privacy Guides](https://www.privacyguides.org/news/2025/11/18/google-reports-less-than-20-of-new-vulnerabilities-in-android-are-memory-safety-related/)) | Below **20%** in 2025 |
| NSA CSI "Software Memory Safety", Nov 2022 | Recommends moving to memory-safe languages (C#, Go, Java, Ruby, Rust, Swift) |
| CISA et al. "The Case for Memory Safe Roadmaps", Dec 2023 ([CISA](https://www.cisa.gov/resources-tools/resources/case-memory-safe-roadmaps)) | Manufacturers should publish memory-safety roadmaps |
| White House ONCD "Back to the Building Blocks", Feb 2024 | Calls for memory-safe languages at the national-policy level |
| CISA/FBI "Product Security Bad Practices", Oct 2024 / Jan 2025 ([CISA](https://www.cisa.gov/resources-tools/resources/product-security-bad-practices); coverage: [TechRepublic](https://techrepublic.com/article/cisa-fbi-memory-safety-recommendations)) | Building **new** products for critical infrastructure in memory-unsafe languages is a "bad practice". Existing products should have a memory-safety roadmap **by 1 Jan 2026** (non-binding) |
| CISA + NSA CSI "Memory Safe Languages: Reducing Vulnerabilities in Modern Software Development", 24 Jun 2025 ([CISA alert](https://www.cisa.gov/news-events/alerts/2025/06/24/new-guidance-released-reducing-memory-related-vulnerabilities), [NSA](https://www.nsa.gov/Press-Room/Press-Releases-Statements/Press-Release-View/Article/4223298/)) | Memory-safe languages are "the most comprehensive mitigation" for this vulnerability class. Adoption can be incremental, through interop |

### 2.2 How existing languages achieve it

| Language | Spatial safety (bounds) | Temporal safety (UAF, double free) | Data-race safety | Escape hatch |
|---|---|---|---|---|
| Rust | Bounds-checked indexing | Ownership and borrowing at compile time, no GC | `Send`/`Sync` traits make data races compile errors | `unsafe { }` blocks, which are auditable |
| Go | Bounds checks | GC | **Not** race-free (race detector only). Racy map/slice/interface access can corrupt memory | `unsafe`, cgo |
| Swift | Bounds checks | ARC plus exclusivity enforcement | Swift 6 strict concurrency (actors, `Sendable`) | `Unsafe*Pointer` |
| Java / C# / Kotlin | Bounds checks | GC | Races are possible but memory-safe (JMM) | JNI / `unsafe` (C#), `sun.misc.Unsafe` |
| Pony | Bounds checks | GC per actor | **Reference capabilities** make data races impossible at compile time | FFI |

### 2.3 Implication for a language that compiles to C

Compiling to C (as Nim, Vala and early C++ did) is a legitimate back end, but **C's undefined behaviour must never be reachable from safe source code**:

- **Bounds**: emit checked indexing. Elide checks only when the optimizer proves them redundant.
- **Lifetime**: use GC, ARC/ORC (as Nim does), or ownership. Pick one; do not expose raw `free`.
- **Integer overflow**: signed overflow is UB in C. Emit `__builtin_add_overflow` and similar, or compile with `-fwrapv` *and* insert explicit checks.
- **Shifts** by ≥ the bit width, **division** of INT_MIN by -1, and **division by zero** are UB in C. Each needs a guard.
- **Strict aliasing, uninitialised reads, null dereference, unsequenced modifications** are UB too. Generate code that avoids them, or compile with `-fno-strict-aliasing`, zero-initialise everything, and make null impossible at the type level.
- **Hardening flags as defaults**: `-D_FORTIFY_SOURCE=3`, `-fstack-protector-strong`, `-fstack-clash-protection`, `-fcf-protection` / BTI, PIE, full RELRO, and `-ftrivial-auto-var-init=zero` (see [OpenSSF Compiler Options Hardening Guide](https://best.openssf.org/Compiler-Hardening-Guides/Compiler-Options-Hardening-Guide-for-C-and-C++)).
- **Continuously fuzz the C back end** with UBSan/ASan builds of generated code. Any sanitizer hit on safe source is a **compiler security bug**.
- **FFI**: calling C must happen inside an `unsafe`/`foreign` block that the package manager can count and the build can deny.
- **Games**: the performance escape hatch (SIMD, manual arenas, GPU buffers) should be a typed, bounds-checked arena or slice API, not raw pointers. Raw pointers go behind `unsafe`.

---

## 3. Secure-by-default language features

### 3.1 Type system and core semantics

| Feature | Prior art | Attack or bug class prevented | Recommendation |
|---|---|---|---|
| Strong static typing with local inference | Rust, Kotlin, Swift, TypeScript-strict, OCaml | Type juggling, confused-deputy logic bugs (PHP `==`, JS `[] + {}`) | **MUST** |
| No implicit coercion, one strict equality | Rust, Go, Swift | Magic hashes, `"1abc" == 1`, `null == 0` | **MUST**: `==` only between identical types. No truthiness. `if` takes `Bool` only |
| No null; use `Option<T>` / `T?` with forced handling | Rust, Swift, Kotlin; Hoare's "billion-dollar mistake" | Null-deref crashes (DoS), auth checks skipped on `null` | **MUST** |
| Explicit errors (`Result<T,E>` + `?`), no silent failure | Rust, Swift `throws`, Zig error unions | OWASP A10:2025 *Mishandling of Exceptional Conditions*: fail-open auth, ignored errors | **MUST**: an unused `Result` is a compile error (Rust's `#[must_use]`) |
| Immutability by default (`let`), explicit `mut` | Rust, Swift, Kotlin `val` | Shared-state bugs, TOCTOU, accidental config mutation | **SHOULD** (strongly) |
| Checked integer arithmetic | Swift (traps), Rust (debug traps, explicit `wrapping_`/`checked_`/`saturating_`), C# `checked` | Integer overflow leading to undersized allocation, price/quantity bugs (CWE-190) | **MUST trap in all build modes** (Swift model). Wrapping must be an explicit operator or function |
| Exact decimal type for money | C# `decimal`, Java `BigDecimal` | Rounding-based financial bugs | **SHOULD** ship `Decimal` in the core |
| Data-race freedom | Rust `Send/Sync`, Pony reference caps, Swift 6 | Race-condition auth bypass, memory corruption under concurrency | **SHOULD**: at minimum actor/message-passing as the default concurrency model |
| Exhaustive pattern matching | Rust, Swift, OCaml | Unhandled enum states (for example a new role that falls through to "allow") | **MUST** |

### 3.2 Injection-proof strings: tagged and contextual types

This is the most important feature for SaaS and the web. The pattern in all prior art is the same: **sinks accept a distinct type, not `String`**, and the only way to build that type from untrusted data is through a context-aware escaper or a literal written by the developer.

| Prior art | Mechanism | Evidence |
|---|---|---|
| Google "safe types" (`SafeHtml`, `SafeUrl`, `TrustedResourceUrl`) | Types whose contracts are enforced by builders and reviewed `unchecked` conversions | Kern, "Securing the Tangled Web", CACM 2014 ([ACM](https://cacm.acm.org/research/securing-the-tangled-web/)): XSS in Google apps dropped to near zero where the types were mandatory. [safe-html-types](https://github.com/google/safe-html-types) |
| W3C **Trusted Types** | DOM sinks (`innerHTML`, `eval`, `script.src`) throw unless given `TrustedHTML/TrustedScript/TrustedScriptURL` | [W3C spec](https://w3c.github.io/trusted-types/dist/spec/), [web.dev](https://web.dev/articles/trusted-types) |
| Go `html/template` | **Contextual auto-escaping**: escaping depends on whether the hole is in HTML text, an attribute, a URL, JS or CSS | [pkg.go.dev/html/template](https://pkg.go.dev/html/template) |
| Rust `sqlx` | `query!()` macros check SQL against the real schema at compile time. Parameters are always bound | [github.com/launchbadge/sqlx](https://github.com/launchbadge/sqlx) |
| JS tagged templates (`sql\`...\``, lit-html) / Scala string interpolators / Python PEP 750 t-strings (3.14) | The library receives literal parts and values separately, so it can bind or escape the values | [PEP 750](https://peps.python.org/pep-0750/) |
| Taint tracking (Perl `-T`, Ruby `$SAFE` (removed), CodeQL) | Mark untrusted data and block it at sinks | Runtime taint was slow and leaky. **Static, type-based** separation works better |

**Design for our language:**

```text
// Pseudocode (English skin)
let q  = sql"SELECT * FROM users WHERE email = {email}"   // SqlQuery, email is bound, not spliced
let h  = html"<p>Hello {name}</p>"                        // Html, name is escaped for the HTML-text context
let c  = cmd"git log {branch}"                             // Command, argv array, no shell
db.query(someString)   // COMPILE ERROR: expected SqlQuery, found String
```

- `String` never coerces to `SqlQuery`, `Html`, `Url`, `Command`, `Path` or `Regex`.
- Literal-only constructors (a compile-time constant tag), like Go's `template.HTML` but restricted to **literals**, as Google's "compile-time constant" rule does.
- `unsafe_raw_html(x)` exists, but every use is flagged by the linter, counted by the package manager, and greppable in review.
- Shell: the stdlib has **no `system(string)`**. Process spawning takes `argv: [String]` only.
- Paths: `Path.join(base, userInput)` returns `Result`, with an error on `..`/absolute escape (prevents CWE-22). A `SafeRelativePath` type is used for user-supplied paths.
- Optional (SHOULD): `sqlx`-style compile-time schema checking for the first-party DB layer.

### 3.3 Capability-based security and permissions

| Prior art | Model | Lesson |
|---|---|---|
| **Deno** | Process-level permissions (`--allow-net=api.x.com`, `--allow-read=./data`), deny by default | [docs.deno.com](https://docs.deno.com/runtime/fundamentals/security/). Proven DX for coarse-grained permissions. Weakness: granularity is per process, not per dependency |
| **WASI** (Preview 2 / component model) | No ambient authority. The host passes in handles (preopened dirs, sockets) | [wasi.dev](https://wasi.dev/). Capabilities as values |
| **Pony** | Reference capabilities (`iso`, `val`, `ref`, `box`, `tag`, `trn`) give race freedom. Object-capability style for I/O via `AmbientAuth` | [ponylang.io](https://www.ponylang.io/) |
| **Austral** | Linear types plus capabilities: `main` receives `RootCapability`, and a library cannot touch the filesystem unless passed one | [austral-lang.org](https://austral-lang.org/) |
| Java SecurityManager | Deprecated (JEP 411), because stack-inspection policy was too complex | Lesson: keep the model simple and **static** |

**Design:** `main(io: World)`. Filesystem, network, env, clock, randomness and subprocess are **capability values** that must be passed down explicitly. A dependency whose public API never receives `Net` **cannot** do network I/O, and the compiler can prove it. The manifest declares per-package capabilities (`caps = ["net:api.stripe.com"]`), and the package manager shows any capability change on upgrade as a diff. This turns "a compromised leftpad exfiltrates `~/.aws`" from a runtime incident into a type error.

### 3.4 Code loading, eval and deserialization

| Rule | Prevents |
|---|---|
| **No `eval` / no string-to-code** in the language. If scripting is needed (game modding, plugins), provide a separate **sandboxed interpreter instance** (Wasm/VM) with zero capabilities by default and explicit fuel, memory and time limits | CWE-94/95, Log4Shell-style "data interpreted as code" ([CVE-2021-44228](https://nvd.nist.gov/vuln/detail/CVE-2021-44228)) |
| **No dynamic `import` of a runtime string.** Modules resolve at compile time | LFI/RFI (CWE-98) |
| **No reflective call-by-name** (`call(userString)`) outside an explicit `reflect` capability | `call_user_func($_GET[f])` |
| **Deserialization is schema-first.** `decode<T>(bytes)` produces exactly the declared type `T` (serde/Codable model). Never instantiate class names from the payload. No constructors, destructors or magic methods run during decode | CWE-502 object injection (PHP `unserialize`, Java `ObjectInputStream`, Python `pickle`, YAML `!!python/object`) |
| Decoders enforce **size, depth and count limits** by default | Billion-laughs, JSON bombs, hash-flooding DoS |
| Hash maps use **SipHash or randomized seeds** by default | Hash-flooding DoS ([CVE-2011-4885](https://nvd.nist.gov/vuln/detail/CVE-2011-4885), the PHP `max_input_vars` incident) |
| No `__proto__`-like magic keys, and object-as-map is a distinct `Map` type | Prototype pollution (CWE-1321) |

### 3.5 Secrets, crypto and randomness

- **`Secret<T>` type**: its `Display`/`Debug`/`toString`/JSON encoding emits `"[REDACTED]"`. Reading the value requires `.expose()`, which is greppable. The memory is zeroized on drop. Prior art: Rust [`secrecy`](https://docs.rs/secrecy), Pydantic `SecretStr`. Config loaders return `Secret` for keys matching `*_KEY`, `*_SECRET`, `*_TOKEN` or `PASSWORD`. **Prevents CWE-532** (secrets in logs) and leaks into error pages and crash reports.
- **Constant-time comparison** (`crypto.equal(a, b)`) is the only equality defined on `Secret`/`Mac`/`Hash`, so `==` on them is a compile error. **Prevents** timing attacks (CWE-208).
- **A misuse-resistant crypto stdlib** with high-level APIs only: AEAD (`seal`/`open`), Argon2id/scrypt password hashing (`password.hash`/`verify`), Ed25519 signatures, HKDF, X25519. No ECB, no raw RSA, no MD5/SHA-1 except under `legacy::`. Prior art: libsodium, Go `crypto/*`, Tink. **Prevents** OWASP A04:2025 Cryptographic Failures.
- **Randomness**: the default `random()` is a CSPRNG (as in Go 1.22's `math/rand/v2` ChaCha8 global source). A fast seeded PRNG for games is explicit (`FastRng.seeded(n)`). **Prevents** predictable tokens (CWE-338).
- **Password storage**: `password.hash()` is the only API, and it includes salt, parameters and upgrade logic. **Prevents** the magic-hash/`md5(pw)` class entirely.

### 3.6 Secure defaults in the HTTP / web stdlib

| Default | Prevents |
|---|---|
| Templates use contextual auto-escaping (Go `html/template` semantics). Raw output needs `unsafe_raw` | XSS (CWE-79) |
| CSRF protection on for all state-changing methods with cookie sessions (synchronizer token or `Sec-Fetch-Site`/Origin check, as in Go 1.25 `http.CrossOriginProtection`) | CSRF (CWE-352) |
| Cookies default to `Secure; HttpOnly; SameSite=Lax`, with a `__Host-` prefix for session cookies | Session theft, CSRF |
| CORS **closed** by default. `*` with credentials is impossible to configure | Credential leakage |
| Security headers by default: CSP (nonce-based, `script-src 'nonce-…' 'strict-dynamic'`), `X-Content-Type-Options: nosniff`, `Referrer-Policy: strict-origin-when-cross-origin`, `frame-ancestors 'none'`, HSTS when served over TLS, and Trusted Types `require-trusted-types-for 'script'` in the CSP | XSS, clickjacking, MIME sniffing |
| Errors: production builds **never** render stack traces. A request ID is shown and details go to the log (with `Secret` redacted). There is no runtime "debug mode" flag that can be left on in production; debug pages exist only in dev builds | CWE-209 (PHP `display_errors`) |
| Request limits by default: body size, header size, timeouts (read/write/idle), slowloris protection | DoS |
| Outbound HTTP client blocks private, link-local and metadata IPs (169.254.169.254) unless a capability allows them, and re-checks after DNS resolution | SSRF (CWE-918) |
| Input binding goes into declared structs only (no mass assignment of unknown fields) and rejects unknown fields by default | Mass assignment (CWE-915), `register_globals` redux |
| Authorization middleware is **deny by default**: a route without an explicit `.public()` or `.requires(role)` fails to compile or start | OWASP A01 Broken Access Control |
| TLS 1.2+ only, certificate verification on, no `verify=False` without `unsafe` | MITM |
| Redirects take only relative paths or an allow-list | Open redirect (CWE-601) |
| Upload helpers: content sniffing, size limits, randomized storage names, never executable | Unrestricted upload (CWE-434) |

### 3.7 Package manager and supply chain

Context: OWASP Top 10:2025 placed **Software Supply Chain Failures at A03**. In 2025 the **Shai-Hulud** worm self-propagated through npm using `postinstall`/`preinstall` scripts and stolen tokens. Reports range from 180 to 500+ packages in September and tens of thousands of repos in the November "2.0" wave ([Black Duck](https://blackduck.com/blog/npm-malware-attack-shai-hulud-threat.html), [Cycode](https://cycode.com/blog/shai-hulud-npm-supply-chain-attack/); counts vary by vendor). The **xz-utils backdoor** ([CVE-2024-3094](https://nvd.nist.gov/vuln/detail/CVE-2024-3094)) hid in build-time test fixtures and release tarballs that differed from git. In response, **pnpm 10** disabled dependency lifecycle scripts by default, with an allow-list in `onlyBuiltDependencies` ([release notes](https://newreleases.io/project/npm/pnpm/release/10.0.0)). A bypass via git dependencies, CVE-2025-69264, was fixed in 10.26 ([OSV](https://osv.dev/vulnerability/CVE-2025-69264)), which shows that "no scripts" must cover **every** fetch path. GitHub/npm revoked classic tokens and pushed **OIDC trusted publishing with automatic provenance** ([GitHub changelog](https://github.blog/changelog/2025-09-29-strengthening-npm-security-important-changes-to-authentication-and-token-management/)).

| Control | Prior art | Prevents |
|---|---|---|
| **No install-time code execution, ever**, on every path (registry, git, path dependencies). Build-time codegen runs in a sandbox with no network, a read-only source tree and output-only write access | pnpm 10, Deno (no install scripts), Go (no install hooks) | Shai-Hulud, credential theft on `install` |
| Mandatory **lockfile with content hashes**. CI installs are frozen (`--locked`) | Cargo.lock, Go `go.sum` + checksum DB | Dependency swapping, tampering |
| **Transparency log / checksum database** for every published version (immutable versions, no re-publishing a version) | Go [sumdb](https://go.dev/ref/mod#checksum-database) | Targeted malicious versions, registry compromise |
| **Sigstore signing plus SLSA provenance** on publish. Publishing **only** via OIDC trusted publishing from CI, with no long-lived tokens | [Sigstore](https://www.sigstore.dev/), [SLSA](https://slsa.dev/), npm trusted publishing, PyPI trusted publishers | Token theft, "tarball ≠ source" (xz) |
| **Registry builds from source** or verifies that the published artifact matches the tagged commit | SLSA L3, reproducible builds | xz-style tarball injection |
| **Reproducible builds** of the compiler and packages (bit-for-bit, documented `SOURCE_DATE_EPOCH`) | [reproducible-builds.org](https://reproducible-builds.org/), Go toolchain (reproducible since 1.21) | Compromised build machines (Thompson's "trusting trust") |
| **Per-package capability manifest** (section 3.3), with upgrades that **add** capabilities requiring explicit approval | Deno permissions, [Socket](https://socket.dev/)-style capability diffs, LavaMoat | Compromised dependency gaining new behaviour |
| **Typosquat and namespace defences**: mandatory scopes/namespaces (`@org/pkg`), Levenshtein, confusable-skeleton and **cross-script** similarity checks at publish time (critical with Arabic and Franco names), reserved names | PyPI/npm typosquat heuristics, UTS #39 skeletons | Typosquatting, dependency confusion |
| **Dependency-confusion protection**: private registries are namespace-pinned, never fall back to public | Microsoft/Birsan 2021 research | Dependency confusion |
| Minimum release age / cooldown for new versions (for example 24-72 h before auto-adoption) | pnpm `minimumReleaseAge`, Renovate `minimumReleaseAge` | Fast-moving worms (most malicious versions are pulled within hours) |
| 2FA/passkeys mandatory for maintainers, and a mandatory second maintainer approval for ownership transfer | npm, PyPI 2FA mandates | Account takeover, xz-style social engineering |
| `unsafe`/FFI/`unsafe_raw_html` **counters** shown on package pages and in `pkg audit` | `cargo-geiger` | Hidden unsafe surface |
| Built-in `pkg audit` against [OSV](https://osv.dev/), plus SBOM output (CycloneDX/SPDX) | `govulncheck` (reachability-aware), `cargo audit` | Known-vuln dependencies |

---

## 4. Risks specific to our design

### 4.1 Unicode, bidi and "Trojan Source", made critical by Arabic support

In 2021, Boucher and Anderson showed **CVE-2021-42574** (bidi overrides reorder how tokens *display* without changing how they *parse*) and **CVE-2021-42694** (homoglyph identifiers). They affected C, C++, C#, JS, Java, Rust, Go and Python ([trojansource.codes](https://trojansource.codes/); [Help Net Security](https://www.helpnetsecurity.com/2021/11/02/supply-chain-attacks-source-code/)). Rust 1.56.1 responded with two lints that are errors by default, `text_direction_codepoint_in_literal` and `text_direction_codepoint_in_comment`. Its earlier `confusable_idents`, `mixed_script_confusables` and `uncommon_codepoints` lints, which arrived with non-ASCII identifiers, already covered homoglyphs ([Rust blog](https://blog.rust-lang.org/2021/11/01/cve-2021-42574.html)). The Unicode Consortium published **UTS #55 "Unicode Source Code Handling"** (v2, 2024) for language designers ([unicode.org/reports/tr55](https://unicode.org/reports/tr55/), [blog](https://blog.unicode.org/2023/09/source-code-handling-preventing.html)), alongside **UAX #31** (identifiers) and **UTS #39** (confusables, mixed-script detection).

**Most languages can simply ban RTL in identifiers. We cannot, because Arabic identifiers and keywords are a product feature.** Arabic adds risks that Latin-only languages never see:

| Arabic-specific hazard | Example | Mitigation |
|---|---|---|
| Arabic Letter Mark **U+061C** is itself a bidi control | Invisible, reorders display | Treat it the same as U+202A-202E and U+2066-2069 |
| Implicit bidi reordering of mixed Arabic and Latin/digits **without** any control character | `if عمر > 18` can render with operands visually swapped in some editors | Our own `fmt` and LSP render code using UTS #55's recommended per-token isolation. Diff/review tools show the canonical (LTR-token-ordered) view. Ship a "bidi skeleton" check (ICU `bidiSkeleton`) |
| Letter variants: ي U+064A vs ی U+06CC (Farsi yeh), ك U+0643 vs ک U+06A9 (keheh), ه vs ە, final and isolated forms (U+FExx presentation forms) | `مستخدم` spelled with two different yehs gives two different identifiers that look identical | NFKC normalization (folds presentation forms) **plus** a project-level rule: identifiers with Arabic-script confusable pairs map to one **skeleton**, and a skeleton collision in the same scope is a compile error |
| Tatweel/kashida **U+0640** | `ســعر` == `سعر` visually (stretched) | Disallow U+0640 in identifiers (it is not XID_Continue-relevant for meaning) |
| Harakat (diacritics U+064B-U+065F, U+0670) | `سعر` vs `سَعر` | Strip from identifiers (or disallow). Allow in strings only |
| Arabic-Indic digits ٠-٩ (U+0660) vs Extended/Persian ۰-۹ (U+06F0), plus Latin 0-9 | Most of the two sets look identical but are different codepoints (only 4/5/6 differ in shape, and font-dependently). Worse: Arabic-Indic one `١` resembles alef `ا`, Latin `l` and `1`, and zero `٠` resembles a dot | Numeric literals: allow **one digit system per literal**. The formatter normalizes. Digit systems are never mixed in a literal. The identifier skeleton maps `١`, `ا`, `l` and `1` to one class (verify the exact pairs against UTS #39 `confusables.txt`) |
| ZWJ/ZWNJ (U+200D/U+200C) | Invisible, changes shaping | Disallow in identifiers (UAX #31 permits them only in narrow contexts not needed for Arabic) |
| Cross-script homoglyphs in Latin skins: Cyrillic `а`, Greek `ο` | `pаypal` | UAX #31 + UTS #39 **mixed-script** restriction: an identifier must be single-script (Arabic, Latin, or "Common + one script") |

### 4.2 Multiple keyword skins (English / Arabic / Franco)

| Risk | Example | Mitigation |
|---|---|---|
| **A keyword in one skin is an identifier in another** | In the English skin, a variable named `law` (Franco for "if", لو) could be read by an Arabic/Franco reader as a conditional | Two options. (a) Reserve **every skin's keywords in every skin**. This is the safest option, but it bans ordinary English identifiers such as `law`, `fe` and `men` (Franco keywords are Latin script). (b) Reserve within each skin only, and have the formatter and LSP **warn** when an identifier equals another skin's keyword, auto-escaping it (`r#law`) on skin conversion. Recommendation: (b) plus a mandatory escape on cross-skin rendering. Leave the final call to the keyword-table design |
| **Mixed skins in one file** confuse reviewers | Half English `if`, half Arabic `إذا` | **One skin per file**, declared in a header or file extension. The compiler rejects mixing |
| **Reviewers read a different skin than the author wrote** | A malicious PR uses a subtle Arabic keyword variant | **One canonical AST and a canonical skin for diffs.** `fmt --skin=en` is lossless. Code review and CI diffs can render in the reviewer's skin. Signatures and hashes cover the canonical form |
| **Franco uses digits as letters** (2=ء, 3=ع, 5=خ, 7=ح, 9=ق) | `3ard` (عرض) or `7aga` (حاجة) begin with digits, which collides with numeric literals and suffixes (`3e5`, `0x...`, `7f`) | Report 03 (section 4.1) proposes a maximal-munch rule: a full number-grammar match is a number, then a keyword, and otherwise an identifier if it contains a letter, so `3ala` and `7aga` are legal. That is workable, but it is a **spoofing surface**: `0x1Fa` vs `0x1Fg`, `1e5` vs `1e5x`, `7f` vs `7F`, where one character flips a token between number and identifier. Security additions: (a) digit-leading identifiers are allowed **only in Franco-skin files**. (b) Any digit-leading identifier that is within edit distance 1 of a valid number literal is a compile error. (c) Franco keywords never parse as numbers (as report 03 already says), enforced by a unit test. (d) `fmt --skin=en` must re-print digit-leading identifiers with an escape such as `r#3ala`, so other skins never see them bare |
| Keyword spoofing via confusables | Arabic keyword `لو` spelled with a presentation-form `ﻟ` | Keyword matching **after** NFKC + Arabic skeleton normalization. Non-normalized source is rejected (or auto-fixed by `fmt`) |
| Error messages in a different skin hide meaning | Security warning in a language the reader does not read | Diagnostics carry a stable code (`SEC0042`) and render in the user's locale |
| Search/grep for dangerous calls misses skins | Grep for `unsafe` misses the Arabic `غير_آمن` | The audit tool works on the **AST**, not on text. `pkg audit --unsafe` counts across skins. Escape hatches use the **same ASCII keyword in all skins** (for example `unsafe`), on purpose, so they are always greppable |

### 4.3 Transpiling to JavaScript (web target)

- **Prototype pollution (CWE-1321)**: never compile language maps or records to plain JS objects keyed by user strings. Use `Map` or `Object.create(null)`, and reject `__proto__`/`constructor`/`prototype` keys on decode.
- **JS coercion semantics leaking through**: the generated code must use `===`, explicit numeric checks, and BigInt or checked arithmetic for integer types. JS `Number` silently loses precision above 2^53, so map `Int64` to BigInt or a checked wrapper.
- **DOM sinks**: the runtime emits Trusted Types policies and never writes `innerHTML` with a `String`. `Html` values map to `TrustedHTML`.
- **No `eval` / `new Function`** in generated code, so a strict CSP without `'unsafe-eval'` works.
- **Bundle supply chain**: SRI hashes on emitted `<script>` tags, and no CDN fetches at runtime by default.

### 4.4 Compiling to C (native/game target)

See section 2.3. In short: UB-free codegen, checks always on, hardening flags on, sanitizer-fuzzed generated C, and FFI only behind `unsafe`. Also: **pin and bundle a known C compiler version** (zig cc / clang) for reproducibility, so the system compiler's UB-exploiting optimizations cannot change semantics between machines.

### 4.5 Games-specific notes

- Mod/scripting systems are the "eval" problem in disguise. Run mods in the **sandboxed VM** (section 3.4) with zero capabilities, fuel and memory limits.
- Save files and network packets are untrusted deserialization. Use schema-first decode with limits (section 3.4).
- Multiplayer: an authoritative-server template in the stdlib avoids trusting client state.

---

## 5. The Security Charter

Priority key: **P0** = must exist in v0.1 (cannot be retrofitted without breaking code). **P1** = must exist by 1.0. **P2** = should, by 1.x.

### 5.1 Language core

| # | Rule | Level | Priority | Rationale | Attack class prevented |
|---|---|---|---|---|---|
| L1 | Safe code is memory-safe: bounds-checked, no manual free, no dangling references. The only exception is code inside an `unsafe` block | MUST | P0 | About 70% of severe bugs in C/C++ codebases (MSRC, Chromium). CISA/NSA guidance | Buffer overflow, UAF, double free (CWE-119/416/415) |
| L2 | No undefined behaviour is reachable from safe code on **any** back end (C, JS, Wasm). Sanitizer hits on generated code are compiler CVEs | MUST | P0 | Compile-to-C must not import C's UB | UB-driven miscompilation, memory corruption |
| L3 | Static, strong typing with inference. **No implicit conversions** between types | MUST | P0 | PHP type juggling | Auth bypass via coercion (CWE-697/843) |
| L4 | One equality operator, only between identical types. No truthiness (`if` requires `Bool`) | MUST | P0 | `"0e1" == "0e2"`, `0 == "a"` | Magic-hash and type-juggling bypass |
| L5 | No null. `Option<T>` with exhaustive handling | MUST | P0 | Null derefs, skipped checks | DoS, logic bypass (CWE-476) |
| L6 | Errors are values (`Result`). Ignoring a `Result` is a compile error. No unchecked exceptions crossing API boundaries | MUST | P0 | OWASP A10:2025 | Fail-open error handling (CWE-390/755) |
| L7 | Integer overflow traps in all build modes. Wrapping/saturating are explicit | MUST | P0 | Allocation-size and money bugs | CWE-190/191 |
| L8 | Exhaustive `match` on enums/sum types | MUST | P0 | New states falling through to "allow" | Authorization logic bugs |
| L9 | **No `eval`**, no runtime string-to-code, no dynamic import of runtime strings, no call-by-name without a `reflect` capability | MUST | P0 | PHP `eval`/`include $x`/`call_user_func` | Code injection, LFI/RFI (CWE-94/95/98) |
| L10 | Immutable by default; `mut` is explicit | SHOULD | P0 | Shared-state and TOCTOU bugs | Race-based logic bugs |
| L11 | Data-race freedom (ownership/`Sendable` or actor-only shared state) | SHOULD | P1 | Go shows races can corrupt memory | Races (CWE-362) |
| L12 | **Capability-passing I/O**: `main` receives `World`. FS/Net/Env/Proc/Clock/Rand are values. No ambient authority in libraries | MUST | P0 | Austral, WASI, Deno | Malicious or compromised dependencies, SSRF from libraries |
| L13 | Every escape hatch (`unsafe`, `unsafe_raw_html`, FFI, `expose()` on secrets) uses one **ASCII keyword identical in all skins** and is countable by tooling | MUST | P0 | Auditability across skins | Hidden unsafe code |
| L14 | `Decimal` type in core | SHOULD | P1 | Financial SaaS | Rounding/fraud bugs |

### 5.2 Source text, Unicode and skins

| # | Rule | Level | Priority | Rationale | Attack class prevented |
|---|---|---|---|---|---|
| U1 | Reject unpaired or any bidi control characters (U+202A-E, U+2066-9, U+200E/F, **U+061C**) anywhere in code tokens. In **string literals**, they are allowed only as escapes (`\u{202E}`), never raw. In **comments**, raw bidi controls are rejected (or the tooling renders them isolated and visible) | MUST | P0 | Trojan Source CVE-2021-42574, UTS #55 | Bidi spoofing |
| U2 | Source must be NFC (or NFKC for identifiers). Non-normalized files are rejected and `fmt` fixes them | MUST | P0 | UAX #31, UTS #55 | Normalization-based spoofing |
| U3 | Identifiers follow UAX #31 (XID_Start/Continue). They must be **single-script** (UTS #39 §5.2 restriction level "Single Script", where Common/Inherited plus one script is allowed; "Highly Restrictive" would also allow some CJK combinations we do not need). No ZWJ/ZWNJ, tatweel or harakat in identifiers | MUST | P0 | CVE-2021-42694 homoglyphs | Homoglyph identifiers |
| U4 | **Confusable-skeleton collisions** (UTS #39 plus an Arabic variant table: ي/ی, ك/ک, ه/ە, presentation forms) within a scope or between a module's exports are compile errors | MUST | P0 | Arabic letter variants | Lookalike-identifier shadowing |
| U5 | Numeric literals use one digit system each. Keywords never parse as numbers. Digit-leading identifiers (report 03's Franco rule) are allowed only in Franco-skin files, are rejected when within edit distance 1 of a valid number literal, and are escaped (`r#3ala`) when rendered in other skins | MUST | P0 | Franco 3/7/2 letters; Arabic vs Persian digits | Lexer ambiguity, misread constants |
| U6 | One skin per file (MUST). An identifier that equals another skin's keyword triggers a warning and is auto-escaped on skin conversion. Full cross-skin reservation is optional, because it would ban English words such as `law`/`fe`/`men` (see §4.2) | MUST | P0 | Cross-skin confusion | Keyword/identifier spoofing |
| U7 | A canonical AST serialization is the unit of hashing, signing and diffing. Reviewers can render any skin losslessly | MUST | P1 | Review integrity | Skin-switching review evasion |
| U8 | First-party formatter, LSP and diff viewer apply UTS #55 display rules (token isolation, bidi skeleton warnings) | SHOULD | P1 | Implicit bidi reordering exists even without control characters | Visual reordering in Arabic code |
| U9 | Lint for invisible and uncommon codepoints in string literals (zero-width, tag characters, variation selectors) | SHOULD | P1 | Hidden data and smuggled prompts | Hidden payloads |

### 5.3 Standard library

| # | Rule | Level | Priority | Rationale | Attack class prevented |
|---|---|---|---|---|---|
| S1 | Sinks take typed values: `SqlQuery`, `Html`, `Url`, `Command(argv)`, `Path`, `Regex`. `String` never coerces. Construction happens through tagged literals with contextual escaping or binding | MUST | P0 | Google safe types, Trusted Types, Go `html/template` | SQLi, XSS, command injection, path traversal (CWE-89/79/78/22) |
| S2 | No shell-string process API. `spawn(argv)` only | MUST | P0 | `system($x)` | OS command injection |
| S3 | Templates use contextual auto-escaping (HTML/attr/URL/JS/CSS) | MUST | P0 | Go `html/template` | XSS |
| S4 | Deserialization is schema-first (`decode<T>`), never instantiates types named by the payload, runs no user hooks, and enforces size/depth/count limits by default | MUST | P0 | PHP `unserialize`, Java, pickle | Object injection, RCE, DoS (CWE-502/400) |
| S5 | `Secret<T>` redacts in all formatting, logging and serialization, is zeroized on drop, compares in constant time, and config loaders return it for secret-like keys | MUST | P1 | Leaks into logs and error pages | CWE-532, CWE-209, timing (CWE-208) |
| S6 | Misuse-resistant crypto: AEAD, Argon2id `password.hash/verify`, Ed25519, HKDF. Legacy algorithms only under `legacy::`. Constant-time implementations | MUST | P1 | OWASP A04:2025 | Weak crypto, magic-hash, timing |
| S7 | Default RNG is a CSPRNG. Seeded fast RNG is explicit (games) | MUST | P0 | Predictable tokens | CWE-330/338 |
| S8 | HTTP server defaults: CSRF on, cookies `Secure/HttpOnly/SameSite=Lax`, CORS closed, CSP + nosniff + frame-ancestors + Referrer-Policy + HSTS, body/header/time limits | MUST | P1 | OWASP A02:2025 Misconfiguration | CSRF, XSS, clickjacking, DoS |
| S9 | Routes are **deny by default**: each route must declare `.public()` or an authorization requirement, or the app fails to build | MUST | P1 | OWASP A01:2025 | Broken access control (CWE-862) |
| S10 | Request binding into declared types only. Unknown fields are rejected | MUST | P1 | `register_globals`, mass assignment | CWE-915 |
| S11 | Production builds cannot render stack traces or debug pages. There is no runtime debug toggle | MUST | P1 | PHP `display_errors` | Info disclosure (CWE-209) |
| S12 | The HTTP client blocks private, loopback, link-local and metadata ranges after DNS resolution unless a capability allows them. TLS verification is always on | MUST | P1 | Cloud-metadata SSRF | SSRF (CWE-918), MITM |
| S13 | `Path.join` refuses `..`/absolute escapes. Uploads are stored with random names and are never executable | MUST | P1 | LFI, upload-to-RCE | CWE-22, CWE-434 |
| S14 | Hash maps are DoS-resistant (keyed SipHash) | MUST | P0 | CVE-2011-4885 class | Hash flooding |
| S15 | Sandboxed scripting VM (Wasm-based) with zero capabilities and fuel/memory limits, for plugins and game mods | SHOULD | P2 | Legitimate need for dynamic code | Mod/plugin RCE |
| S16 | Compile-time checked SQL against the schema (sqlx-style) | SHOULD | P2 | Catches injection and type bugs early | SQLi, runtime errors |
| S17 | Open-redirect-safe `redirect()` (relative paths or allow-list) | SHOULD | P1 | Phishing | CWE-601 |

### 5.4 Package manager and toolchain

| # | Rule | Level | Priority | Rationale | Attack class prevented |
|---|---|---|---|---|---|
| P1 | **No install-time code execution on any fetch path** (registry, git, path). Build-time codegen runs sandboxed with no network | MUST | P0 | Shai-Hulud (2025); pnpm CVE-2025-69264 bypass | Malicious install scripts, credential theft |
| P2 | Lockfile with content hashes. CI uses frozen installs. Versions are immutable | MUST | P0 | Tamper-evidence | Dependency substitution |
| P3 | Public transparency/checksum log for all versions (Go sumdb model) | MUST | P1 | Registry compromise | Targeted malicious versions |
| P4 | Publish **only** via OIDC trusted publishing. Sigstore signatures and SLSA provenance are mandatory. No long-lived tokens | MUST | P1 | npm classic-token revocation (2025) | Token theft, unauthorized publish |
| P5 | Registry verifies that the artifact reproduces from the tagged source (or builds it itself) | SHOULD | P2 | xz-utils CVE-2024-3094 | Tarball ≠ source backdoors |
| P6 | Per-package capability manifest, enforced by the compiler (L12). Capability **additions** on upgrade need explicit approval | MUST | P1 | Least privilege for dependencies | Compromised dependency escalation |
| P7 | Mandatory namespaces. Publish-time similarity checks: edit distance, UTS #39 skeleton, **cross-script** (Latin↔Arabic↔Franco transliteration) | MUST | P1 | Typosquatting; three skins triple the name space | Typosquat, dependency confusion |
| P8 | Private registries are namespace-pinned with no public fallback | MUST | P1 | Birsan 2021 | Dependency confusion |
| P9 | Default minimum release age (for example 48 h) before a new version is auto-resolved | SHOULD | P1 | Worms spread within hours | Fast-propagating malware |
| P10 | Built-in `audit` (OSV, reachability-aware like `govulncheck`), SBOM export, and `unsafe`/FFI/raw-HTML counters shown per package | MUST | P1 | Visibility | Known-vulnerable or hidden-unsafe dependencies |
| P11 | 2FA/passkeys mandatory for maintainers. Ownership transfer needs a delay and notification | MUST | P1 | xz social engineering, account takeover | Maintainer takeover |
| P12 | Reproducible compiler and package builds. Pinned, bundled C toolchain for the native back end | SHOULD | P1 | Trusting-trust, cross-machine UB differences | Build-system compromise |
| P13 | Default hardening flags on native output (FORTIFY=3, stack protector, CFI/CET/BTI, PIE, RELRO, auto-var-init=zero) | MUST | P1 | OpenSSF hardening guide | Exploit mitigation for FFI/unsafe code |
| P14 | Generated JS: no `eval`/`new Function`, Trusted Types policies, prototype-safe maps, SRI on emitted scripts | MUST | P1 | Web target integrity | Prototype pollution, CSP bypass, XSS |

### 5.5 Process (for the language project itself)

| # | Rule | Level | Rationale |
|---|---|---|---|
| G1 | Publish a security policy, a CVE process and a memory-safety/UB bug class for the compiler itself | MUST | CISA Secure by Design pledge |
| G2 | Continuously fuzz the parser, lexer (especially Unicode/skins), decoders and the C/JS back ends with sanitizers | MUST | Lexer and codegen are attack surface |
| G3 | Every new escape hatch requires a written security review | SHOULD | Prevents slow erosion of defaults |
| G4 | No feature ships "insecure now, fix later". Defaults are frozen at 1.0 | MUST | PHP took 18 years to remove its defaults |

---

## 6. "Unlike PHP": the one-page contrast

| PHP default (historic or current) | Our default |
|---|---|
| `$_GET` values become variables (`register_globals`, `extract`) | Request data binds only into declared types (S10) |
| `include $page` | Modules resolved at compile time. No dynamic include (L9) |
| `eval`, `create_function`, `/e` | No eval. A sandboxed VM if needed (L9, S15) |
| `unserialize($_COOKIE[...])` | `decode<T>`, schema-first, no gadgets (S4) |
| `"SELECT ... $id"` | `sql"... {id}"` → `SqlQuery`. A string will not type-check (S1) |
| `echo $name` | Templates auto-escape. `unsafe_raw` is needed for raw output (S3) |
| `==` juggles types; `"0e1"=="0e2"` | One equality, same types only (L3, L4) |
| `strict_types` opt-in per file | Strict is the only mode (L3) |
| `display_errors=On` leaks stack traces | Production cannot render traces (S11) |
| `md5($pw)`, `rand()` tokens | `password.hash`, CSPRNG by default (S6, S7) |
| Composer plugins run code on install | No install-time code, ever (P1) |

---

## 7. Open questions for the design team

1. **Memory model**: report 03 already proposes GC semantics (host GC on JS, Nim-style ARC/ORC or Boehm on C) with the JS backend first. That choice satisfies L1 **as long as** the C runtime and every `extern` binding are treated as `unsafe` and fuzzed (G2). The remaining question is data-race freedom (L11) once threads arrive. Report 03 defers shared-memory threads, which is the right default.
2. **Capability granularity**: are compile-time capabilities per function signature (Austral) usable for beginners, or do we need per-package manifests plus inference?
3. **Arabic confusable table**: we need to curate our own table beyond UTS #39 (yeh/keheh/heh variants, Persian/Urdu letters). This needs a linguist review.
4. **Franco keyword list**: it must be designed alongside the lexer to guarantee that no keyword collides with a numeric-literal form.
5. **Canonical skin for signing and diffs**: English, or a neutral AST text form?

---

## Sources (consolidated)

- PHP: [PHP 5.4 incompatible changes (mirror)](https://www.cs.auckland.ac.nz/references/php/2012/migration54.incompatible.html) · [RFC: magic quotes](https://wiki.php.net/rfc/magicquotes) · [RFC: Saner string to number comparisons](https://wiki.php.net/rfc/string_to_number_comparison) · [RFC: Saner numeric strings](https://wiki.php.net/rfc/saner-numeric-strings) · [LWN on PHP 8 comparisons](https://lwn.net/Articles/834396/) · [PHPGGC](https://github.com/ambionics/phpggc)
- OWASP Top 10:2025: [The Register](https://www.theregister.com/2025/11/11/new_owasp_top_ten_broken/) · [Qualys](https://blog.qualys.com/qualys-insights/2026/06/15/what-changed-in-owasp-top-10-2025-and-recommendations-for-each-category) · [Kiteworks](https://www.kiteworks.com/cybersecurity-risk-management/owasp-top-10-2025-vulnerabilities/) · [CWE Top 25](https://cwe.mitre.org/top25/)
- Memory safety: [MSRC 2019](https://msrc.microsoft.com/blog/2019/07/a-proactive-approach-to-more-secure-code/) · [Chromium](https://www.chromium.org/Home/chromium-security/memory-safety/) · [Google Android 2024](https://security.googleblog.com/2024/09/eliminating-memory-safety-vulnerabilities-Android.html) · [The Register 2024](https://www.theregister.com/2024/09/25/google_rust_safe_code_android/) · [The Hacker News 2024](https://thehackernews.com/2024/09/googles-shift-to-rust-programming-cuts.html) · [Privacy Guides 2025](https://www.privacyguides.org/news/2025/11/18/google-reports-less-than-20-of-new-vulnerabilities-in-android-are-memory-safety-related/) · [CISA Memory Safe Roadmaps](https://www.cisa.gov/resources-tools/resources/case-memory-safe-roadmaps) · [CISA/FBI Product Security Bad Practices](https://www.cisa.gov/resources-tools/resources/product-security-bad-practices) · [TechRepublic on 2026 deadline](https://techrepublic.com/article/cisa-fbi-memory-safety-recommendations) · [CISA June 2025 MSL CSI](https://www.cisa.gov/news-events/alerts/2025/06/24/new-guidance-released-reducing-memory-related-vulnerabilities) · [NSA press release](https://www.nsa.gov/Press-Room/Press-Releases-Statements/Press-Release-View/Article/4223298/) · [OpenSSF compiler hardening](https://best.openssf.org/Compiler-Hardening-Guides/Compiler-Options-Hardening-Guide-for-C-and-C++)
- Injection-safe types: [Kern, CACM 2014](https://cacm.acm.org/research/securing-the-tangled-web/) · [Google safe-html-types](https://github.com/google/safe-html-types) · [W3C Trusted Types](https://w3c.github.io/trusted-types/dist/spec/) · [web.dev Trusted Types](https://web.dev/articles/trusted-types) · [Go html/template](https://pkg.go.dev/html/template) · [sqlx](https://github.com/launchbadge/sqlx) · [PEP 750](https://peps.python.org/pep-0750/)
- Capabilities: [Deno security](https://docs.deno.com/runtime/fundamentals/security/) · [WASI](https://wasi.dev/) · [Pony](https://www.ponylang.io/) · [Austral](https://austral-lang.org/)
- Secrets/crypto: [Rust secrecy](https://docs.rs/secrecy)
- Supply chain: [Shai-Hulud, Black Duck](https://blackduck.com/blog/npm-malware-attack-shai-hulud-threat.html) · [Cycode](https://cycode.com/blog/shai-hulud-npm-supply-chain-attack/) · [Blackpoint](https://blackpointcyber.com/blog/inside-the-shai-hulud-npm-supply-chain-attack/) · [pnpm 10 release](https://newreleases.io/project/npm/pnpm/release/10.0.0) · [CVE-2025-69264 (OSV)](https://osv.dev/vulnerability/CVE-2025-69264) · [GitHub npm auth changes](https://github.blog/changelog/2025-09-29-strengthening-npm-security-important-changes-to-authentication-and-token-management/) · [xz CVE-2024-3094](https://nvd.nist.gov/vuln/detail/CVE-2024-3094) · [Log4Shell CVE-2021-44228](https://nvd.nist.gov/vuln/detail/CVE-2021-44228) · [Go checksum DB](https://go.dev/ref/mod#checksum-database) · [Sigstore](https://www.sigstore.dev/) · [SLSA](https://slsa.dev/) · [Reproducible Builds](https://reproducible-builds.org/) · [OSV](https://osv.dev/)
- Unicode: [Trojan Source](https://trojansource.codes/) · [Help Net Security](https://www.helpnetsecurity.com/2021/11/02/supply-chain-attacks-source-code/) · [The Hacker News](https://thehackernews.com/2021/11/new-trojan-source-technique-lets.html) · [Rust response](https://blog.rust-lang.org/2021/11/01/cve-2021-42574.html) · [UTS #55](https://unicode.org/reports/tr55/) · [Unicode blog on UTS #55](https://blog.unicode.org/2023/09/source-code-handling-preventing.html) · [UAX #31](https://www.unicode.org/reports/tr31/) · [UTS #39](https://www.unicode.org/reports/tr39/)

*Verification note:* The 2025 Android figure, Shai-Hulud package counts and the npm token revocation date were checked through secondary coverage. Confirm them against the primary posts before quoting externally. Some prior-art claims are cited from general knowledge of the linked primary docs and were not re-fetched in this session: Go 1.22 ChaCha8 `math/rand/v2`, Go 1.25 `CrossOriginProtection`, pnpm `minimumReleaseAge`, and the Kern CACM XSS outcome. A quick check before publishing is advised.
