//! The message catalog: one entry per diagnostic code, with a title, a message, help text
//! and an explain-page body in each of the three message languages (`[en, ar, franco]`).
//!
//! Style rules (spec §21.4, report 01 §3.4): plain-language title; first person; positive
//! tone (never "illegal", "invalid", "fatal" or "bad"); say what was found, why it matters
//! and how to fix it.
//!
//! - `message` says what happened and why it matters.
//! - `help` says how to fix it, in one or two sentences.
//! - `explain` is the long form shown by `kalam explain CODE`: the reason behind the rule,
//!   and an example ("Instead of … write …"). Its code examples are written in the reader's
//!   own skin, because an explain page has no file.
//!
//! **Placeholders.** `message` and `help` are templates. A `{name}` placeholder is filled
//! from the diagnostic's args. A keyword placeholder (`{kw_fn}`, `{core_none}`,
//! `{punct_try}`: a `kw_`, `core_` or `punct_` prefix plus a `kalam-skins` concept name) is
//! filled from the keyword table in the *file's* skin when the emitter does not supply it,
//! so code inside a message stays in the file's skin (spec §21.4 rule 1). `explain` bodies
//! use no placeholders. `tests/catalog_completeness.rs` checks every placeholder against the
//! args each emitter supplies.
//!
//! This inline table covers the M2 strict code set (FOUNDER-QUEUE Q24): every spec §27 code
//! in every family except retired ones, plus every code the compiler emits, plus proposed
//! codes §27 does not list yet (`proposed: true`: the type checker's provisional K0600,
//! K0603, K0804, K1105, K1106 and K1108). That includes the M1 P6 set (Q11). It moves to `catalog/{en,ar,franco}.toml` plus `explain/` pages once it
//! grows (ARCHITECTURE.md §12.2). CI parity rule (05 P6): every code has all three languages.
//!
//! ar/franco: pending native review. Every Arabic and Franco text in this table was
//! machine-drafted and has not yet been checked by a native Egyptian Arabic speaker.

use kalam_skins::{Concept, KeywordTable};

use crate::Lang;

pub struct Entry {
    pub code: &'static str,
    pub title: [&'static str; 3],
    /// What happened and why it matters (a template).
    pub message: [&'static str; 3],
    /// How to fix it (a template).
    pub help: [&'static str; 3],
    /// The body of the `kalam explain` page: the reason and an example. No placeholders.
    pub explain: [&'static str; 3],
    /// True for codes this architecture needs but spec §27 does not list yet.
    pub proposed: bool,
}

pub fn lookup(code: &str) -> Option<&'static Entry> {
    CATALOG.iter().find(|e| e.code == code)
}

/// Fills `{name}` placeholders. Braces whose content is not a known argument name are
/// copied unchanged, so templates can mention `\u{...}` literally.
pub fn render_template(template: &str, args: &[(&str, String)]) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        let replaced = after.find('}').and_then(|close| {
            let name = &after[..close];
            args.iter()
                .find(|(n, _)| *n == name)
                .map(|(_, value)| (value, close))
        });
        match replaced {
            Some((value, close)) => {
                out.push_str(value);
                rest = &after[close + 1..];
            }
            None => {
                out.push('{');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// `{name}` placeholders in `template`, in order of first use: a name is one or more ASCII
/// lowercase letters or `_` (the rule [`render_template`] fills; `\u{...}` is not one).
pub fn placeholders(template: &str) -> Vec<&str> {
    let mut out: Vec<&str> = Vec::new();
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        let after = &rest[open + 1..];
        match after.find('}') {
            Some(close)
                if close > 0
                    && after[..close]
                        .bytes()
                        .all(|b| b.is_ascii_lowercase() || b == b'_') =>
            {
                let name = &after[..close];
                if !out.contains(&name) {
                    out.push(name);
                }
                rest = &after[close + 1..];
            }
            _ => rest = after,
        }
    }
    out
}

/// The concept a keyword placeholder stands for: `kw_fn` → `KW_FN`, `core_none` →
/// `CORE_NONE`, `punct_try` → `PUNCT_TRY`. Other names are not keyword placeholders.
pub fn keyword_concept(name: &str) -> Option<Concept> {
    if !(name.starts_with("kw_") || name.starts_with("core_") || name.starts_with("punct_")) {
        return None;
    }
    Concept::from_name(&name.to_ascii_uppercase())
}

/// [`render_template`], then every keyword placeholder the args did not fill is spelled in
/// `skin` (the file's skin for a diagnostic, the reader's skin on an explain page).
pub fn render_in_skin(template: &str, args: &[(&str, String)], skin: kalam_base::Skin) -> String {
    let mut all: Vec<(&str, String)> = args.to_vec();
    for name in placeholders(template) {
        if all.iter().any(|(n, _)| *n == name) {
            continue;
        }
        if let Some(concept) = keyword_concept(name) {
            let spelling = KeywordTable::builtin().spelling(concept, skin).to_string();
            all.push((name, spelling));
        }
    }
    render_template(template, &all)
}

/// Fixed interface words used by the renderers.
#[derive(Clone, Copy, Debug)]
pub enum Ui {
    Try,
    Help,
    LearnMore,
}

pub fn ui(lang: Lang, word: Ui) -> &'static str {
    let texts: [&'static str; 3] = match word {
        Ui::Try => ["Try:", "جرّب:", "Garrab:"],
        Ui::Help => ["Help:", "مساعدة:", "Mosa3da:"],
        Ui::LearnMore => ["Learn more:", "لمعرفة المزيد:", "E3raf aktar:"],
    };
    texts[lang.index()]
}

pub static CATALOG: &[Entry] = &[
    // ---- Source text (SEC01xx, spec §5.1–§5.3, §6.2). ar/franco: pending native review.
    Entry {
        code: "SEC0100",
        title: ["FILE ENCODING", "ترميز الملف", "ENCODING EL FILE"],
        message: [
            "I found {cp} {name} here: a byte order mark (U+FEFF), or bytes that are not UTF-8. Kalam reads source files only as plain UTF-8, so every editor, reviewer and compiler sees exactly the same characters.",
            "وجدت {cp} {name} هنا: علامة ترتيب البايتات (U+FEFF) أو بايتات ليست بترميز UTF-8. يقرأ كلام ملفات الكود بترميز UTF-8 العادي فقط، حتى يرى كل محرر ومراجع ومترجم الحروف نفسها تمامًا.",
            "La2eit {cp} {name} hena: byte order mark (U+FEFF), aw bytes mesh UTF-8. Kalam beye2ra malafat el code UTF-8 3adi bas, 3ashan kol editor w reviewer w compiler yshoofo nafs el 7oroof bel zabt.",
        ],
        help: [
            "Run `kalam fmt` to remove a byte order mark. For other bytes, save the file again from your editor as \"UTF-8\" (not \"UTF-8 with BOM\").",
            "شغّل `kalam fmt` لحذف علامة ترتيب البايتات. أما البايتات الأخرى فاحفظ الملف من جديد في محررك بترميز \"UTF-8\" (وليس \"UTF-8 with BOM\").",
            "Shaghal `kalam fmt` 3ashan yshil el byte order mark. Law fe bytes tanya, e7faz el file tany men el editor b \"UTF-8\" (mesh \"UTF-8 with BOM\").",
        ],
        explain: [
            "Why: a byte order mark is invisible in most editors, and bytes that are not UTF-8 are decoded differently by different tools. Either one means two people can open the same file and see different text. Kalam stops at both, so the bytes on disk are exactly the program you reviewed.\n\nFix: in most editors, choose \"Save with Encoding\" and pick \"UTF-8\". `kalam fmt` removes a byte order mark by itself.",
            "السبب: علامة ترتيب البايتات لا تظهر في معظم المحررات، والبايتات التي ليست UTF-8 تفسّرها الأدوات المختلفة بطرق مختلفة. وفي الحالتين قد يفتح شخصان الملف نفسه فيريان نصين مختلفين. لذلك يتوقف كلام عند الحالتين، فتكون البايتات المحفوظة هي البرنامج الذي راجعته بالضبط.\n\nالحل: في معظم المحررات اختر \"Save with Encoding\" ثم \"UTF-8\". والأمر `kalam fmt` يحذف علامة ترتيب البايتات تلقائيًا.",
            "Leh: el byte order mark mesh bayna f mo3zam el editors, wel bytes elly mesh UTF-8 kol tool beyfhamha b shakl. F el 7altein momken etnein yefta7o nafs el file w kol wa7ed yshoof kalam mokhtalef. 3ashan keda Kalam beyo2af 3and el etnein, fa el bytes elly 3ala el disk heya bel zabt el program elly enta rage3to.\n\nEl 7al: f mo3zam el editors ekhtar \"Save with Encoding\" w ba3dein \"UTF-8\". W `kalam fmt` beyshil el byte order mark lewa7do.",
        ],
        proposed: false,
    },
    Entry {
        code: "SEC0101",
        title: ["TEXT NOT IN NFC", "نص بغير صيغة NFC", "EL NASS MESH NFC"],
        message: [
            "I found {cp} {name} in text that is not in Unicode normalization form NFC. The same letter can be stored as one code point or as a base letter plus combining marks; the two look identical but compare as different text.",
            "وجدت {cp} {name} في نص ليس بصيغة التطبيع NFC في يونيكود. يمكن تخزين الحرف نفسه كنقطة ترميز واحدة، أو كحرف أساسي تتبعه علامات مركّبة، فيبدوان متطابقين لكنهما نصان مختلفان عند المقارنة.",
            "La2eit {cp} {name} f kalam mesh b sheklo el NFC f Unicode. Nafs el 7arf momken yetkhezen code point wa7ed, aw 7arf asasi w ba3do 3alamat murakkaba; el etnein shabah ba3d bel zabt, bas lamma tqarenhom yetle3o mokhtalfeen.",
        ],
        help: [
            "Run `kalam fmt` or `kalam fix`, which rewrite the text in NFC and change nothing else.",
            "شغّل `kalam fmt` أو `kalam fix`، فكلاهما يعيد كتابة النص بصيغة NFC ولا يغيّر شيئًا آخر.",
            "Shaghal `kalam fmt` aw `kalam fix`, hayekteb el kalam b NFC w mesh hayghayar ay 7aga tanya.",
        ],
        explain: [
            "Why: `é` can be stored as U+00E9, or as `e` followed by U+0301. Arabic has the same problem: `أ` can be one code point, or `ا` followed by a combining hamza (U+0654). If both forms were accepted, two names that look the same could be two different names. Kalam requires NFC, so each visible text has exactly one spelling.\n\nFix: `kalam fmt` normalizes the file. Many editors can also be set to save in NFC.",
            "السبب: يمكن تخزين `أ` كنقطة ترميز واحدة، أو كحرف `ا` تتبعه همزة مركّبة (U+0654)، وكذلك `é` في الحروف اللاتينية. لو قُبلت الصيغتان لأمكن أن يبدو اسمان متطابقين وهما في الحقيقة اسمان مختلفان. لذلك يشترط كلام صيغة NFC، فيكون لكل نص ظاهر كتابة واحدة فقط.\n\nالحل: الأمر `kalam fmt` يطبّع الملف. ويمكن أيضًا ضبط محررات كثيرة لتحفظ بصيغة NFC.",
            "Leh: `أ` momken tetkhezen code point wa7ed, aw `ا` w ba3daha hamza murakkaba (U+0654), w nafs el kalam f `é`. Law el shakleen masmoo7in, momken esmeen yeb2o shabah ba3d w homa aslan esmeen mokhtalfeen. 3ashan keda Kalam 3ayez NFC, fa kol kalam bayen leeh ketaba wa7da bas.\n\nEl 7al: `kalam fmt` bey3mel normalize lel file. W editors keteer momken tezbotha te7faz b NFC.",
        ],
        proposed: false,
    },
    Entry {
        code: "SEC0102",
        title: ["CONTROL CHARACTER", "حرف تحكم", "CONTROL CHARACTER"],
        message: [
            "I found the control character {cp} {name} here. Control characters are invisible, and terminals and editors each handle them differently.",
            "وجدت حرف التحكم {cp} {name} هنا. حروف التحكم لا تُرى، وكل طرفية ومحرر يتعامل معها بطريقة مختلفة.",
            "La2eit control character {cp} {name} hena. El control characters mesh bayna, w kol terminal w editor beyt3amel ma3aha b shakl.",
        ],
        help: [
            "Inside a string, write it as an escape such as `\\u{...}`. Anywhere else, delete it.",
            "داخل النص اكتبه كتسلسل هروب مثل `\\u{...}`، وفي أي مكان آخر احذفه.",
            "Gowa string ektebo escape zay `\\u{...}`, w f ay makan tany emsa7o.",
        ],
        explain: [
            "Why: a control character such as U+0007 (BELL) or U+001B (ESCAPE) does not print, but it can move the cursor, clear the screen or change colors when a file is shown in a terminal. That makes a reviewed file look different from what the compiler reads. Tab, LF and CR have their own rules (K0103, SEC0113).\n\nInstead of a raw control character inside a string, write its escape:\n    let bell = \"\\u{7}\"",
            "السبب: حرف التحكم مثل U+0007 (BELL) أو U+001B (ESCAPE) لا يُطبع، لكنه قد يحرّك المؤشر أو يمسح الشاشة أو يغيّر الألوان حين يُعرض الملف في الطرفية. وهكذا يبدو الملف المراجَع مختلفًا عما يقرؤه المترجم. أما Tab وLF وCR فلها قواعدها الخاصة (K0103 وSEC0113).\n\nبدلًا من حرف تحكم خام داخل النص، اكتب تسلسل الهروب الخاص به:\n    ليكن الجرس = \"\\u{7}\"",
            "Leh: control character zay U+0007 (BELL) aw U+001B (ESCAPE) mesh beyetbe3, bas momken y7arrak el cursor aw yemsa7 el shasha aw yghayar el alwan lamma el file yetfere7 f terminal. Fa el file elly etraga3 yban mokhtalef 3an elly el compiler beye2rah. El Tab wel LF wel CR leehom qawa3ed lewa7dohom (K0103, SEC0113).\n\nBadal control character gowa string, ekteb el escape bta3o:\n    khali garas = \"\\u{7}\"",
        ],
        proposed: false,
    },
    Entry {
        code: "SEC0103",
        title: ["INVISIBLE SPACE", "مسافة غير مرئية", "MASAFA MESH BAYNA"],
        message: [
            "I found {cp} {name}, a space that is not the ordinary space U+0020. Kalam code uses only the ordinary space, so an invisible character can never split or join words.",
            "وجدت {cp} {name}، وهي ليست المسافة العادية U+0020. كود كلام لا يستخدم إلا المسافة العادية، حتى لا يفصل حرف مخفي بين الكلمات أو يدمجها.",
            "La2eit {cp} {name}, w di mesh el masafa el 3adeya U+0020. Kalam beyesta5dem el masafa el 3adeya bas, 3ashan 7arf mestakhabi mayefsselsh aw yelza2 kelmat.",
        ],
        help: [
            "Replace it with an ordinary space. `kalam fix` does this for you.",
            "استبدل بها مسافة عادية. الأمر `kalam fix` يفعل ذلك لك.",
            "Ghayarha b masafa 3adeya. `kalam fix` bey3mel keda lak.",
        ],
        explain: [
            "Why: U+00A0 NO-BREAK SPACE, U+2003 EM SPACE and similar characters look exactly like a space but are different characters. If code could contain them, two lines that look the same could be split into words differently. Inside a string they are allowed, with the warning W0114, because text sometimes needs them.\n\nFix: retype the space, or run `kalam fix`. Text copied from web pages and chat apps often carries these spaces.",
            "السبب: الحرف U+00A0 (مسافة غير فاصلة) والحرف U+2003 وأمثالهما تبدو كالمسافة تمامًا لكنها حروف مختلفة. لو سُمح بها في الكود لأمكن أن يُقسَّم سطران متطابقان في الشكل إلى كلمات بطريقتين مختلفتين. أما داخل النصوص فهي مسموحة مع التحذير W0114، لأن النص قد يحتاج إليها.\n\nالحل: أعد كتابة المسافة، أو شغّل `kalam fix`. النصوص المنسوخة من صفحات الويب وتطبيقات المحادثة تحمل هذه المسافات كثيرًا.",
            "Leh: el 7arf U+00A0 (no-break space) wel 7arf U+2003 w zayohom shabah el masafa bel zabt bas homa 7oroof tanya. Law masmoo7in f el code, momken satrein shabah ba3d yetqasamo l kelmat b tare2tein mokhtalfeen. Gowa el strings masmoo7in ma3 el warning W0114, 3ashan el kalam sa3at beye7tagha.\n\nEl 7al: ekteb el masafa tany, aw shaghal `kalam fix`. El kalam elly mansookh men el web aw el chat ghaleban beykoon feeh el masafat di.",
        ],
        proposed: false,
    },
    Entry {
        code: "SEC0104",
        title: ["NAMES THAT LOOK THE SAME", "اسمان متطابقان في الشكل", "ESMEEN SHABAH BA3D"],
        message: [
            "{name} and {other} are two different names in this scope that look the same once lookalike letters are folded together (for example `ي` and `ى`, `ك` and `ک`, or a Latin letter and its Cyrillic twin). A reader can't tell which of the two a line uses.",
            "{name} و{other} اسمان مختلفان في هذا النطاق يتطابقان في الشكل بعد توحيد الحروف المتشابهة (مثل `ي` و`ى`، أو `ك` و`ک`، أو حرف لاتيني وشبيهه السيريلي). لا يستطيع القارئ أن يعرف أي الاسمين يستخدمه السطر.",
            "{name} w {other} esmeen mokhtalfeen f el scope da bas shabah ba3d ba3d ma el 7oroof el shabah ba3d tetwa7ad (zay `ي` w `ى`, aw `ك` w `ک`, aw 7arf latini w el 7arf el cyrillic elly shabaho). Elly beye2ra mesh hay3raf el satr beyesta5dem anhy esm.",
        ],
        help: [
            "Rename one of the two names so they differ in a way a reader can see, and update its uses.",
            "غيّر اسم أحدهما بحيث يختلفان اختلافًا يراه القارئ، ثم عدّل المواضع التي يُستخدم فيها.",
            "Ghayar esm wa7ed fehom 3ashan yeb2o mokhtalfeen b shakl bayen, w 3addel el amaken elly beyesta5demo.",
        ],
        explain: [
            "Why: Unicode has many letters that are drawn alike. For each name Kalam computes a skeleton that folds lookalikes together (including the Persian and Urdu letters ی ک ە ھ ۀ, decision D19), and it refuses two different names with the same skeleton in one scope. Otherwise a second variable could be declared that looks exactly like the first.\n\nInstead of (one name ends in `ي`, the other in `ى`):\n    let علي = 1\n    let على = 2\n\nwrite:\n    let ali = 1\n    let ala = 2",
            "السبب: في يونيكود حروف كثيرة تُرسم بالشكل نفسه. يحسب كلام لكل اسم هيكلًا يوحّد الحروف المتشابهة (ومنها الحروف الفارسية والأردية ی ک ە ھ ۀ، القرار D19)، ويرفض اسمين مختلفين لهما الهيكل نفسه في نطاق واحد. وإلا أمكن تعريف متغير ثانٍ يشبه الأول تمامًا.\n\nبدلًا من:\n    ليكن علي = 1\n    ليكن على = 2\n\nاكتب:\n    ليكن علي = 1\n    ليكن الأعلى = 2",
            "Leh: f Unicode 7oroof keteer bteteresem nafs el shakl. Kalam bey7seb l kol esm skeleton beywa7ad el 7oroof el shabah ba3d (w menhom el 7oroof el farsy wel urdu ی ک ە ھ ۀ, qarar D19), w beyrfod esmeen mokhtalfeen leehom nafs el skeleton f scope wa7ed. Gheir keda momken tet3arraf variable tany shabah el awel bel zabt.\n\nBadal (wa7ed bey5las b `ي` wel tany b `ى`):\n    khali علي = 1\n    khali على = 2\n\nEkteb:\n    khali ali = 1\n    khali ala = 2",
        ],
        proposed: false,
    },
    Entry {
        code: "SEC0105",
        title: ["MIXED SCRIPTS IN A NAME", "اسم بأكثر من نظام كتابة", "ESM BE AKTAR MEN ABGADEYA"],
        message: [
            "The name `{word}` mixes letters from more than one script: {cp} {name} comes from a different script than the letters before it. Mixing scripts is the usual way to build a name that looks like a different one.",
            "الاسم `{word}` يخلط حروفًا من أكثر من نظام كتابة: {cp} {name} من نظام كتابة غير نظام الحروف التي قبله. وخلط أنظمة الكتابة هو الطريقة المعتادة لصنع اسم يشبه اسمًا آخر.",
            "El esm `{word}` khalat 7oroof men aktar men abgadeya: {cp} {name} men abgadeya gheir el 7oroof elly 2ablo. W khalt el abgadeyat howa el tare2a el ma3roofa 3ashan te3mel esm shabah esm tany.",
        ],
        help: [
            "Write the name in one script. Digits and `_` go with any script. To show Arabic names in an English-skin project, use a display glossary (`names.toml`) instead.",
            "اكتب الاسم بنظام كتابة واحد. الأرقام و`_` تصلح مع أي نظام. ولعرض أسماء عربية في مشروع بالواجهة الإنجليزية استخدم مسرد العرض (`names.toml`) بدلًا من ذلك.",
            "Ekteb el esm b abgadeya wa7da. El arqam w `_` maashya ma3 ay abgadeya. W law 3ayez asami 3arabi tban f project English, esta5dem el glossary (`names.toml`) badal keda.",
        ],
        explain: [
            "Why: `paypal` spelled with the Cyrillic letter U+0430 in place of the Latin `a` looks identical to the real `paypal` on screen. Kalam requires each name to use a single script (Unicode \"single script\", plus `_` and ASCII digits), so lookalike names can't be built by mixing alphabets. `user_اسم` is mixed too, even though both halves are readable.\n\nInstead of:\n    let user_اسم = \"Sara\"\n\nwrite one of:\n    let user_name = \"Sara\"\n    let اسم_المستخدم = \"Sara\"",
            "السبب: الاسم `paypal` المكتوب بالحرف السيريلي U+0430 مكان الحرف اللاتيني `a` يبدو مطابقًا تمامًا لـ `paypal` الحقيقي. لذلك يشترط كلام أن يُكتب كل اسم بنظام كتابة واحد (مع `_` وأرقام ASCII)، فلا يمكن صنع أسماء متشابهة بخلط الأبجديات. والاسم `user_اسم` مختلط أيضًا، حتى لو كان كل جزء منه مقروءًا.\n\nبدلًا من:\n    ليكن user_اسم = \"Sara\"\n\nاكتب أحد الاسمين:\n    ليكن اسم_المستخدم = \"Sara\"\n    ليكن user_name = \"Sara\"",
            "Leh: `paypal` elly maktooba bel 7arf el cyrillic U+0430 makan el `a` el latini shabah `paypal` el 7a2i2eya bel zabt 3ala el shasha. 3ashan keda Kalam 3ayez kol esm yetketeb b abgadeya wa7da (ma3 `_` w arqam ASCII), fa mayenfa3sh te3mel asami shabah ba3d b khalt el 7oroof. W `user_اسم` makhloot bardo, 7atta law kol 7etta feeh mafhooma.\n\nBadal:\n    khali user_اسم = \"Sara\"\n\nEkteb wa7ed men dool:\n    khali user_name = \"Sara\"\n    khali اسم_المستخدم = \"Sara\"",
        ],
        proposed: false,
    },
    Entry {
        code: "SEC0106",
        title: ["MARK INSIDE A NAME", "علامة داخل اسم", "3ALAMA GOWA ESM"],
        message: [
            "{cp} {name} can't appear inside a keyword or a name, because it changes how a word looks without changing what it is.",
            "لا يمكن أن يظهر {cp} {name} داخل كلمة محجوزة أو اسم، لأنه يغيّر شكل الكلمة دون أن يغيّر ما هي.",
            "{cp} {name} mayenfa3sh gowa keyword aw esm, 3ashan beyghayar shakl el kelma men gher ma yghayar heya eh.",
        ],
        help: [
            "Delete the mark, or replace a presentation form with the ordinary letter. `kalam fix` does this for you.",
            "احذف العلامة، أو استبدل بالحرف المعروض شكلًا الحرفَ العادي. الأمر `kalam fix` يفعل ذلك لك.",
            "Emsa7 el 3alama, aw ghayar el presentation form bel 7arf el 3adi. `kalam fix` bey3mel keda lak.",
        ],
        explain: [
            "Why: tatweel (ـ), harakat such as fatha and shadda, the superscript alef, Quranic marks and Arabic presentation forms all change how a word is drawn. If names could contain them, `كتاب` and `كـتـاب` would be two different names that read the same. Names and keywords are written with plain letters; harakat are welcome inside strings and comments. (ZWJ and ZWNJ in a name are SEC0111.)\n\nInstead of:\n    let كـتـاب = 1\n\nwrite:\n    let كتاب = 1",
            "السبب: التطويل (ـ) والحركات مثل الفتحة والشدة والألف الخنجرية وعلامات المصحف وأشكال العرض العربية كلها تغيّر رسم الكلمة. لو سُمح بها في الأسماء لصار `كتاب` و`كـتـاب` اسمين مختلفين يُقرآن بالطريقة نفسها. لذلك تُكتب الأسماء والكلمات المحجوزة بحروف مجردة، أما الحركات فمرحّب بها داخل النصوص والتعليقات. (ZWJ وZWNJ داخل الاسم هما SEC0111.)\n\nبدلًا من:\n    ليكن كـتـاب = 1\n\nاكتب:\n    ليكن كتاب = 1",
            "Leh: el tatweel (ـ) wel 7arakat zay el fat7a wel shadda wel alef el khangareya w 3alamat el mos7af w ashkal el 3ard el 3arabi kollohom beyghayaro rasm el kelma. Law masmoo7in f el asami, `كتاب` w `كـتـاب` yeb2o esmeen mokhtalfeen beyet2ro zay ba3d. 3ashan keda el asami wel keywords btetketeb b 7oroof 3adeya, wel 7arakat ahlan beeha gowa el strings wel comments. (ZWJ w ZWNJ gowa esm yeb2o SEC0111.)\n\nBadal:\n    khali كـتـاب = 1\n\nEkteb:\n    khali كتاب = 1",
        ],
        proposed: false,
    },
    Entry {
        code: "SEC0108",
        title: ["GLOSSARY NAME LOOKS LIKE ANOTHER", "اسم المسرد يشبه اسمًا آخر", "ESM EL GLOSSARY SHABAH ESM TANY"],
        message: [
            "A display name in the project glossary (`names.toml`) has the same skeleton as another identifier, keyword or core name in scope. Readers of the glossary view could not tell the two apart.",
            "اسم عرض في مسرد المشروع (`names.toml`) له الهيكل نفسه لمعرّف آخر أو كلمة محجوزة أو اسم أساسي في النطاق. ولن يستطيع من يقرأ بواجهة المسرد أن يفرّق بينهما.",
            "Esm 3ard f glossary el project (`names.toml`) leeh nafs el skeleton bta3 esm tany aw keyword aw core name f el scope. Elly beye2ra bel glossary mesh hayefre2 benhom.",
        ],
        help: [
            "Choose a different display name for this entry in `names.toml`.",
            "اختر اسم عرض مختلفًا لهذا البند في `names.toml`.",
            "Ekhtar esm 3ard tany lel entry di f `names.toml`.",
        ],
        explain: [
            "Why: a glossary (spec §4.5) shows identifiers in another language without changing the file. Each display name must still be unique by skeleton, or the glossary view would show two different things with one name. This is the glossary version of SEC0104.\n\nInstead of (in `names.toml`, where `على` is already a name in scope):\n    on_top = \"علي\"\n\nwrite:\n    on_top = \"في_الأعلى\"",
            "السبب: المسرد (القسم 4.5 من المواصفة) يعرض المعرّفات بلغة أخرى دون أن يغيّر الملف. ومع ذلك يجب أن يكون كل اسم عرض فريدًا في هيكله، وإلا عرضت واجهة المسرد شيئين مختلفين باسم واحد. هذه هي صورة SEC0104 الخاصة بالمسرد.\n\nبدلًا من (في `names.toml`، و`على` اسم موجود في النطاق):\n    on_top = \"علي\"\n\nاكتب:\n    on_top = \"في_الأعلى\"",
            "Leh: el glossary (spec §4.5) beyfarraj el asami b logha tanya men gher ma yghayar el file. Bas lazem kol esm 3ard yeb2a mokhtalef f el skeleton, gheir keda el glossary hayfarraj 7agtein mokhtalfeen b esm wa7ed. Di nos5et SEC0104 bta3et el glossary.\n\nBadal (f `names.toml`, w `على` esm mawgood f el scope):\n    on_top = \"علي\"\n\nEkteb:\n    on_top = \"في_الأعلى\"",
        ],
        proposed: false,
    },
    Entry {
        code: "SEC0110",
        title: ["HIDDEN TEXT-DIRECTION CHARACTER", "حرف اتجاه مخفي", "7ARF ETEGAH MESTAKHABI"],
        message: [
            "I found {cp} {name}. It changes how the line is displayed without changing how it compiles, which is how Trojan Source attacks work. Arabic text never needs it raw.",
            "وجدت {cp} {name}. هذا الحرف يغيّر طريقة عرض السطر دون أن يغيّر ما يُترجم، وهكذا تعمل هجمات Trojan Source. النص العربي لا يحتاجه خامًا أبدًا.",
            "La2eit {cp} {name}. Da beyghayar shakl el satr men gher ma yghayar el code nafso, w di fekret hagamat Trojan Source. El kalam el 3arabi mesh me7tago khales.",
        ],
        help: [
            "Delete it. If a string really needs it, write it as an escape, `\\u{...}`, so readers can see it.",
            "احذفه. وإذا احتاجه نص فعلًا فاكتبه كتسلسل هروب `\\u{...}` حتى يراه القارئ.",
            "Emsa7o. W law string me7tago bgad, ektebo escape `\\u{...}` 3ashan elly beye2ra yshofo.",
        ],
        explain: [
            "Why: bidirectional controls (U+202A–U+202E, U+2066–U+2069) and marks (LRM, RLM, ALM) reorder how text is drawn. An attacker can make `return` inside a comment look like code, or code look like a comment (CVE-2021-42574, \"Trojan Source\"). Kalam's tools display Arabic right to left by themselves, so source files never need these characters, not even in comments.\n\nInstead of a raw U+2067 inside a string, write:\n    let start_rtl = \"\\u{2067}\"",
            "السبب: حروف التحكم في الاتجاه (U+202A–U+202E وU+2066–U+2069) وعلامات الاتجاه (LRM وRLM وALM) تعيد ترتيب طريقة رسم النص. فيستطيع مهاجم أن يجعل `return` داخل تعليق تبدو كودًا، أو يجعل كودًا يبدو تعليقًا (CVE-2021-42574، \"Trojan Source\"). أدوات كلام تعرض العربية من اليمين إلى اليسار بنفسها، فلا تحتاج ملفات الكود إلى هذه الحروف أبدًا، ولا حتى في التعليقات.\n\nبدلًا من U+2067 خام داخل نص، اكتب:\n    ليكن بداية_الاتجاه = \"\\u{2067}\"",
            "Leh: 7oroof el etegah (U+202A–U+202E w U+2066–U+2069) w 3alamat el etegah (LRM w RLM w ALM) beyrattebo rasm el kalam men gedeed. Fa momken 7ad yekhalli `return` gowa comment tban code, aw code yban comment (CVE-2021-42574, \"Trojan Source\"). Adawat Kalam bet3red el 3arabi men el yemeen lel shemal lewa7daha, fa malafat el code mesh me7taga el 7oroof di khales, wala 7atta f el comments.\n\nBadal U+2067 khaam gowa string, ekteb:\n    khali bedayet_etegah = \"\\u{2067}\"",
        ],
        proposed: false,
    },
    Entry {
        code: "SEC0111",
        title: ["INVISIBLE CHARACTER", "حرف غير مرئي", "7ARF MESH BAYEN"],
        message: [
            "I found the invisible character {cp} {name}. It can make two different names look the same.",
            "وجدت الحرف غير المرئي {cp} {name}. قد يجعل اسمين مختلفين يبدوان متطابقين.",
            "La2eit 7arf mesh bayen {cp} {name}. Momken ykhalli esmeen mokhtalfeen shabah ba3d.",
        ],
        help: [
            "In code or a name, delete it (`kalam fix` does this). Inside a string, write it as `\\u{...}` so readers can see it.",
            "في الكود أو داخل اسم احذفه (الأمر `kalam fix` يفعل ذلك). داخل النص اكتبه على صورة `\\u{...}` حتى يراه القارئ.",
            "F el code aw gowa esm emsa7o (`kalam fix` beyemsa7o). Gowa el string ektebo `\\u{...}` 3ashan elly beye2ra yshofo.",
        ],
        explain: [
            "Why: zero-width characters such as U+200B ZERO WIDTH SPACE, U+200D ZERO WIDTH JOINER (ZWJ), U+200C ZWNJ, U+2060 WORD JOINER and the Hangul fillers take no space on screen. Inside a name they make twins, such as `total` and `to` + U+200B + `tal`, that look identical but are different names. A variation selector is allowed only right after the character it modifies. Emoji sequences with ZWJ are fine inside strings.\n\nInstead of a raw ZWNJ inside a string, write:\n    let joiner = \"\\u{200C}\"",
            "السبب: الحروف عديمة العرض مثل U+200B وU+200D (ZWJ) وU+200C (ZWNJ) وU+2060 وحروف الحشو الكورية لا تأخذ مكانًا على الشاشة. وداخل الاسم تصنع توائم تبدو متطابقة وهي أسماء مختلفة. ومحدد الصيغة (variation selector) مسموح فقط بعد الحرف الذي يعدّله مباشرة. أما تسلسلات الإيموجي مع ZWJ فمقبولة داخل النصوص.\n\nبدلًا من ZWNJ خام داخل نص، اكتب:\n    ليكن الفاصل = \"\\u{200C}\"",
            "Leh: el 7oroof elly malhash 3ard zay U+200B w U+200D (ZWJ) w U+200C (ZWNJ) w U+2060 w 7oroof el 7ashw el koreya mesh bta5od makan 3ala el shasha. Gowa esm bte3mel taw2am shabah ba3d bel zabt w homa asami mokhtalfa. El variation selector masmoo7 bas ba3d el 7arf elly beyghayaro 3ala tool. W el emoji elly feeha ZWJ maashya gowa el strings.\n\nBadal ZWNJ khaam gowa string, ekteb:\n    khali fasel = \"\\u{200C}\"",
        ],
        proposed: false,
    },
    Entry {
        code: "SEC0112",
        title: ["HIDDEN OR UNASSIGNED CHARACTER", "حرف مخفي أو غير معرّف", "7ARF MESTAKHABI"],
        message: [
            "I found {cp}, a tag character, private-use character, noncharacter or unassigned code point. It has no visible meaning in source code.",
            "وجدت {cp}، وهو حرف وسم أو حرف للاستخدام الخاص أو حرف محجوز أو نقطة ترميز غير معرّفة. ليس له معنى ظاهر في الكود.",
            "La2eit {cp}, w da tag character aw private use aw noncharacter aw code point mesh ma3roof. Malhosh ma3na bayen f el code.",
        ],
        help: [
            "Delete it. If a string really needs it, write it as `\\u{...}`.",
            "احذفه. وإذا احتاجه نص فعلًا فاكتبه على صورة `\\u{...}`.",
            "Emsa7o. W law string me7tago bgad, ektebo `\\u{...}`.",
        ],
        explain: [
            "Why: Unicode tag characters (U+E0000–U+E007F) mirror ASCII but are invisible, so they can smuggle hidden instructions into a file, for example to an AI assistant that reads it. Private-use and unassigned code points look different, or like nothing at all, depending on the font. Kalam accepts only characters assigned in Unicode 16.0.\n\nFix: delete the character. Text pasted from chat tools or AI output is the usual source.",
            "السبب: حروف الوسم في يونيكود (U+E0000–U+E007F) تقابل حروف ASCII لكنها لا تُرى، فيمكن أن تُهرَّب بها تعليمات مخفية إلى الملف، مثلًا إلى مساعد ذكاء اصطناعي يقرؤه. أما حروف الاستخدام الخاص ونقاط الترميز غير المعرّفة فيختلف شكلها حسب الخط، وقد لا يظهر لها شيء. لذلك لا يقبل كلام إلا الحروف المعرّفة في يونيكود 16.0.\n\nالحل: احذف الحرف. مصدره المعتاد نص ملصوق من أدوات المحادثة أو من مخرجات الذكاء الاصطناعي.",
            "Leh: tag characters f Unicode (U+E0000–U+E007F) zay 7oroof ASCII bas mesh bayna, fa momken yetharrab beeha ta3leemat mestakhabeya gowa el file, masalan l AI assistant beye2rah. W el private use wel code points el mesh ma3roofa shakleha beyetghayar 7asab el font, w momken mayebanlhash 7aga. 3ashan keda Kalam beyqbal bas el 7oroof el ma3roofa f Unicode 16.0.\n\nEl 7al: emsa7 el 7arf. Ghaleban beyeegi men kalam mansookh men chat aw men AI.",
        ],
        proposed: false,
    },
    Entry {
        code: "SEC0113",
        title: ["UNUSUAL LINE BREAK", "فاصل أسطر غير معتاد", "NEHAYET SATR GHARIBA"],
        message: [
            "I found {cp} {name}. Kalam lines end with LF, or with CRLF, which `kalam fmt` rewrites as LF.",
            "وجدت {cp} {name}. أسطر كلام تنتهي بـ LF، أو بـ CRLF الذي يحوّله `kalam fmt` إلى LF.",
            "La2eit {cp} {name}. Satr Kalam beyekhlas b LF, aw b CRLF elly `kalam fmt` beyhawwelo LF.",
        ],
        help: [
            "Replace it with an ordinary line break (LF). Inside a string, write `\\n` instead.",
            "استبدل به فاصل أسطر عاديًا (LF). وداخل النص اكتب `\\n` بدلًا منه.",
            "Ghayaro b satr gedeed 3adi (LF). W gowa el string ekteb `\\n` badalo.",
        ],
        explain: [
            "Why: U+2028 LINE SEPARATOR, U+2029 PARAGRAPH SEPARATOR and a lone CR are shown as a new line by some editors and not by others. Code that looks like two lines in review could be one line to the compiler, or the other way around. Kalam counts lines by LF only.\n\nFix: retype the line break, or set your editor to use LF line endings.",
            "السبب: الحرفان U+2028 وU+2029 وحرف CR المنفرد تعرضها بعض المحررات كسطر جديد ولا تعرضها محررات أخرى. فقد يبدو الكود سطرين عند المراجعة وهو سطر واحد عند المترجم، أو العكس. لذلك لا يعدّ كلام الأسطر إلا بحرف LF.\n\nالحل: أعد كتابة فاصل السطر، أو اضبط محررك ليستخدم نهايات الأسطر LF.",
            "Leh: U+2028 w U+2029 w CR lewa7do, ba3d el editors betfarrajhom satr gedeed w ba3dohom la2. Fa momken el code yban satrein f el review w howa satr wa7ed 3and el compiler, aw el 3aks. 3ashan keda Kalam bey3od el satr b LF bas.\n\nEl 7al: ekteb nehayet el satr tany, aw ezbot el editor yesta5dem LF.",
        ],
        proposed: false,
    },
    Entry {
        code: "SEC0120",
        title: ["MIXED DIGITS", "أرقام مختلطة", "ARQAM MAKHLOUTA"],
        message: [
            "The number `{literal}` mixes two digit systems, or uses the other system's decimal separator (it contains {cp}). A reader could misread its value.",
            "الرقم `{literal}` يخلط بين نظامين للأرقام، أو يستخدم الفاصلة العشرية الخاصة بالنظام الآخر (فيه {cp})، فقد يُخطئ القارئ في قيمته.",
            "El rakam `{literal}` khalat ben nezameen arqam, aw esta5dem el fasla el 3ashreya bta3et el nezam el tany (feeh {cp}), fa momken elly beye2ra yefham qeemto ghalat.",
        ],
        help: [
            "Write each number in one system: ASCII digits with `.`, or Arabic-Indic digits with `٫`.",
            "اكتب كل رقم بنظام واحد: أرقام ASCII مع `.`، أو أرقام عربية هندية مع `٫`.",
            "Ekteb kol rakam b nezam wa7ed: ASCII ma3 `.`, aw arqam 3arabeya ma3 `٫`.",
        ],
        explain: [
            "Why: `1٢3` mixes ASCII and Arabic-Indic digits, and `1٫5` puts the Arabic decimal separator after an ASCII digit. Inside an Arabic sentence such a number can be read in more than one order, so Kalam keeps every literal in a single system.\n\nInstead of:\n    let price = 1٫5\n\nwrite:\n    let price = 1.5",
            "السبب: الرقم `1٢3` يخلط أرقام ASCII بالأرقام العربية الهندية، والرقم `1٫5` يضع الفاصلة العشرية العربية بعد رقم ASCII. وداخل جملة عربية قد يُقرأ مثل هذا الرقم بأكثر من ترتيب، لذلك يُبقي كلام كل قيمة حرفية بنظام واحد.\n\nبدلًا من:\n    ليكن السعر = 1٫5\n\nاكتب:\n    ليكن السعر = 1.5\n\nأو في ملف عربي يستخدم الأرقام العربية الهندية:\n    ليكن السعر = ١٫٥",
            "Leh: `1٢3` khalat arqam ASCII bel arqam el 3arabeya, w `1٫5` 7atet el fasla el 3ashreya el 3arabi ba3d rakam ASCII. W gowa gomla 3arabi el rakam da momken yet2ery b aktar men tarteeb, 3ashan keda Kalam bey5alli kol literal b nezam wa7ed.\n\nBadal:\n    khali se3r = 1٫5\n\nEkteb:\n    khali se3r = 1.5",
        ],
        proposed: false,
    },
    Entry {
        code: "SEC0121",
        title: ["PERSIAN DIGITS", "أرقام فارسية", "ARQAM FARSY"],
        message: [
            "`{literal}` uses Persian digits such as {cp} (۰–۹), which look almost the same as the Arabic-Indic digits ٠–٩ but are different characters: ۴ and ٤, ۶ and ٦ are easy to confuse.",
            "`{literal}` يستخدم الأرقام الفارسية مثل {cp} (۰–۹)، وهي تشبه الأرقام العربية الهندية ٠–٩ جدًا لكنها حروف مختلفة، فمن السهل الخلط بين ۴ و٤، وبين ۶ و٦.",
            "`{literal}` feeh arqam farsy zay {cp} (۰–۹), w di shabah el arqam el 3arabeya ٠–٩ awy bas 7oroof tanya, w sahl telkhbat ben ۴ w ٤, w ben ۶ w ٦.",
        ],
        help: [
            "Use ASCII digits, or the Arabic-Indic digits ٠–٩ in an Arabic-skin file.",
            "استخدم أرقام ASCII، أو الأرقام العربية الهندية ٠–٩ في ملف بالواجهة العربية.",
            "Esta5dem arqam ASCII, aw ٠–٩ f file 3arabi.",
        ],
        explain: [
            "Why: Persian keyboards type ۰–۹ (U+06F0–U+06F9). Several of them are drawn exactly like Arabic-Indic digits in many fonts, while ۴, ۵ and ۶ are drawn differently. A reviewer can't reliably tell which value a literal has, so Kalam accepts only ASCII digits, and Arabic-Indic digits in Arabic-skin files.\n\nInstead of:\n    let count = ۱۲\n\nwrite:\n    let count = 12",
            "السبب: لوحات المفاتيح الفارسية تكتب ۰–۹ (U+06F0–U+06F9). بعض هذه الأرقام يُرسم في خطوط كثيرة مثل الأرقام العربية الهندية تمامًا، بينما يُرسم ۴ و۵ و۶ بشكل مختلف. فلا يستطيع المراجع أن يعرف قيمة الرقم بثقة، لذلك لا يقبل كلام إلا أرقام ASCII، والأرقام العربية الهندية في ملفات الواجهة العربية.\n\nبدلًا من:\n    ليكن العدد = ۱۲\n\nاكتب:\n    ليكن العدد = ١٢",
            "Leh: keyboards el farsy btekteb ۰–۹ (U+06F0–U+06F9). Ba3dohom beyetresem f fonts keteer zay el arqam el 3arabeya bel zabt, w ۴ w ۵ w ۶ beyetresmo b shakl tany. Fa elly beyraga3 mesh hay3raf qeemet el rakam b sohoola, 3ashan keda Kalam beyqbal bas arqam ASCII, wel arqam el 3arabeya f malafat el 3arabi.\n\nBadal:\n    khali talabat = ۱۲\n\nEkteb:\n    khali talabat = 12",
        ],
        proposed: false,
    },
    Entry {
        code: "SEC0122",
        title: ["ARABIC-INDIC DIGITS", "أرقام عربية هندية", "ARQAM 3ARABEYA"],
        message: [
            "`{literal}` uses Arabic-Indic digits such as {cp}, which only Arabic-skin files (`.ar.kl`) accept. In this file, numbers use ASCII digits.",
            "`{literal}` مكتوب بأرقام عربية هندية مثل {cp}، ولا تقبلها إلا ملفات الواجهة العربية (`.ar.kl`). أما في هذا الملف فالأرقام تُكتب بأرقام ASCII.",
            "`{literal}` maktoob b arqam 3arabeya zay {cp}, w di bas f malafat el 3arabi (`.ar.kl`). F el file da el arqam btetketeb ASCII.",
        ],
        help: [
            "Write it as `{fixed}`. `kalam fix` does this for you.",
            "اكتبه هكذا: `{fixed}`. الأمر `kalam fix` يفعل ذلك لك.",
            "Ektebo keda: `{fixed}`. `kalam fix` bey3mel keda lak.",
        ],
        explain: [
            "Why: each file has one skin (decision D6). English and Franco files write numbers with ASCII digits; only Arabic files may use ٠–٩. Keeping the digits with the skin means `kalam convert` can switch a file between skins without guessing.\n\nInstead of (in a `.kl` file):\n    let limit = ١٠٠\n\nwrite:\n    let limit = 100",
            "السبب: لكل ملف واجهة واحدة (القرار D6). الملفات الإنجليزية والفرانكو تكتب الأرقام بأرقام ASCII، والملفات العربية وحدها يمكنها استخدام ٠–٩. وربط الأرقام بالواجهة يجعل `kalam convert` ينقل الملف بين الواجهات دون تخمين.\n\nبدلًا من (في ملف `.kl`):\n    let limit = ١٠٠\n\nاكتب:\n    let limit = 100",
            "Leh: kol file leeh skin wa7ed (qarar D6). Malafat el English wel Franco btekteb el arqam ASCII, w malafat el 3arabi bas elly momken testa5dem ٠–٩. W rabt el arqam bel skin beyekhalli `kalam convert` yehawwel el file ben el skins men gher takhmeen.\n\nBadal (f file `.franco.kl`):\n    khali hadd = ١٠٠\n\nEkteb:\n    khali hadd = 100",
        ],
        proposed: false,
    },
    Entry {
        code: "SEC0123",
        title: ["ARABIC THOUSANDS SEPARATOR", "فاصل الآلاف العربي", "FASEL EL ALAF"],
        message: [
            "U+066C (٬) looks like `,` and like `٫`, so a number containing it can be read as a list or as a fraction. Kalam doesn't use it.",
            "الحرف U+066C (٬) يشبه `,` ويشبه `٫`، فقد يُقرأ الرقم الذي يحتويه كقائمة أو ككسر. لذلك لا يستخدمه كلام.",
            "El 7arf U+066C (٬) shabah `,` w shabah `٫`, fa el rakam elly feeh momken yet2ery ka list aw ka kasr. 3ashan keda Kalam mabyesta5demhosh.",
        ],
        help: [
            "Group digits with `_` instead, for example `1_000_000`. `kalam fix` does this for you.",
            "افصل مجموعات الأرقام بـ `_` بدلًا منه، مثل `1_000_000`. الأمر `kalam fix` يفعل ذلك لك.",
            "Efsel el arqam b `_` badalo, masalan `1_000_000`. `kalam fix` bey3mel keda lak.",
        ],
        explain: [
            "Why: in `f(1٬000)` a reader can't tell whether the function gets one argument or two, and in some fonts `٬` is hard to tell from the decimal separator `٫`. The underscore is unambiguous in every skin.\n\nInstead of:\n    let budget = 1٬000٬000\n\nwrite:\n    let budget = 1_000_000",
            "السبب: في `f(1٬000)` لا يعرف القارئ هل تأخذ الدالة معاملًا واحدًا أم اثنين، وفي بعض الخطوط يصعب تمييز `٬` من الفاصلة العشرية `٫`. أما الشرطة السفلية فواضحة في كل الواجهات.\n\nبدلًا من:\n    ليكن الميزانية = ١٬٠٠٠٬٠٠٠\n\nاكتب:\n    ليكن الميزانية = ١_٠٠٠_٠٠٠",
            "Leh: f `f(1٬000)` elly beye2ra mesh 3aref el dala wakhda argument wa7ed walla etnein, w f ba3d el fonts sa3b tefre2 `٬` men el fasla el 3ashreya `٫`. El underscore wade7 f kol el skins.\n\nBadal:\n    khali mizaneya = 1٬000٬000\n\nEkteb:\n    khali mizaneya = 1_000_000",
        ],
        proposed: false,
    },
    Entry {
        code: "SEC0124",
        title: ["DIGIT INSIDE A NAME", "رقم داخل اسم", "RAKAM GOWA ESM"],
        message: [
            "The Arabic-Indic digit {cp} inside a name can look like the letter ا, like `l` or like `1`, so two names could look the same.",
            "الرقم العربي الهندي {cp} داخل الاسم قد يشبه الحرف ا أو `l` أو `1`، فقد يبدو اسمان متطابقين.",
            "El rakam {cp} gowa el esm momken yeb2a shabah el 7arf ا aw `l` aw `1`, fa momken esmeen yeb2o shabah ba3d.",
        ],
        help: [
            "Use an ASCII digit in the name. `kalam fix` does this for you.",
            "استخدم رقم ASCII داخل الاسم. الأمر `kalam fix` يفعل ذلك لك.",
            "Esta5dem rakam ASCII gowa el esm. `kalam fix` bey3mel keda lak.",
        ],
        explain: [
            "Why: `١` (Arabic-Indic one) and `ا` (alef) are nearly the same stroke, and `٥` looks like a Latin `o`. Inside a name that makes lookalike pairs easy, so names use ASCII digits in every skin, including Arabic.\n\nInstead of:\n    let نسخة٢ = 1\n\nwrite:\n    let نسخة2 = 1",
            "السبب: الرقم `١` والألف `ا` يكادان يكونان الخط نفسه، والرقم `٥` يشبه حرف `o` اللاتيني. وداخل الاسم يسهّل هذا صنع أسماء متشابهة، لذلك تستخدم الأسماء أرقام ASCII في كل الواجهات، ومنها العربية.\n\nبدلًا من:\n    ليكن نسخة٢ = 1\n\nاكتب:\n    ليكن نسخة2 = 1",
            "Leh: `١` wel alef `ا` ta2reeban nafs el khatt, w `٥` shabah `o` el latini. W gowa esm da beysahhel te3mel asami shabah ba3d, 3ashan keda el asami btesta5dem arqam ASCII f kol el skins, 7atta el 3arabi.\n\nBadal:\n    khali نسخة٢ = 1\n\nEkteb:\n    khali نسخة2 = 1",
        ],
        proposed: false,
    },
    // ---- Lexing (K01xx, spec §5.5–§5.9). ar/franco: pending native review.
    Entry {
        code: "K0103",
        title: ["TAB CHARACTER", "حرف جدولة", "TAB"],
        message: [
            "I found a tab. Kalam code is indented with 4 spaces per level, so it looks the same in every editor.",
            "وجدت حرف جدولة (tab). الكود في كلام يُزاح بأربع مسافات لكل مستوى، حتى يظهر بالشكل نفسه في كل محرر.",
            "La2eit tab. Kalam beyesta5dem 4 masafat l kol mostawa, 3ashan el code yban zay ba3do f kol editor.",
        ],
        help: [
            "Replace the tab with spaces (4 per level). `kalam fmt` does this for you; inside a string, write `\\t`.",
            "استبدل بحرف الجدولة مسافات (4 لكل مستوى). الأمر `kalam fmt` يفعل ذلك لك، وداخل النص اكتب `\\t`.",
            "Ghayar el tab b masafat (4 l kol mostawa). `kalam fmt` bey3mel keda lak, w gowa el string ekteb `\\t`.",
        ],
        explain: [
            "Why: blocks in Kalam are marked by indentation (decision D15). A tab is 2, 4 or 8 columns wide depending on the editor, so a file that mixes tabs and spaces can look correctly indented and still mean something else. With spaces only, what you see is the block structure the compiler reads.\n\nFix: set your editor to insert spaces when you press Tab, and run `kalam fmt`.",
            "السبب: الكتل في كلام تُحدَّد بالإزاحة (القرار D15). وعرض حرف الجدولة 2 أو 4 أو 8 أعمدة حسب المحرر، فالملف الذي يخلط الجدولة بالمسافات قد يبدو مُزاحًا إزاحة صحيحة وهو يعني شيئًا آخر. ومع المسافات وحدها يكون ما تراه هو بنية الكتل التي يقرؤها المترجم.\n\nالحل: اضبط محررك ليُدخل مسافات عند الضغط على Tab، ثم شغّل `kalam fmt`.",
            "Leh: el blocks f Kalam btet7added bel za7a (qarar D15). W el tab 3ardo 2 aw 4 aw 8 3awameed 7asab el editor, fa el file elly khalat tabs w masafat momken yban maza7 sa7 w howa ma3nah 7aga tanya. Bel masafat bas, elly enta shayfo howa nafs shakl el blocks elly el compiler beye2rah.\n\nEl 7al: ezbot el editor yekteb masafat lamma tedos Tab, w shaghal `kalam fmt`.",
        ],
        proposed: false,
    },
    Entry {
        code: "K0105",
        title: ["UNEXPECTED CHARACTER", "حرف غير متوقع", "7ARF MESH MATWAQA3"],
        message: [
            "I don't know what to do with {cp} {name} here. Outside strings and comments, Kalam code uses letters, digits, spaces and the ASCII operators.",
            "لا أعرف ماذا أفعل بـ {cp} {name} هنا. خارج النصوص والتعليقات، يستخدم كود كلام الحروف والأرقام والمسافات ورموز ASCII.",
            "Mesh 3aref a3mel eh b {cp} {name} hena. Bara el strings wel comments, Kalam beyesta5dem 7oroof w arqam w masafat w romooz ASCII.",
        ],
        help: [
            "Delete the character, or move it into a string or a comment, where any visible character is welcome.",
            "احذف الحرف، أو انقله إلى داخل نص أو تعليق، فهناك يُقبل أي حرف ظاهر.",
            "Emsa7 el 7arf, aw 7otto gowa string aw comment, henak ay 7arf bayen maashy.",
        ],
        explain: [
            "Why: Kalam's operators are ASCII (`+ - * / = < > ( ) [ ] { } : . ,`), plus the Arabic comma `،` and question mark `؟` in Arabic files. Other symbols, such as `@` outside an attribute, `$`, or curly quotes pasted from a word processor, have no meaning in code.\n\nInstead of:\n    let total = price × 2\n\nwrite:\n    let total = price * 2",
            "السبب: رموز العمليات في كلام من ASCII (`+ - * / = < > ( ) [ ] { } : . ,`)، ومعها الفاصلة `،` وعلامة الاستفهام `؟` في الملفات العربية. أما الرموز الأخرى، مثل `$` أو علامات التنصيص المنحنية الملصوقة من معالج النصوص، فليس لها معنى في الكود.\n\nبدلًا من:\n    ليكن المجموع = السعر × 2\n\nاكتب:\n    ليكن المجموع = السعر * 2",
            "Leh: romooz el 3amaleyat f Kalam ASCII (`+ - * / = < > ( ) [ ] { } : . ,`), w ma3aha `،` w `؟` f malafat el 3arabi. Ay romooz tanya, zay `$` aw el quotes el ma3wooga elly mansookha men Word, malhash ma3na f el code.\n\nBadal:\n    khali magmoo3 = se3r × 2\n\nEkteb:\n    khali magmoo3 = se3r * 2",
        ],
        proposed: false,
    },
    Entry {
        code: "K0120",
        title: ["INDENTATION", "الإزاحة", "EL ZA7A"],
        message: [
            "This line's indentation doesn't fit. Each block goes exactly one level (4 spaces) deeper than the line that opens it with `:`.",
            "إزاحة هذا السطر لا تناسب مكانه. كل كتلة تُزاح مستوى واحدًا بالضبط (4 مسافات) أعمق من السطر الذي يفتحها بـ `:`.",
            "Za7et el satr da mesh mazboota. Kol block beykoon mostawa wa7ed bel zabt (4 masafat) agwa men el satr elly beyfta7o b `:`.",
        ],
        help: [
            "Indent this line by a multiple of 4 spaces, one level deeper than the line ending in `:` above it, or line it up with its neighbours. `kalam fmt` fixes indentation it can work out.",
            "أزح هذا السطر بمضاعفات 4 مسافات، أعمق بمستوى واحد من السطر المنتهي بـ `:` فوقه، أو اجعله على محاذاة الأسطر المجاورة. الأمر `kalam fmt` يصلح الإزاحة التي يستطيع استنتاجها.",
            "Zo77 el satr da b mada3afat 4 masafat, mostawa wa7ed agwa men el satr elly bey5las b `:` fo2o, aw zabato ma3 el sotoor elly gambo. `kalam fmt` beysalla7 el za7a elly ye2dar yefhamha.",
        ],
        explain: [
            "Why: indentation is the block structure (decision D15), so it has to be exact. A line indented 2 or 6 spaces, or indented when no `:` opened a block, could belong to more than one block.\n\nInstead of:\n    if ready:\n      start()\n\nwrite:\n    if ready:\n        start()",
            "السبب: الإزاحة هي بنية الكتل (القرار D15)، لذلك يجب أن تكون دقيقة. فالسطر المُزاح مسافتين أو ست مسافات، أو المُزاح دون أن تفتح `:` كتلة قبله، قد ينتمي إلى أكثر من كتلة.\n\nبدلًا من:\n    إذا جاهز:\n      ابدأ()\n\nاكتب:\n    إذا جاهز:\n        ابدأ()",
            "Leh: el za7a heya shakl el blocks (qarar D15), 3ashan keda lazem tekoon mazboota. El satr elly maza7 masafteen aw 6, aw maza7 men gher `:` fata7et block 2ablo, momken yeb2a tabe3 l aktar men block.\n\nBadal:\n    law gahez:\n      ebda2()\n\nEkteb:\n    law gahez:\n        ebda2()",
        ],
        proposed: false,
    },
    Entry {
        code: "K0122",
        title: ["DEDENT", "رجوع الإزاحة", "RAG3ET EL ZA7A"],
        message: [
            "This line moves left to a column that no enclosing block uses, so I can't tell which block it continues.",
            "هذا السطر يرجع إلى عمود لا تستخدمه أي كتلة محيطة، فلا أعرف أي كتلة يكمل.",
            "El satr da rage3 l 3amood mafeesh block beyesta5demo, fa mesh 3aref howa bykammel anhy block.",
        ],
        help: [
            "Line it up exactly with one of the blocks above it.",
            "اجعله على محاذاة إحدى الكتل التي فوقه بالضبط.",
            "Zabato bel zabt ma3 wa7ed men el blocks elly fo2o.",
        ],
        explain: [
            "Why: when a line moves left, it closes blocks until it reaches the block at its column. If no open block starts at that column, the line belongs nowhere, and any guess I made could silently move code into the wrong block.\n\nInstead of:\n    if ready:\n        if late:\n            warn()\n      start()\n\nwrite:\n    if ready:\n        if late:\n            warn()\n        start()",
            "السبب: حين يرجع سطر إلى اليسار فإنه يغلق الكتل حتى يصل إلى الكتلة التي تبدأ عند عموده. فإن لم تبدأ أي كتلة مفتوحة عند هذا العمود فلا مكان للسطر، وأي تخمين مني قد ينقل الكود إلى الكتلة الخطأ دون أن تلاحظ.\n\nبدلًا من:\n    إذا جاهز:\n        إذا متأخر:\n            نبّه()\n      ابدأ()\n\nاكتب:\n    إذا جاهز:\n        إذا متأخر:\n            نبّه()\n        ابدأ()",
            "Leh: lamma satr yerga3 shemal, beye2fel el blocks le7add ma yewsal lel block elly beyebda2 3and 3amoodo. Law mafeesh block maftoo7 beyebda2 3and el 3amood da, yeb2a el satr malosh makan, w ay takhmeen menni momken yenqel el code lel block el ghalat men gher ma takhod balak.\n\nBadal:\n    law gahez:\n        law met2akhar:\n            nabbeh()\n      ebda2()\n\nEkteb:\n    law gahez:\n        law met2akhar:\n            nabbeh()\n        ebda2()",
        ],
        proposed: false,
    },
    Entry {
        code: "K0125",
        title: ["LEADING ZERO", "صفر في البداية", "SEFR F EL AWEL"],
        message: [
            "`{literal}` starts with a zero. Kalam numbers don't start with zeros, so `007` can never be mistaken for an octal number.",
            "`{literal}` يبدأ بصفر. أرقام كلام لا تبدأ بأصفار، حتى لا يُظن أن `007` رقم ثماني.",
            "`{literal}` beyebda2 b sefr. Arqam Kalam mabtebda2sh b asfar, 3ashan `007` mayet2resh octal.",
        ],
        help: [
            "Write `{fixed}`. For an octal number, write it with `0o`. `kalam fix` does this for you.",
            "اكتب `{fixed}`. وللرقم الثماني اكتبه بـ `0o`. الأمر `kalam fix` يفعل ذلك لك.",
            "Ekteb `{fixed}`. W law 3ayez octal ektebo b `0o`. `kalam fix` bey3mel keda lak.",
        ],
        explain: [
            "Why: in C, JavaScript and older Python, `010` means eight, not ten. Kalam refuses the leading zero so the value is never in doubt; octal is always spelled `0o10`.\n\nInstead of:\n    let mode = 0755\n\nwrite:\n    let mode = 0o755",
            "السبب: في لغات C وJavaScript وPython القديمة يعني `010` ثمانية لا عشرة. لذلك يرفض كلام الصفر في البداية حتى لا تكون القيمة موضع شك، ويُكتب الرقم الثماني دائمًا `0o10`.\n\nبدلًا من:\n    ليكن الصلاحيات = 0755\n\nاكتب:\n    ليكن الصلاحيات = 0o755",
            "Leh: f C w JavaScript w Python el 2adeem, `010` ma3naha tamanya mesh 3ashra. 3ashan keda Kalam beyrfod el sefr f el awel 3ashan el qeema matb2ash mashkook feeha, wel octal dayman yetketeb `0o10`.\n\nBadal:\n    khali solta = 0755\n\nEkteb:\n    khali solta = 0o755",
        ],
        proposed: false,
    },
    Entry {
        code: "K0126",
        title: ["NUMBER TOO BIG FOR ITS TYPE", "رقم أكبر من نوعه", "RAKAM AKBAR MEN NO3O"],
        message: [
            "This number doesn't fit in `{type}`. For `{core_int}`, the range is the whole numbers from -9223372036854775808 to 9223372036854775807 (64 bits) on every platform.",
            "هذا الرقم لا يتسع له النوع `{type}`. مدى النوع `{core_int}` هو الأعداد الصحيحة من -9223372036854775808 إلى 9223372036854775807 (64 بت) على كل المنصات.",
            "El rakam da mesh hayeksa3 f `{type}`. Lel `{core_int}`, el mada howa el arqam el sa7ee7a men -9223372036854775808 l 9223372036854775807 (64 bit) 3ala kol platform.",
        ],
        help: [
            "Use a smaller value, or give the value the type `BigInt` for numbers without a limit.",
            "استخدم قيمة أصغر، أو اجعل نوع القيمة `BigInt` للأعداد التي لا حد لها.",
            "Esta5dem qeema as8ar, aw ekhali no3 el qeema `BigInt` lel arqam elly malhash 7add.",
        ],
        explain: [
            "Why: `Int` is a checked 64-bit integer on every backend (decision D18), including JavaScript, where numbers beyond 2^53 would otherwise lose precision without a warning. A literal outside the range would be wrong before the program even runs, so I stop at compile time.\n\nInstead of:\n    let big: Int = 10000000000000000000\n\nwrite:\n    let big: BigInt = 10000000000000000000",
            "السبب: النوع `عدد` عدد صحيح من 64 بت مع فحص للتجاوز على كل المنصات (القرار D18)، ومنها JavaScript التي تفقد فيها الأرقام الأكبر من 2^53 دقتها دون تحذير. والقيمة الحرفية الخارجة عن المدى خاطئة قبل أن يعمل البرنامج أصلًا، لذلك أتوقف عندها وقت الترجمة.\n\nبدلًا من:\n    ليكن الكبير: عدد = 10000000000000000000\n\nاكتب:\n    ليكن الكبير: BigInt = 10000000000000000000",
            "Leh: `adad` 3adad sa7ee7 64 bit metraqab 3ala kol platform (qarar D18), w menhom JavaScript elly el arqam elly akbar men 2^53 fiha bteb2a mesh daqee2a men gher warning. W el literal elly bara el mada ghalat 2abl ma el program yeshtaghal aslan, 3ashan keda bawa2af 3ando w2t el compile.\n\nBadal:\n    khali kebeer: adad = 10000000000000000000\n\nEkteb:\n    khali kebeer: BigInt = 10000000000000000000",
        ],
        proposed: false,
    },
    Entry {
        code: "K0127",
        title: ["NUMBER GLUED TO LETTERS", "رقم ملتصق بحروف", "RAKAM LAZE2 F 7OROOF"],
        message: [
            "`{word}` starts with a digit, so I read it as a number, but it has letters in it. Names can't start with a digit in any skin.",
            "`{word}` يبدأ برقم، فقرأته كرقم، لكنه يحتوي على حروف. لا يمكن أن يبدأ اسم برقم في أي واجهة.",
            "`{word}` beyebda2 b rakam, fa 2areito rakam, bas feeh 7oroof. Mafeesh esm yebda2 b rakam f ay skin.",
        ],
        help: [
            "Start the name with a letter, or put a space between the number and the word. When the word is a misspelled keyword, which happens often in Franco, I suggest the right spelling.",
            "ابدأ الاسم بحرف، أو ضع مسافة بين الرقم والكلمة. وإذا كانت الكلمة كلمة محجوزة مكتوبة خطأً، وهذا يحدث كثيرًا في الفرانكو، أقترح عليك الكتابة الصحيحة.",
            "Ebda2 el esm b 7arf, aw 7ot masafa ben el rakam wel kelma. W law el kelma keyword maktoob ghalat, w da beye7sal keteer f el Franco, ha2tera7 3aleek el ketaba el sa7.",
        ],
        explain: [
            "Why: `2x` could mean `2 * x` or a name, and `1e5` is a number while `1e5x` is not. A name that starts with a digit is always ambiguous, so every skin forbids it. Franco writers often start words with 2, 3 or 7 (`3ala`, `7aga`); the Franco keyword table spells keywords without a leading digit for this reason.\n\nInstead of:\n    let 2nd_place = \"Sara\"\n\nwrite:\n    let second_place = \"Sara\"",
            "السبب: `2x` قد تعني `2 * x` وقد تكون اسمًا، و`1e5` رقم بينما `1e5x` ليس رقمًا. فالاسم الذي يبدأ برقم ملتبس دائمًا، لذلك تمنعه كل الواجهات. وكتّاب الفرانكو يبدؤون كلمات كثيرة بـ 2 أو 3 أو 7، ولهذا تُكتب كلمات الفرانكو المحجوزة دون رقم في أولها.\n\nبدلًا من:\n    ليكن 2المركز = \"Sara\"\n\nاكتب:\n    ليكن المركز_الثاني = \"Sara\"",
            "Leh: `2x` momken tekoon `2 * x` aw esm, w `1e5` rakam laken `1e5x` la2. Fa el esm elly beyebda2 b rakam dayman mel5bat, 3ashan keda kol el skins bteman3o. W elly beyekteb Franco keteer beyebda2 kelmat b 2 aw 3 aw 7 (`3ala`, `7aga`), w 3ashan keda keywords el Franco btetketeb men gher rakam f awelha.\n\nBadal:\n    khali 7aga = 1\n\nEkteb:\n    khali haga = 1",
        ],
        proposed: false,
    },
    Entry {
        code: "K0128",
        title: ["ARABIC-INDIC NUMBER FORM", "صيغة رقم عربي", "SHAKL RAKAM 3ARABI"],
        message: [
            "`{literal}` mixes Arabic-Indic digits with a Latin prefix or exponent. Hex, binary, octal and exponent forms use ASCII digits only.",
            "`{literal}` يخلط أرقامًا عربية هندية ببادئة أو أس لاتيني. الصيغ الست عشرية والثنائية والثمانية والأسية تستخدم أرقام ASCII فقط.",
            "`{literal}` khalat arqam 3arabeya ma3 prefix aw exponent latini. Hex w binary w octal wel exponent bas b arqam ASCII.",
        ],
        help: [
            "Write the whole number with ASCII digits, for example `0xFF` or `1.5e3`.",
            "اكتب الرقم كله بأرقام ASCII، مثل `0xFF` أو `1.5e3`.",
            "Ekteb el rakam kollo b arqam ASCII, zay `0xFF` aw `1.5e3`.",
        ],
        explain: [
            "Why: a Latin `x`, `b`, `o` or `e` in the middle of Arabic-Indic digits makes a token in two scripts, which is exactly what the identifier rules forbid (decision DR-01b). These forms are rare in application code, so they exist only with ASCII digits.\n\nInstead of:\n    let mask = 0x١٠\n\nwrite:\n    let mask = 0x10",
            "السبب: وجود حرف لاتيني مثل `x` أو `b` أو `o` أو `e` وسط أرقام عربية هندية يصنع رمزًا بنظامي كتابة، وهذا بالضبط ما تمنعه قواعد الأسماء (القرار DR-01b). وهذه الصيغ نادرة في كود التطبيقات، لذلك لا توجد إلا بأرقام ASCII.\n\nبدلًا من:\n    ليكن القناع = 0x١٠\n\nاكتب:\n    ليكن القناع = 0x10",
            "Leh: 7arf latini zay `x` aw `b` aw `o` aw `e` f nos arqam 3arabeya beye3mel token b abgadeyteen, w da bel zabt elly qawa3ed el asami bteman3o (qarar DR-01b). W el ashkal di nadra f code el applications, 3ashan keda mawgooda bas b arqam ASCII.\n\nBadal:\n    khali mask = 0x١٠\n\nEkteb:\n    khali mask = 0x10",
        ],
        proposed: false,
    },
    Entry {
        code: "K0129",
        title: ["UNFINISHED STRING", "نص غير مكتمل", "STRING MESH KAMLA"],
        message: [
            "This string has no closing `\"`. A `\"...\"` string ends on the same line it starts on.",
            "هذا النص لا ينتهي بـ `\"`. النص `\"...\"` ينتهي في السطر نفسه الذي يبدأ فيه.",
            "El string di malhash `\"` f el akher. `\"...\"` lazem tekhlas f nafs el satr elly bdaet feeh.",
        ],
        help: [
            "Add the closing `\"` before the end of the line. For text that spans several lines, use `\"\"\"`.",
            "أضف `\"` الختامية قبل نهاية السطر. وللنصوص متعددة الأسطر استخدم `\"\"\"`.",
            "Zawwed el `\"` elly bte2fel 2abl akher el satr. W lel kalam elly 3ala kaza satr esta5dem `\"\"\"`.",
        ],
        explain: [
            "Why: if a string could run on to the next lines, one missing quote would swallow the rest of the file and every error after it would be confusing. Ending strings at the line end keeps the damage to one line.\n\nInstead of:\n    let text = \"first line\n    second line\"\n\nwrite:\n    let text = \"\"\"\n        first line\n        second line\n        \"\"\"",
            "السبب: لو امتد النص إلى الأسطر التالية لابتلعت علامة تنصيص واحدة ناقصة بقية الملف، ولصارت كل الأخطاء بعدها محيّرة. وإنهاء النص عند نهاية السطر يحصر الضرر في سطر واحد.\n\nبدلًا من:\n    ليكن النص = \"السطر الأول\n    السطر الثاني\"\n\nاكتب:\n    ليكن النص = \"\"\"\n        السطر الأول\n        السطر الثاني\n        \"\"\"",
            "Leh: law el string tenfa3 tekammel f el sotoor elly ba3daha, quote wa7da na2sa kanet hatebla3 ba2eit el file, w kol el errors elly ba3daha hateb2a mo7ayara. W enn el string tekhlas m3a akher el satr beye7sor el mashakel f satr wa7ed.\n\nBadal:\n    khali nass_el_resala = \"awel satr\n    tany satr\"\n\nEkteb:\n    khali nass_el_resala = \"\"\"\n        awel satr\n        tany satr\n        \"\"\"",
        ],
        proposed: false,
    },
    Entry {
        code: "K0130",
        title: ["UNKNOWN ESCAPE", "تسلسل هروب غير معروف", "ESCAPE MESH MA3ROOF"],
        message: [
            "I don't know the escape `{escape}`. The escapes are `\\n \\t \\r \\\\ \\\" \\{ \\} \\0` and `\\u{...}`.",
            "لا أعرف تسلسل الهروب `{escape}`. التسلسلات المتاحة هي `\\n \\t \\r \\\\ \\\" \\{ \\} \\0` و`\\u{...}`.",
            "Mesh 3aref el escape `{escape}`. El escapes homa `\\n \\t \\r \\\\ \\\" \\{ \\} \\0` w `\\u{...}`.",
        ],
        help: [
            "Use one of those escapes. For a literal backslash, write `\\\\`; for any other character, write `\\u{...}` with its hexadecimal code point.",
            "استخدم أحد هذه التسلسلات. وللشرطة المائلة العكسية نفسها اكتب `\\\\`، ولأي حرف آخر اكتب `\\u{...}` مع رمزه الست عشري.",
            "Esta5dem wa7ed men el escapes di. W law 3ayez backslash 3adi ekteb `\\\\`, w l ay 7arf tany ekteb `\\u{...}` b el code point bta3o bel hex.",
        ],
        explain: [
            "Why: other languages accept escapes such as `\\x41`, `\\a` or `\\e`, each with slightly different rules. Kalam keeps a short list and spells everything else with `\\u{...}`, so an escape always means the same thing. A Windows path is the usual surprise.\n\nInstead of:\n    let path = \"C:\\data\"\n\nwrite:\n    let path = \"C:\\\\data\"",
            "السبب: لغات أخرى تقبل تسلسلات مثل `\\x41` أو `\\a` أو `\\e`، ولكل منها قواعد مختلفة قليلًا. أما كلام فيحتفظ بقائمة قصيرة، ويكتب كل ما عداها بـ `\\u{...}`، فيكون معنى التسلسل ثابتًا دائمًا. ومسارات ويندوز هي المفاجأة المعتادة.\n\nبدلًا من:\n    ليكن المسار = \"C:\\data\"\n\nاكتب:\n    ليكن المسار = \"C:\\\\data\"",
            "Leh: loghat tanya beteqbal escapes zay `\\x41` aw `\\a` aw `\\e`, w kol wa7da leeha qawa3ed mokhtalfa shewaya. Kalam beyekhalli list so8ayara, w ay 7aga tanya btetketeb b `\\u{...}`, fa el escape dayman ma3nah wa7ed. W el mofag2a el ma3roofa heya masarat Windows.\n\nBadal:\n    khali masar = \"C:\\data\"\n\nEkteb:\n    khali masar = \"C:\\\\data\"",
        ],
        proposed: false,
    },
    Entry {
        code: "K0131",
        title: ["STRING INSIDE A HOLE", "نص داخل فجوة إدراج", "STRING GOWA HOLE"],
        message: [
            "A string literal inside an interpolation hole is hard to read: the quotes inside the quotes make it unclear where the outer string ends.",
            "النص الحرفي داخل فجوة الإدراج صعب القراءة: علامات التنصيص داخل علامات التنصيص تجعل نهاية النص الخارجي غير واضحة.",
            "String gowa el hole sa3b te2raha: el quotes gowa el quotes betkhalli mesh wade7 el string el bara bte5las fein.",
        ],
        help: [
            "Bind the value with `{kw_let}` on the line before, then put the name in the hole.",
            "اربط القيمة بـ `{kw_let}` في السطر السابق، ثم ضع الاسم في الفجوة.",
            "E7faz el qeema b `{kw_let}` f el satr elly 2ablo, w ba3dein 7ot el esm f el hole.",
        ],
        explain: [
            "Why: `\"Hi {greet(\"Sara\")}\"` has four quote marks, and a reader, a syntax highlighter or a quick review can easily pair the wrong ones. Giving the inner value a name keeps each string flat.\n\nInstead of:\n    print(\"Hi {greet(\"Sara\")}\")\n\nwrite:\n    let greeting = greet(\"Sara\")\n    print(\"Hi { greeting }\")",
            "السبب: النص `\"Hi {greet(\"Sara\")}\"` فيه أربع علامات تنصيص، ومن السهل أن يقرنها القارئ أو أداة التلوين أو المراجعة السريعة قرنًا خاطئًا. وإعطاء القيمة الداخلية اسمًا يُبقي كل نص بسيطًا.\n\nبدلًا من:\n    اطبع(\"Hi {رحب(\"Sara\")}\")\n\nاكتب:\n    ليكن الترحيب = رحب(\"Sara\")\n    اطبع(\"Hi {الترحيب}\")",
            "Leh: `\"Hi {greet(\"Sara\")}\"` feeha arba3 quotes, w sahl elly beye2ra aw el highlighter aw el review el sare3 yewsal el quotes ghalat. Lamma tedy el qeema el gowa esm, kol string bteb2a basita.\n\nBadal:\n    ektb(\"Hi {rahhab(\"Sara\")}\")\n\nEkteb:\n    khali tar7eeb = rahhab(\"Sara\")\n    ektb(\"Hi { tar7eeb }\")",
        ],
        proposed: false,
    },
    Entry {
        code: "K0132",
        title: ["LONE SURROGATE", "نصف زوج بديل", "SURROGATE LEWA7DO"],
        message: [
            "`{escape}` is half of a UTF-16 surrogate pair, which is not a character on its own. Kalam strings hold whole Unicode characters.",
            "`{escape}` نصف زوج بديل في UTF-16، وليس حرفًا بمفرده. نصوص كلام تحتوي على حروف يونيكود كاملة.",
            "`{escape}` nos surrogate pair f UTF-16, w da mesh 7arf lewa7do. Strings Kalam feeha 7oroof Unicode kamla.",
        ],
        help: [
            "Write the character's own code point instead, for example `\\u{1F600}` rather than `\\u{D83D}\\u{DE00}`.",
            "اكتب رمز الحرف نفسه بدلًا من ذلك، مثل `\\u{1F600}` بدلًا من `\\u{D83D}\\u{DE00}`.",
            "Ekteb el code point bta3 el 7arf nafso, zay `\\u{1F600}` badal `\\u{D83D}\\u{DE00}`.",
        ],
        explain: [
            "Why: JavaScript and Java store text as UTF-16, so they write emoji as two halves. A Kalam string is a sequence of whole characters, and a lone half would turn into broken text when the string is sent as UTF-8 or JSON.\n\nInstead of:\n    let smile = \"\\u{D83D}\\u{DE00}\"\n\nwrite:\n    let smile = \"\\u{1F600}\"",
            "السبب: JavaScript وJava تخزّنان النص بترميز UTF-16، فتكتبان الإيموجي نصفين. أما نص كلام فسلسلة من حروف كاملة، والنصف المنفرد يتحول إلى نص تالف حين يُرسل النص بترميز UTF-8 أو JSON.\n\nبدلًا من:\n    ليكن الابتسامة = \"\\u{D83D}\\u{DE00}\"\n\nاكتب:\n    ليكن الابتسامة = \"\\u{1F600}\"",
            "Leh: JavaScript w Java bey7fazo el kalam UTF-16, fa beyketbo el emoji nosseen. Laken string Kalam heya 7oroof kamla wara ba3d, wel nos lewa7do beyeb2a kalam bayez lamma el string tetba3at UTF-8 aw JSON.\n\nBadal:\n    khali ebtesama = \"\\u{D83D}\\u{DE00}\"\n\nEkteb:\n    khali ebtesama = \"\\u{1F600}\"",
        ],
        proposed: false,
    },
    // ---- Skins (K02xx, spec §5.4). ar/franco: pending native review.
    Entry {
        code: "K0201",
        title: ["WORD FROM ANOTHER SKIN", "كلمة من واجهة أخرى", "KELMA MEN SKIN TANY"],
        message: [
            "`{found}` belongs to another skin. In this file, write `{expected}`.",
            "`{found}` من واجهة أخرى. في هذا الملف اكتب `{expected}`.",
            "`{found}` men skin tany. F el file da ekteb `{expected}`.",
        ],
        help: [
            "Replace `{found}` with `{expected}`, or run `kalam convert` to rewrite a pasted block. If you meant a name of your own, declare it first or write it in backticks.",
            "استبدل بـ `{found}` الكلمة `{expected}`، أو شغّل `kalam convert` لإعادة كتابة كتلة ملصوقة. وإن قصدت اسمًا من عندك فعرّفه أولًا أو اكتبه بين علامتي backtick.",
            "Ghayar `{found}` b `{expected}`, aw shaghal `kalam convert` 3ashan yekteb el block el mansookh tany. W law te2sod esm men 3andak, 3arrafo el awel aw ektebo bein backticks.",
        ],
        explain: [
            "Why: each file is written in one skin (decision D6), so a reader only needs one vocabulary. Mixing skins usually happens when code is pasted from another file; `kalam convert` translates a whole file losslessly. Words from other skins are still fine as your own names, as long as they are declared.\n\nInstead of (in an English file):\n    إذا ready:\n        start()\n\nwrite:\n    if ready:\n        start()",
            "السبب: كل ملف مكتوب بواجهة واحدة (القرار D6)، فلا يحتاج القارئ إلا إلى مفردات واحدة. ويحدث الخلط بين الواجهات غالبًا عند لصق كود من ملف آخر، والأمر `kalam convert` يترجم الملف كله دون فقد. وكلمات الواجهات الأخرى تبقى مقبولة أسماءً من عندك ما دامت معرّفة.\n\nبدلًا من (في ملف عربي):\n    if جاهز:\n        ابدأ()\n\nاكتب:\n    إذا جاهز:\n        ابدأ()",
            "Leh: kol file maktoob b skin wa7ed (qarar D6), fa elly beye2ra me7tag mofradat wa7da bas. W el khalt ben el skins ghaleban beyeegi men code mansookh men file tany, w `kalam convert` beyetargem el file kollo men gher ma ydayya3 7aga. W kelmat el skins el tanya maashya ka asami men 3andak tool ma heya met3arrafa.\n\nBadal (f file Franco):\n    if gahez:\n        ebda2()\n\nEkteb:\n    law gahez:\n        ebda2()",
        ],
        proposed: false,
    },
    // ---- Modules and names (K03xx, K04xx, spec §13, §10.1, §5.3; kalam-resolve).
    // ar/franco: pending native review.
    Entry {
        code: "K0301",
        title: ["ONE MODULE IN TWO FILES", "وحدة واحدة في ملفين", "MODULE WA7ED F FILEIN"],
        message: [
            "The module {name} has more than one file, one per skin. The skin suffix is not part of a module's name, so `users.kl` and `users.ar.kl` are the same module.",
            "للوحدة {name} أكثر من ملف، ملف لكل واجهة. لاحقة الواجهة ليست جزءًا من اسم الوحدة، فالملفان `users.kl` و`users.ar.kl` وحدة واحدة.",
            "El module {name} leeh aktar men file, file l kol skin. El suffix bta3 el skin mesh gez2 men esm el module, fa `users.kl` w `users.ar.kl` homa nafs el module.",
        ],
        help: [
            "Keep one file per module. To change a file's skin, run `kalam convert`: it renames the file and every `{kw_use}` line stays as it is.",
            "أبقِ ملفًا واحدًا لكل وحدة. ولتغيير واجهة ملف شغّل `kalam convert`، فهو يعيد تسمية الملف وتبقى كل أسطر `{kw_use}` كما هي.",
            "Khalli file wa7ed l kol module. W 3ashan teghayar el skin bta3 file shaghal `kalam convert`: beyghayar esm el file w kol satr `{kw_use}` beyfdal zay ma howa.",
        ],
        explain: [
            "Why: a module's path never includes its skin (spec §4.1), so converting a file never changes an import anywhere. Two files for one path would make every `use` of it ambiguous.\n\nInstead of:\n    src/users.kl\n    src/users.ar.kl\n\nkeep one of them:\n    src/users.ar.kl",
            "السبب: مسار الوحدة لا يتضمن واجهتها أبدًا (المواصفة §4.1)، فتحويل ملف لا يغيّر أي استيراد في أي مكان. ولو وُجد ملفان لمسار واحد لصار كل `استخدم` يشير إليه ملتبسًا.\n\nبدلًا من:\n    src/users.kl\n    src/users.ar.kl\n\nأبقِ أحدهما:\n    src/users.ar.kl",
            "Leh: el path bta3 el module 3omro ma beykoon feeh el skin (el spec §4.1), fa ta7weel file 3omro ma beyghayar ay import f ay makan. W law fe filein l nafs el path, kol `hat` leeh hayeb2a mesh wade7.\n\nBadal:\n    src/users.kl\n    src/users.ar.kl\n\nKhalli wa7ed bas:\n    src/users.ar.kl",
        ],
        proposed: false,
    },
    Entry {
        code: "K0302",
        title: ["OPAQUE IS FOR TYPES", "الإغلاق للأنواع فقط", "MA2FOOL LEL ANWA3 BAS"],
        message: [
            "`{kw_opaque}` hides the fields, variants and constructor of a `{kw_type}` or an `{kw_enum}`. A function, trait, constant or extern declaration has nothing for it to hide.",
            "الكلمة `{kw_opaque}` تخفي حقول `{kw_type}` أو حالات `{kw_enum}` ومُنشئه. أما الدالة أو السمة أو الثابت أو التعريف الخارجي فليس فيها ما تخفيه.",
            "`{kw_opaque}` betkhabbi el fields wel variants wel constructor bta3 `{kw_type}` aw `{kw_enum}`. El dala aw el trait aw el constant aw el extern ma fihomsh 7aga tetkhebbi.",
        ],
        help: [
            "Remove `{kw_opaque}` and keep `{kw_export}`.",
            "احذف `{kw_opaque}` وأبقِ `{kw_export}`.",
            "Shil `{kw_opaque}` w khalli `{kw_export}`.",
        ],
        explain: [
            "Why: `export opaque` exports a type's name while keeping its insides private, so a validated type such as `Email` can only be built by its own module (spec §13.2). Other declarations are either exported or not.\n\nInstead of:\n    export opaque fn answer() -> Int => 42\n\nwrite:\n    export fn answer() -> Int => 42",
            "السبب: `علني مغلق` يصدّر اسم النوع ويُبقي داخله خاصًا، فلا يبني نوعًا مُتحقَّقًا منه مثل `بريد` إلا وحدته (المواصفة §13.2). أما بقية التعريفات فإما مُصدَّرة وإما لا.\n\nبدلًا من:\n    علني مغلق دالة الجواب() -> عدد => 42\n\nاكتب:\n    علني دالة الجواب() -> عدد => 42",
            "Leh: `talla3 ma2fool` beysaddar esm el no3 w beykhalli el gowa bta3o khas, fa no3 et2akkedna meno zay `email` mayetbenish gher f el module bta3o (el spec §13.2). Ba2y el ta3reefat ya mosaddara ya la2.\n\nBadal:\n    talla3 ma2fool dala answer() -> adad => 42\n\nEkteb:\n    talla3 dala answer() -> adad => 42",
        ],
        proposed: false,
    },
    Entry {
        code: "K0303",
        title: ["PRIVATE TYPE IN AN EXPORTED SIGNATURE", "نوع خاص في توقيع مُصدَّر", "NO3 KHAS F SIGNATURE MOSADDAR"],
        message: [
            "This exported declaration uses {name}, which is private to this module. Other modules could call it, but they could not write the type it takes or returns.",
            "هذا التعريف المُصدَّر يستخدم {name}، وهو خاص بهذه الوحدة. تستطيع الوحدات الأخرى استدعاءه، لكنها لا تستطيع كتابة النوع الذي يأخذه أو يرجعه.",
            "El ta3reef el mosaddar da beyesta5dem {name}, w da khas bel module da. El modules el tanya te2dar tenadeeh, bas mesh hate2dar tekteb el no3 elly beyakhdo aw beyrag3o.",
        ],
        help: [
            "Add `{kw_export}` to the private type (or `{kw_export} {kw_opaque}` to keep its fields hidden), or stop exporting this declaration.",
            "أضف `{kw_export}` إلى النوع الخاص (أو `{kw_export} {kw_opaque}` لتبقى حقوله مخفية)، أو توقّف عن تصدير هذا التعريف.",
            "Zawwed `{kw_export}` lel no3 el khas (aw `{kw_export} {kw_opaque}` 3ashan el fields bta3to tefdal mestakhabeya), aw ma tsaddarsh el ta3reef da.",
        ],
        explain: [
            "Why: a package's public API is every exported item (spec §13.2). If an exported function returns a private type, callers get a value whose type they cannot name, and `kalam pkg diff` cannot classify changes to it.\n\nInstead of:\n    type Token:\n        value: String\n\n    export fn issue() -> Token => Token(value: \"x\")\n\nwrite:\n    export opaque type Token:\n        value: String\n\n    export fn issue() -> Token => Token(value: \"x\")",
            "السبب: الواجهة العامة للحزمة هي كل عنصر مُصدَّر (المواصفة §13.2). فإن أرجعت دالة مُصدَّرة نوعًا خاصًا، حصل مستدعوها على قيمة لا يستطيعون تسمية نوعها، ولا يستطيع `kalam pkg diff` تصنيف التغييرات عليه.\n\nبدلًا من:\n    هيكل رمز:\n        قيمة: نص\n\n    علني دالة أصدر() -> رمز => رمز(قيمة: \"x\")\n\nاكتب:\n    علني مغلق هيكل رمز:\n        قيمة: نص\n\n    علني دالة أصدر() -> رمز => رمز(قيمة: \"x\")",
            "Leh: el API el 3omoomy bta3 el package howa kol 7aga mosaddara (el spec §13.2). Law dala mosaddara betraga3 no3 khas, elly beynadeeha hayakhod 2eema mayeb2ash 3aref yesamy no3ha, w `kalam pkg diff` mesh hayeb2a 3aref yesannef el taghyeerat 3aleeh.\n\nBadal:\n    haykal token:\n        value: nass\n\n    talla3 dala issue() -> token => token(value: \"x\")\n\nEkteb:\n    talla3 ma2fool haykal token:\n        value: nass\n\n    talla3 dala issue() -> token => token(value: \"x\")",
        ],
        proposed: false,
    },
    Entry {
        code: "K0401",
        title: ["NAME NOT FOUND", "اسم غير معرّف", "ESM MESH MET3ARRAF"],
        message: [
            "I can't find a declaration of {name} that is visible here. Kalam has no global names and no glob imports, so every name comes from this scope, this module, an import or the prelude.",
            "لم أجد تعريفًا لـ {name} يمكن رؤيته هنا. ليس في كلام أسماء عامة ولا استيراد بالجملة، فكل اسم يأتي من هذا النطاق أو هذه الوحدة أو من استيراد أو من التمهيد.",
            "Mal2etsh ta3reef l {name} bayen hena. Kalam ma fihoosh asami 3amma wala glob imports, fa kol esm gay men el scope da aw el module da aw men import aw men el prelude.",
        ],
        help: [
            "Check the spelling (the suggested fix shows the closest visible name), declare it before this line with `{kw_let}` or `{kw_fn}`, or import it with `{kw_use}`. A name written with lookalike letters is a different name: use the declared spelling exactly.",
            "راجع الكتابة (الإصلاح المقترح يعرض أقرب اسم مرئي)، أو عرّفه قبل هذا السطر بـ `{kw_let}` أو `{kw_fn}`، أو استورده بـ `{kw_use}`. والاسم المكتوب بحروف متشابهة اسم مختلف، فاكتبه كما عُرِّف بالضبط.",
            "Rage3 el ketaba (el fix el mo2tara7 beyewarrik a2rab esm bayen), aw 3arrafo 2abl el satr da b `{kw_let}` aw `{kw_fn}`, aw e3melo import b `{kw_use}`. W el esm el maktoob b 7oroof shabah ba3d esm tany, fa ekteb el esm zay ma et3arraf bel zabt.",
        ],
        explain: [
            "Why: every name in Kalam comes from a visible declaration: a local, a parameter, an item of this module, an import or the prelude. Uses are never matched loosely. A name spelled with lookalike letters (Persian `ک` for Arabic `ك`, Cyrillic `а` for Latin `a`) is a different name, and the fix offers the declared spelling (spec §5.3).\n\nInstead of:\n    let total = 1\n    print(totl)\n\nwrite:\n    let total = 1\n    print(total)",
            "السبب: كل اسم في كلام يأتي من تعريف مرئي: متغير محلي أو معامل أو عنصر في هذه الوحدة أو استيراد أو التمهيد. ولا يُطابَق أي استخدام تقريبيًا. فالاسم المكتوب بحروف متشابهة (`ک` الفارسية بدل `ك` العربية، أو `а` السيريلية بدل `a` اللاتينية) اسم مختلف، والإصلاح يقترح الكتابة المعرَّفة (المواصفة §5.3).\n\nبدلًا من:\n    ليكن كتاب = 1\n    اطبع(کتاب)\n\nاكتب:\n    ليكن كتاب = 1\n    اطبع(كتاب)",
            "Leh: kol esm f Kalam gay men ta3reef bayen: variable local aw parameter aw 7aga f el module da aw import aw el prelude. W mafeesh ay estekhdam beyet2arenn ta2reeban. El esm el maktoob b 7oroof shabah ba3d (`ک` el farsy badal `ك` el 3araby, aw `а` el cyrillic badal `a` el latini) esm tany, wel fix beye2tere7 el ketaba el met3arrafa (el spec §5.3).\n\nBadal:\n    khali total = 1\n    ektb(totl)\n\nEkteb:\n    khali total = 1\n    ektb(total)",
        ],
        proposed: false,
    },
    Entry {
        code: "K0405",
        title: ["NAME ALREADY IN USE", "اسم مستخدم بالفعل", "ESM MESTA5DEM ABLAN"],
        message: [
            "{name} is already declared in a scope that is visible here. Kalam does not let a new declaration hide an older one, so a name means one thing at a time.",
            "{name} معرَّف بالفعل في نطاق مرئي هنا. لا يسمح كلام لتعريف جديد بأن يخفي تعريفًا أقدم، فيبقى للاسم معنى واحد في كل وقت.",
            "{name} met3arraf ablan f scope bayen hena. Kalam mesh beysma7 l ta3reef gedeed yekhabbi ta3reef a2dam, fa el esm leeh ma3na wa7ed f kol wa2t.",
        ],
        help: [
            "Give the new declaration a name of its own. For a value of a type with the same name, Arabic code adds the article (`الشكل: شكل`) and Franco code adds `el_` (`el_donya: donya`).",
            "أعطِ التعريف الجديد اسمًا خاصًا به. ولقيمة من نوع بالاسم نفسه يضيف الكود العربي أداة التعريف (`الشكل: شكل`) ويضيف كود الفرانكو `el_` (`el_donya: donya`).",
            "Edy el ta3reef el gedeed esm khas beeh. W l 2eema men no3 b nafs el esm, el code el 3araby beyzawwed \"ال\" (`الشكل: شكل`) w code el Franco beyzawwed `el_` (`el_donya: donya`).",
        ],
        explain: [
            "Why: shadowing lets one name mean two things in nearby lines, which hides bugs in review (DR-28). Types, functions, values and module names share one namespace per scope (DR-11), and so do imports and the prelude.\n\nInstead of:\n    fn area(width: Int) -> Int:\n        let width = width * 2\n        return width\n\nwrite:\n    fn area(width: Int) -> Int:\n        let doubled = width * 2\n        return doubled",
            "السبب: الإخفاء يجعل الاسم الواحد يعني شيئين في أسطر متقاربة، فتختفي الأخطاء عند المراجعة (DR-28). والأنواع والدوال والقيم وأسماء الوحدات تشترك في فضاء أسماء واحد لكل نطاق (DR-11)، ومعها الاستيرادات والتمهيد.\n\nبدلًا من:\n    دالة مساحة(عرض: عدد) -> عدد:\n        ليكن عرض = عرض * 2\n        أرجع عرض\n\nاكتب:\n    دالة مساحة(عرض: عدد) -> عدد:\n        ليكن الضعف = عرض * 2\n        أرجع الضعف",
            "Leh: el shadowing beykhalli el esm el wa7ed ye3ni 7agtein f stoor 2orayeba, fa el aghlat betestakhabba f el review (DR-28). El anwa3 wel dawal wel 2eyam w asami el modules leehom namespace wa7ed l kol scope (DR-11), w ma3ahom el imports wel prelude.\n\nBadal:\n    dala masa7a(3ard: adad) -> adad:\n        khali 3ard = 3ard * 2\n        rag3 3ard\n\nEkteb:\n    dala masa7a(3ard: adad) -> adad:\n        khali el_de3f = 3ard * 2\n        rag3 el_de3f",
        ],
        proposed: false,
    },
    Entry {
        code: "K0413",
        title: ["CORE NAME IS RESERVED", "اسم أساسي محجوز", "ESM ASASY MA7GOOZ"],
        message: [
            "{name} is a core name (spec §7.2). Core names are translated between skins, so each one means the same thing in every program and is never declared again.",
            "{name} اسم أساسي (المواصفة §7.2). الأسماء الأساسية تُترجم بين الواجهات، فلكل منها المعنى نفسه في كل برنامج ولا يُعرَّف من جديد.",
            "{name} esm asasy (el spec §7.2). El asami el asaseya betetargem ben el skins, fa kol wa7ed leeh nafs el ma3na f kol program w mabyet3arrafsh tany.",
        ],
        help: [
            "Choose another name for your declaration. The one core name a program declares is `{core_main}`, its entry point.",
            "اختر اسمًا آخر لتعريفك. والاسم الأساسي الوحيد الذي يعرّفه البرنامج هو `{core_main}`، نقطة البداية.",
            "Ekhtar esm tany lel ta3reef bta3ak. W el esm el asasy el wa7eed elly el program beye3arrafo howa `{core_main}`, no2tet el bedaya.",
        ],
        explain: [
            "Why: core names such as `Int`, `print` and `Some` are spelled differently in each skin (`عدد`, `اطبع`, `موجود`). If a program could redefine one, converting the file to another skin would change what it means. The one core name you declare is `main`, the entry point.\n\nInstead of:\n    fn print(x: Int):\n        pass\n\nwrite:\n    fn show_number(x: Int):\n        pass",
            "السبب: الأسماء الأساسية مثل `عدد` و`اطبع` و`موجود` تُكتب بشكل مختلف في كل واجهة (`Int` و`print` و`Some`). ولو استطاع برنامج أن يعيد تعريف أحدها لغيّر تحويل الملف إلى واجهة أخرى معناه. والاسم الأساسي الوحيد الذي تعرّفه هو `رئيسي`، نقطة البداية.\n\nبدلًا من:\n    دالة اطبع(س: عدد):\n        تخطى\n\nاكتب:\n    دالة اعرض_العدد(س: عدد):\n        تخطى",
            "Leh: el asami el asaseya zay `adad` w `ektb` w `mawgood` betetketeb b shakl mokhtalef f kol skin (`Int` w `print` w `Some`). W law program 2eder ye3arraf wa7ed menhom tany, ta7weel el file l skin tany hayghayar ma3nah. W el esm el asasy el wa7eed elly enta bet3arrafo howa `ra2isi`, no2tet el bedaya.\n\nBadal:\n    dala ektb(x: adad):\n        fot\n\nEkteb:\n    dala e3red_el_rakam(x: adad):\n        fot",
        ],
        proposed: false,
    },
    Entry {
        code: "K0417",
        title: ["FIELD NAMED WITH", "حقل اسمه with", "FIELD ESMO WITH"],
        message: [
            "A record field can't be called `with`: every record already has a `with` method, which returns a copy with some fields changed (`u.with(name: \"Omar\")`).",
            "لا يمكن أن يُسمّى حقل في سجل `with`: لكل سجل دالة `with` جاهزة ترجع نسخة منه بعد تغيير بعض الحقول (`u.with(name: \"Omar\")`).",
            "Mayenfa3sh field f record yeb2a esmo `with`: kol record 3ando method `with` gahza betraga3 nos5a meno ba3d ma tghayar shwayet fields (`u.with(name: \"Omar\")`).",
        ],
        help: [
            "Rename the field, for example to `with_value` or a name that says what it holds.",
            "غيّر اسم الحقل، مثلًا إلى `with_value` أو اسم يصف ما يحمله.",
            "Ghayar esm el field, masalan `with_value` aw esm beyewa22a7 howa shayel eh.",
        ],
        explain: [
            "Why: `x.with(…)` is how records are updated, since values are immutable (spec §9.4). A field named `with` would make `x.with` mean two things.\n\nInstead of:\n    type Box:\n        with: Int\n\nwrite:\n    type Box:\n        extra: Int",
            "السبب: `س.with(…)` هي طريقة تحديث السجلات، لأن القيم لا تتغير (المواصفة §9.4). ولو سُمّي حقل `with` لصار لـ `س.with` معنيان.\n\nبدلًا من:\n    هيكل صندوق:\n        with: عدد\n\nاكتب:\n    هيكل صندوق:\n        إضافي: عدد",
            "Leh: `x.with(…)` heya tare2t ta7dees el records, 3ashan el 2eyam mabtetghayarsh (el spec §9.4). W law fe field esmo `with`, `x.with` hayeb2a leeha ma3nein.\n\nBadal:\n    haykal box:\n        with: adad\n\nEkteb:\n    haykal box:\n        extra: adad",
        ],
        proposed: false,
    },
    // ---- Parse errors (kalam-parser/PARSE-CODES.md). K0250–K0259 are proposed codes
    // (spec/CRITIQUE.md B4; FOUNDER-QUEUE Q1); K0710, K0720 and K0731 are spec §27 codes.
    // Only K0250 takes code arguments (`expected`, `found`). Keywords inside these texts are
    // keyword placeholders, so code stays in the file's skin (spec §21.4 rule 1).
    // ar/franco: pending native review.
    Entry {
        code: "K0250",
        title: ["UNEXPECTED TOKEN", "رمز غير متوقع", "7AGA MESH MATWAQA3A"],
        message: [
            "I expected {expected} here, but I found {found}. I skipped the rest of this line and carried on from the next one.",
            "كنت أتوقع {expected} هنا، لكنني وجدت {found}. تجاوزت بقية هذا السطر وأكملت من السطر التالي.",
            "Kont mestanni {expected} hena, bas la2eit {found}. Fawett ba2eit el satr da w kammelt men elly ba3do.",
        ],
        help: [
            "Add {expected} here, or remove {found} if it doesn't belong. A missing `:` at the end of a line that opens a block, or a bracket closed too early, is the usual cause.",
            "أضف {expected} هنا، أو احذف {found} إن لم يكن في مكانه. والسبب المعتاد `:` ناقصة في نهاية سطر يفتح كتلة، أو قوس أُغلق مبكرًا.",
            "Zawwed {expected} hena, aw emsa7 {found} law mesh makano. W ghaleban el sabab `:` na2sa f akher satr beyefta7 block, aw qaws et2afal badry.",
        ],
        explain: [
            "Why: at this point the grammar allows only a few things, and what I found is not one of them. I report the first surprise on the line and skip to the next line, so one slip gives one error rather than a cascade.\n\nInstead of:\n    if ready\n        start()\n\nwrite:\n    if ready:\n        start()",
            "السبب: في هذا الموضع لا تسمح القواعد إلا بأشياء قليلة، وما وجدته ليس منها. أبلغ عن أول مفاجأة في السطر ثم أنتقل إلى السطر التالي، فتعطي الزلة الواحدة خطأً واحدًا لا سلسلة من الأخطاء.\n\nبدلًا من:\n    إذا جاهز\n        ابدأ()\n\nاكتب:\n    إذا جاهز:\n        ابدأ()",
            "Leh: f el makan da el qawa3ed bteqbal 7agat 2olayela bas, w elly la2eto mesh menhom. Ba2ool 3ala awel mofag2a f el satr w anot lel satr elly ba3do, fa el ghalta el wa7da tedy error wa7ed mesh selsela.\n\nBadal:\n    law gahez\n        ebda2()\n\nEkteb:\n    law gahez:\n        ebda2()",
        ],
        proposed: true,
    },
    Entry {
        code: "K0251",
        title: ["UNCLOSED BRACKET", "قوس غير مغلق", "QAWS MESH MA2FOOL"],
        message: [
            "This bracket is still open when the line or file ends, so I can't tell where the expression inside it stops.",
            "هذا القوس ما زال مفتوحًا عند نهاية السطر أو الملف، فلا أعرف أين ينتهي التعبير الذي بداخله.",
            "El qaws da lessa maftoo7 lamma el satr aw el file khelso, fa mesh 3aref el expression elly gowah bte5las fein.",
        ],
        help: [
            "Add the matching `)`, `]` or `}` where the expression ends.",
            "أضف القوس المقابل `)` أو `]` أو `}` حيث ينتهي التعبير.",
            "Zawwed el qaws elly ye2felo `)` aw `]` aw `}` fein ma el expression bte5las.",
        ],
        explain: [
            "Why: a bracket may span several lines, but it must be closed before the block or the file ends. When it isn't, every following line would be swallowed into the expression, so I stop at the first line that can't belong to it.\n\nInstead of:\n    let total = sum(prices\n    print(\"done\")\n\nwrite:\n    let total = sum(prices)\n    print(\"done\")",
            "السبب: يمكن أن يمتد القوس على عدة أسطر، لكن يجب أن يُغلق قبل أن تنتهي الكتلة أو الملف. وإن لم يُغلق لابتلع التعبير كل الأسطر التالية، لذلك أتوقف عند أول سطر لا يمكن أن ينتمي إليه.\n\nبدلًا من:\n    ليكن المجموع = اجمع(الأسعار\n    اطبع(\"done\")\n\nاكتب:\n    ليكن المجموع = اجمع(الأسعار)\n    اطبع(\"done\")",
            "Leh: el qaws momken yemtedd 3ala kaza satr, bas lazem yet2efel 2abl ma el block aw el file yekhlas. Law ma et2afalsh, el expression kanet hatebla3 kol el sotoor elly ba3do, 3ashan keda bawa2af 3and awel satr mayenfa3sh yeb2a tab3o.\n\nBadal:\n    khali magmoo3 = egma3(as3ar\n    ektb(\"done\")\n\nEkteb:\n    khali magmoo3 = egma3(as3ar)\n    ektb(\"done\")",
        ],
        proposed: true,
    },
    Entry {
        code: "K0252",
        title: ["MISSING BLOCK", "كتلة ناقصة", "BLOCK NA2ES"],
        message: [
            "This line ends with `:`, so I expected an indented block on the next line, one level (4 spaces) deeper, but there isn't one.",
            "هذا السطر ينتهي بـ `:`، لذلك توقعت كتلة مُزاحة في السطر التالي، أعمق بمستوى واحد (4 مسافات)، لكنها غير موجودة.",
            "El satr da beyekhlas b `:`, fa kont mestanni block maza7 f el satr elly ba3do, mostawa wa7ed agwa (4 masafat), bas mafeesh.",
        ],
        help: [
            "Indent the lines that belong to it by 4 more spaces. If the block is meant to be empty for now, write `{kw_pass}` in it.",
            "أزح الأسطر التي تنتمي إليه 4 مسافات إضافية. وإن أردت الكتلة فارغة الآن فاكتب فيها `{kw_pass}`.",
            "Zo77 el sotoor elly tab3a lo 4 masafat zyada. W law 3ayez el block fady delwa2ty, ekteb feeh `{kw_pass}`.",
        ],
        explain: [
            "Why: `:` at the end of a line always opens a block (decision D15), and a block can't be empty, because an empty block is usually code that was lost by mistake. `pass` says \"nothing, on purpose\".\n\nInstead of:\n    fn todo():\n    fn main():\n        print(\"hi\")\n\nwrite:\n    fn todo():\n        pass\n    fn main():\n        print(\"hi\")",
            "السبب: `:` في نهاية السطر تفتح كتلة دائمًا (القرار D15)، والكتلة لا تكون فارغة، لأن الكتلة الفارغة غالبًا كود ضاع بالخطأ. والكلمة `تخطى` تعني \"لا شيء، عن قصد\".\n\nبدلًا من:\n    دالة لاحقًا():\n    دالة رئيسي():\n        اطبع(\"hi\")\n\nاكتب:\n    دالة لاحقًا():\n        تخطى\n    دالة رئيسي():\n        اطبع(\"hi\")",
            "Leh: `:` f akher el satr dayman bteftah block (qarar D15), wel block mayenfa3sh yeb2a fady, 3ashan el block el fady ghaleban code daa3 bel ghalat. W `fot` ma3naha \"mafeesh 7aga, 3ala tool\".\n\nBadal:\n    dala ba3deen():\n    dala ra2isi():\n        ektb(\"hi\")\n\nEkteb:\n    dala ba3deen():\n        fot\n    dala ra2isi():\n        ektb(\"hi\")",
        ],
        proposed: true,
    },
    Entry {
        code: "K0253",
        title: ["INTERPOLATION HOLE", "فجوة الإدراج", "HOLE EL STRING"],
        message: [
            "Each `{...}` hole in a string holds exactly one expression, and this one doesn't: it is empty or holds something I can't read as one value.",
            "كل فجوة `{...}` داخل النص تحتوي على تعبير واحد بالضبط، وهذه لا تحتوي عليه: إما فارغة وإما فيها ما لا أستطيع قراءته كقيمة واحدة.",
            "Kol hole `{...}` gowa el string feeha expression wa7ed bel zabt, w di la2: ya fadya ya feeha 7aga mesh 3aref a2raha ka qeema wa7da.",
        ],
        help: [
            "Put one value in the hole, or write `\\{` and `\\}` for literal braces.",
            "ضع في الفجوة قيمة واحدة، أو اكتب `\\{` و`\\}` إذا أردت أقواسًا عادية.",
            "7ot qeema wa7da f el hole, aw ekteb `\\{` w `\\}` law 3ayez aqwas 3adeya.",
        ],
        explain: [
            "Why: every `\"...\"` string fills `{...}` holes, so a brace in text is always read as the start of a hole. JSON or CSS inside a string needs its braces escaped.\n\nInstead of:\n    let body = \"{}\"\n\nwrite:\n    let body = \"\\{\\}\"",
            "السبب: كل نص `\"...\"` يملأ الفجوات `{...}`، فالقوس داخل النص يُقرأ دائمًا بداية فجوة. ولذلك يحتاج JSON أو CSS داخل النص إلى تهريب أقواسه.\n\nبدلًا من:\n    ليكن الجسم = \"{}\"\n\nاكتب:\n    ليكن الجسم = \"\\{\\}\"",
            "Leh: kol string `\"...\"` betmla el holes `{...}`, fa el qaws gowa el kalam dayman yet2ery bedayet hole. 3ashan keda el JSON aw el CSS gowa string me7tag escape lel aqwas.\n\nBadal:\n    khali gesm = \"{}\"\n\nEkteb:\n    khali gesm = \"\\{\\}\"",
        ],
        proposed: true,
    },
    Entry {
        code: "K0254",
        title: ["ARGUMENT ORDER", "ترتيب المعاملات", "TARTEEB EL ARGUMENTS"],
        message: [
            "This argument has no name, but it comes after a named one, so I can't tell which parameter it is for.",
            "هذا المعامل بلا اسم، لكنه جاء بعد معامل مُسمّى، فلا أعرف لأي مُعامِل هو.",
            "El argument da malosh esm, bas gai ba3d wa7ed leeh esm, fa mesh 3aref howa l anhy parameter.",
        ],
        help: [
            "Put the positional arguments first, then the named ones: `f(a, b, size: 2)`. Or give this argument its name too.",
            "ضع المعاملات بلا أسماء أولًا ثم المُسمّاة: `f(a، b، size: 2)`. أو أعطِ هذا المعامل اسمه أيضًا.",
            "7ot el arguments elly men gher asami el awel, ba3dein elly leeha asami: `f(a, b, size: 2)`. Aw edy el argument da esmo bardo.",
        ],
        explain: [
            "Why: positional arguments are matched by position, and once a named argument appears the positions are no longer obvious. Requiring positional-then-named keeps every call readable without looking up the signature.\n\nInstead of:\n    resize(size: 2, image)\n\nwrite:\n    resize(image, size: 2)",
            "السبب: المعاملات بلا أسماء تُطابَق بموضعها، وبعد ظهور معامل مُسمّى لا تعود المواضع واضحة. واشتراط ما بلا اسم أولًا ثم المُسمّى يجعل كل استدعاء مقروءًا دون الرجوع إلى تعريف الدالة.\n\nبدلًا من:\n    غيّر_الحجم(الحجم: 2، الصورة)\n\nاكتب:\n    غيّر_الحجم(الصورة، الحجم: 2)",
            "Leh: el arguments elly men gher asami betetwesel b makanha, w ba3d ma yeegi argument leeh esm el amaken mabteb2ash wad7a. W enn elly men gher esm yeegi el awel w ba3do elly leeh esm beyekhalli kol call mafhoom men gher ma terga3 lel ta3reef.\n\nBadal:\n    kabbar(size: 2, sora)\n\nEkteb:\n    kabbar(sora, size: 2)",
        ],
        proposed: true,
    },
    Entry {
        code: "K0255",
        title: ["STATEMENT OUTSIDE A FUNCTION", "جملة خارج دالة", "STATEMENT BARA DALA"],
        message: [
            "I found a statement at the top level of the file. The top level holds declarations only: functions, types, traits, constants and imports.",
            "وجدت جملة في المستوى الأعلى من الملف. المستوى الأعلى للتعريفات فقط: الدوال والأنواع والسمات والثوابت والاستيرادات.",
            "La2eit statement f awel mostawa f el file. Awel mostawa lel ta3reefat bas: dawal w anwa3 w traits w sawabet w imports.",
        ],
        help: [
            "Move this line into a function, such as `{core_main}`.",
            "انقل هذا السطر إلى داخل دالة، مثل `{core_main}`.",
            "Ensel el satr da gowa dala, zay `{core_main}`.",
        ],
        explain: [
            "Why: code that runs when a module is loaded makes the order of imports matter and hides side effects. In Kalam, a program starts at `main`, and everything at the top level is a declaration you can read in any order.\n\nInstead of:\n    print(\"starting\")\n\nwrite:\n    fn main():\n        print(\"starting\")",
            "السبب: الكود الذي يعمل عند تحميل الوحدة يجعل ترتيب الاستيراد مهمًا ويخفي الآثار الجانبية. في كلام يبدأ البرنامج من `رئيسي`، وكل ما في المستوى الأعلى تعريف يمكن قراءته بأي ترتيب.\n\nبدلًا من:\n    اطبع(\"starting\")\n\nاكتب:\n    دالة رئيسي():\n        اطبع(\"starting\")",
            "Leh: el code elly beyeshtaghal lamma el module yet7ammel beyekhalli tarteeb el imports mohem w beyekhabbi el side effects. F Kalam el program beyebda2 men `ra2isi`, w kol 7aga f awel mostawa ta3reef te2dar te2rah b ay tarteeb.\n\nBadal:\n    ektb(\"starting\")\n\nEkteb:\n    dala ra2isi():\n        ektb(\"starting\")",
        ],
        proposed: true,
    },
    Entry {
        code: "K0256",
        title: ["IMPORT AFTER A DECLARATION", "استيراد بعد تعريف", "IMPORT BA3D TA3REEF"],
        message: [
            "This import comes after a declaration. Imports go at the top of the file, before everything else, so readers see a file's dependencies first.",
            "هذا الاستيراد جاء بعد تعريف. الاستيرادات تأتي في أول الملف قبل أي شيء آخر، حتى يرى القارئ اعتماديات الملف أولًا.",
            "El import da gai ba3d ta3reef. El imports bteegi f awel el file 2abl ay 7aga, 3ashan elly beye2ra yshoof el dependencies el awel.",
        ],
        help: [
            "Move this `{kw_use}` line up with the other imports. `kalam fmt` does this for you.",
            "انقل سطر `{kw_use}` هذا إلى أعلى مع الاستيرادات الأخرى. الأمر `kalam fmt` يفعل ذلك لك.",
            "Ensel satr el `{kw_use}` da fo2 ma3 el imports el tanya. `kalam fmt` bey3mel keda lak.",
        ],
        explain: [
            "Why: one place for imports means a reviewer can see in one glance what a file depends on, including capabilities such as the network or the file system.\n\nInstead of:\n    fn main():\n        pass\n    use std/http\n\nwrite:\n    use std/http\n\n    fn main():\n        pass",
            "السبب: وجود الاستيرادات في مكان واحد يتيح للمراجع أن يرى بنظرة واحدة ما يعتمد عليه الملف، ومن ذلك القدرات مثل الشبكة ونظام الملفات.\n\nبدلًا من:\n    دالة رئيسي():\n        تخطى\n    استخدم std/http\n\nاكتب:\n    استخدم std/http\n\n    دالة رئيسي():\n        تخطى",
            "Leh: lamma el imports f makan wa7ed, elly beyraga3 yshoof f nazra wa7da el file m3tamed 3ala eh, w menhom el capabilities zay el network w el file system.\n\nBadal:\n    dala ra2isi():\n        fot\n    hat std/http\n\nEkteb:\n    hat std/http\n\n    dala ra2isi():\n        fot",
        ],
        proposed: true,
    },
    Entry {
        code: "K0259",
        title: ["NESTED TOO DEEPLY", "تداخل عميق جدًا", "MET2ASSA GAMED"],
        message: [
            "This code is nested more deeply than I can read safely, so I stopped descending here.",
            "هذا الكود متداخل أعمق مما أستطيع قراءته بأمان، لذلك توقفت عن النزول هنا.",
            "El code da met2assa agwa men elly a2dar a2rah b amaan, fa we2eft hena.",
        ],
        help: [
            "Split the expression or block into smaller pieces, each with its own name (`{kw_let}`) or function (`{kw_fn}`).",
            "قسّم التعبير أو الكتلة إلى أجزاء أصغر، لكل جزء اسمه (`{kw_let}`) أو دالته (`{kw_fn}`).",
            "2assem el expression aw el block l 7etat as8ar, kol 7etta leeha esm (`{kw_let}`) aw dala (`{kw_fn}`).",
        ],
        explain: [
            "Why: the compiler has a fixed nesting limit so that no input, however large or generated, can crash it. Code that reaches the limit is also far too deep for a person to follow.\n\nFix: give inner parts names.\n    let subtotal = price * count\n    let total = subtotal + shipping",
            "السبب: للمترجم حد ثابت للتداخل، حتى لا يستطيع أي مدخل، مهما كان كبيرًا أو مولَّدًا آليًا، أن يُسقطه. والكود الذي يبلغ هذا الحد أعمق بكثير من أن يتابعه إنسان.\n\nالحل: أعطِ الأجزاء الداخلية أسماء.\n    ليكن الفرعي = السعر * الكمية\n    ليكن المجموع = الفرعي + الشحن",
            "Leh: el compiler leeh 7add sabet lel tadakhol, 3ashan mafeesh input, mahma kan kebeer aw metwalled, ye2da yewa22a3o. W el code elly yewsal lel 7add da gameed aktar men ennu ensan yetaba3o.\n\nEl 7al: edy el agza2 elly gowa asami.\n    khali far3i = se3r * kamiya\n    khali magmoo3 = far3i + sha7n",
        ],
        proposed: true,
    },
    // ---- Statements (K07xx, spec §8). ar/franco: pending native review.
    Entry {
        code: "K0710",
        title: ["CAN'T ASSIGN TO THIS", "لا يمكن الإسناد إلى هذا", "MAYENFA3SH TEDDILO QEEMA"],
        message: [
            "The left side of `=` has to be a place: a variable, optionally followed by `.field` or `[index]`, whose root was declared with `{kw_var}`. This is a computed value, so there is nowhere to store the result.",
            "الطرف الأيسر من `=` يجب أن يكون مكانًا: متغيرًا، وبعده اختياريًا `.حقل` أو `[فهرس]`، وأصله مُعرَّف بـ `{kw_var}`. أما هذا فقيمة محسوبة، فلا يوجد مكان لحفظ النتيجة.",
            "El naw7eya el shmal men `=` lazem tekoon makan: variable, w momken ba3do `.field` aw `[index]`, w aslo met3arraf b `{kw_var}`. Da qeema met7asba, fa mafeesh makan net7afaz feeh el natiga.",
        ],
        help: [
            "Bind the value to a new name with `{kw_let}` instead, or assign to the variable the value came from.",
            "اربط القيمة باسم جديد بـ `{kw_let}` بدلًا من ذلك، أو أسند إلى المتغير الذي جاءت منه القيمة.",
            "Erbot el qeema b esm gedeed b `{kw_let}` badal keda, aw eddi el qeema lel variable elly gat menno.",
        ],
        explain: [
            "Why: an assignment changes a stored value, so it needs a place to store into. `f(x) = 1` or `a + b = c` has no such place. Values in Kalam are immutable unless declared with `var`, which keeps every change visible.\n\nInstead of:\n    total() = 10\n\nwrite:\n    var total = 0\n    total = 10",
            "السبب: الإسناد يغيّر قيمة محفوظة، فيحتاج إلى مكان يحفظ فيه. و`f(x) = 1` أو `a + b = c` ليس فيهما هذا المكان. والقيم في كلام ثابتة ما لم تُعرَّف بـ `متغير`، وهذا يُبقي كل تغيير ظاهرًا.\n\nبدلًا من:\n    المجموع() = 10\n\nاكتب:\n    متغير المجموع = 0\n    المجموع = 10",
            "Leh: el assignment beyghayar qeema mat7fooza, fa me7tag makan yet7afaz feeh. W `f(x) = 1` aw `a + b = c` mafeehomsh el makan da. W el qeyam f Kalam sabta ella law et3arrafet b `motghayer`, w da beyekhalli kol taghyeer bayen.\n\nBadal:\n    magmoo3() = 10\n\nEkteb:\n    motghayer magmoo3 = 0\n    magmoo3 = 10",
        ],
        proposed: false,
    },
    Entry {
        code: "K0720",
        title: ["CHAINED COMPARISON", "مقارنة متسلسلة", "MO2ARNA METSALSELA"],
        message: [
            "Comparisons don't chain in Kalam, so `a < b < c` doesn't mean what it does in math.",
            "المقارنات في كلام لا تتسلسل، فـ `a < b < c` لا تعني ما تعنيه في الرياضيات.",
            "El mo2aranat f Kalam mabtetsalselsh, fa `a < b < c` mesh ma3naha zay el riyadeyat.",
        ],
        help: [
            "Write two comparisons joined with `{kw_and}`: `a < b {kw_and} b < c`.",
            "اكتب مقارنتين واربطهما بـ `{kw_and}`: `a < b {kw_and} b < c`.",
            "Ekteb mo2arnteen w erbothom b `{kw_and}`: `a < b {kw_and} b < c`.",
        ],
        explain: [
            "Why: in C and JavaScript, `1 < 5 < 3` compares `true < 3`; in Python it means `1 < 5 and 5 < 3`. Rather than pick one surprise, Kalam asks you to write what you mean.\n\nInstead of:\n    if 0 <= age < 120:\n\nwrite:\n    if 0 <= age and age < 120:",
            "السبب: في C وJavaScript يقارن `1 < 5 < 3` القيمة `true` بالعدد 3، وفي Python يعني `1 < 5 and 5 < 3`. وبدلًا من اختيار إحدى المفاجأتين يطلب منك كلام أن تكتب ما تقصده.\n\nبدلًا من:\n    إذا 0 <= العمر < 120:\n\nاكتب:\n    إذا 0 <= العمر و العمر < 120:",
            "Leh: f C w JavaScript, `1 < 5 < 3` beyqaren `true` b 3, w f Python ma3naha `1 < 5 and 5 < 3`. Badal ma Kalam yekhtar mofag2a menhom, beytlob menak tekteb enta te2sod eh.\n\nBadal:\n    law 0 <= sen < 120:\n\nEkteb:\n    law 0 <= sen we sen < 120:",
        ],
        proposed: false,
    },
    Entry {
        code: "K0731",
        title: ["CONDITION WITHOUT AN ELSE", "شرط بلا فرع بديل", "SHART MEN GHER ELSE"],
        message: [
            "This `{kw_if}` is used as a value, so it needs a `{kw_else}` branch: without one, there is no value when the condition is false.",
            "هذا الشرط `{kw_if}` مستخدم كقيمة، لذلك يحتاج إلى فرع `{kw_else}`: بدونه لا توجد قيمة عندما يكون الشرط خاطئًا.",
            "El `{kw_if}` da mesta5dem ka qeema, fa me7tag far3 `{kw_else}`: men gheiro mafeesh qeema lamma el shart yeb2a ghalat.",
        ],
        help: [
            "Add a `{kw_else}:` branch with the value for the false case, or use the `{kw_if}` as a statement on its own line.",
            "أضف فرع `{kw_else}:` بالقيمة المطلوبة حين يكون الشرط خاطئًا، أو استخدم `{kw_if}` كجملة مستقلة في سطرها.",
            "Zawwed far3 `{kw_else}:` b el qeema lamma el shart yeb2a ghalat, aw esta5dem el `{kw_if}` ka statement f satr lewa7do.",
        ],
        explain: [
            "Why: every expression in Kalam has a value of one type. An `if` without `else` has no value on the false path, and Kalam has no `null` to fill the gap silently.\n\nInstead of:\n    let label = if paid: \"paid\"\n\nwrite:\n    let label = if paid: \"paid\" else: \"due\"",
            "السبب: لكل تعبير في كلام قيمة من نوع واحد. والشرط `إذا` بلا `وإلا` ليس له قيمة في حالة الخطأ، وليس في كلام `null` يسد الفراغ دون أن تلاحظ.\n\nبدلًا من:\n    ليكن الحالة = إذا مدفوع: \"paid\"\n\nاكتب:\n    ليكن الحالة = إذا مدفوع: \"paid\" وإلا: \"due\"",
            "Leh: kol expression f Kalam leeha qeema men no3 wa7ed. W `law` men gher `walla` malhash qeema lamma el shart ghalat, w Kalam mafeehash `null` tesedd el faragh men gher ma takhod balak.\n\nBadal:\n    khali hala = law madfoo3: \"paid\"\n\nEkteb:\n    khali hala = law madfoo3: \"paid\" walla: \"due\"",
        ],
        proposed: false,
    },
    // ---- Known LLM mistakes (K09xx, spec §21.3). Each one is an error with a
    // machine-applicable fix, never a silent alias. The foreign syntax a message quotes
    // (`===`, `def`, `elif`) is skin-neutral; every Kalam keyword is a keyword placeholder,
    // so it is spelled in the file's skin. ar/franco: pending native review.
    Entry {
        code: "K0901",
        title: ["STRICT EQUALITY OPERATOR", "عامل المساواة الصارمة", "OPERATOR EL MOSAWA EL SARMA"],
        message: [
            "Kalam has no `===` or `!==`. Its `==` and `!=` are already strict: values of different types are never silently converted before comparing.",
            "ليس في كلام `===` ولا `!==`. فالعاملان `==` و`!=` صارمان أصلًا: لا تُحوَّل القيم من نوع إلى آخر دون علمك قبل المقارنة.",
            "Kalam mafeehosh `===` wala `!==`. El `==` wel `!=` sarmeen aslan: el qeyam mabtet7awelsh men no3 l no3 men gher ma te3raf 2abl el mo2arna.",
        ],
        help: [
            "Write `==` instead of `===`, and `!=` instead of `!==`. `kalam fix` does this for you.",
            "اكتب `==` بدلًا من `===`، و`!=` بدلًا من `!==`. الأمر `kalam fix` يفعل ذلك لك.",
            "Ekteb `==` badal `===`, w `!=` badal `!==`. `kalam fix` bey3mel keda lak.",
        ],
        explain: [
            "Why: JavaScript needs `===` because its `==` converts types (`\"1\" == 1` is true). Kalam never converts, and comparing two different types is a compile error (K0602), so one operator is enough.\n\nInstead of:\n    if count === 0:\n\nwrite:\n    if count == 0:",
            "السبب: تحتاج JavaScript إلى `===` لأن `==` فيها تحوّل الأنواع (`\"1\" == 1` صحيحة). أما كلام فلا يحوّل الأنواع أبدًا، ومقارنة نوعين مختلفين خطأ ترجمة (K0602)، فيكفي عامل واحد.\n\nبدلًا من:\n    إذا العدد === 0:\n\nاكتب:\n    إذا العدد == 0:",
            "Leh: JavaScript me7taga `===` 3ashan el `==` bta3ha betghayar el anwa3 (`\"1\" == 1` sa7). Laken Kalam 3omro ma beyghayar el anwa3, w mo2arnet no3een mokhtalfeen compile error (K0602), fa operator wa7ed kefaya.\n\nBadal:\n    law kamiya === 0:\n\nEkteb:\n    law kamiya == 0:",
        ],
        proposed: false,
    },
    Entry {
        code: "K0902",
        title: ["FUNCTION KEYWORD FROM ANOTHER LANGUAGE", "كلمة دالة من لغة أخرى", "KELMET DALA MEN LOGHA TANYA"],
        message: [
            "`def`, `function` and `func` start functions in other languages. Kalam declares a function with `{kw_fn}`.",
            "الكلمات `def` و`function` و`func` تبدأ الدوال في لغات أخرى. أما كلام فيعرّف الدالة بـ `{kw_fn}`.",
            "`def` w `function` w `func` bey2ebdo el dawal f loghat tanya. Kalam bey3arraf el dala b `{kw_fn}`.",
        ],
        help: [
            "Write `{kw_fn} name(param: Type):` and give each parameter a type. `kalam fix` rewrites the keyword and leaves `_` type holes for you to fill in.",
            "اكتب `{kw_fn} الاسم(المعامل: النوع):` وأعطِ كل معامل نوعه. الأمر `kalam fix` يغيّر الكلمة ويترك مكان النوع `_` لتملأه.",
            "Ekteb `{kw_fn} esm(param: Type):` w edy kol parameter no3o. `kalam fix` beyghayar el kelma w beysib makan el no3 `_` 3ashan temlah.",
        ],
        explain: [
            "Why: Kalam has exactly one way to declare a function, so every file reads the same. Parameters always have types; the fix leaves `_` where a type is needed.\n\nInstead of:\n    def greet(name):\n        print(\"Hello\")\n\nwrite:\n    fn greet(name: String):\n        print(\"Hello\")",
            "السبب: في كلام طريقة واحدة فقط لتعريف الدالة، فتُقرأ كل الملفات بالطريقة نفسها. وللمعاملات أنواع دائمًا، والإصلاح يترك `_` حيث يلزم نوع.\n\nبدلًا من:\n    def رحب(الاسم):\n        اطبع(\"Hello\")\n\nاكتب:\n    دالة رحب(الاسم: نص):\n        اطبع(\"Hello\")",
            "Leh: f Kalam tare2a wa7da bas tet3arraf beeha el dala, fa kol el malafat btet2ery nafs el shakl. W el parameters dayman leeha anwa3, wel 7al beysib `_` fein ma no3 matloob.\n\nBadal:\n    def rahhab(esm):\n        ektb(\"Hello\")\n\nEkteb:\n    dala rahhab(esm: nass):\n        ektb(\"Hello\")",
        ],
        proposed: false,
    },
    Entry {
        code: "K0903",
        title: ["ELSE-IF SPELLING", "كتابة \"وإلا إذا\"", "KETABET ELSE IF"],
        message: [
            "`elif` and `else if (…) {` come from other languages. In Kalam, the next branch is `{kw_else} {kw_if} condition:`.",
            "`elif` و`else if (…) {` من لغات أخرى. أما في كلام فالفرع التالي يُكتب `{kw_else} {kw_if} الشرط:`.",
            "`elif` w `else if (…) {` men loghat tanya. F Kalam el far3 elly ba3do yetketeb `{kw_else} {kw_if} shart:`.",
        ],
        help: [
            "Write `{kw_else} {kw_if}`, then the condition and `:`, with no parentheses or braces. `kalam fix` does this for you.",
            "اكتب `{kw_else} {kw_if}` ثم الشرط ثم `:`، دون أقواس. الأمر `kalam fix` يفعل ذلك لك.",
            "Ekteb `{kw_else} {kw_if}` w ba3daha el shart w `:`, men gher aqwas. `kalam fix` bey3mel keda lak.",
        ],
        explain: [
            "Why: one spelling for each construct keeps code canonical (spec §21), and `else if` reads the same way in all three skins.\n\nInstead of:\n    if n > 0:\n        print(\"positive\")\n    elif n < 0:\n        print(\"negative\")\n\nwrite:\n    if n > 0:\n        print(\"positive\")\n    else if n < 0:\n        print(\"negative\")",
            "السبب: كتابة واحدة لكل بنية تُبقي الكود قياسيًا (القسم 21 من المواصفة)، و`وإلا إذا` تُقرأ بالطريقة نفسها في الواجهات الثلاث.\n\nبدلًا من:\n    إذا n > 0:\n        اطبع(\"positive\")\n    elif n < 0:\n        اطبع(\"negative\")\n\nاكتب:\n    إذا n > 0:\n        اطبع(\"positive\")\n    وإلا إذا n < 0:\n        اطبع(\"negative\")",
            "Leh: ketaba wa7da l kol 7aga betkhalli el code canonical (spec §21), w `walla law` btet2ery nafs el shakl f el talat skins.\n\nBadal:\n    law n > 0:\n        ektb(\"positive\")\n    elif n < 0:\n        ektb(\"negative\")\n\nEkteb:\n    law n > 0:\n        ektb(\"positive\")\n    walla law n < 0:\n        ektb(\"negative\")",
        ],
        proposed: false,
    },
    Entry {
        code: "K0904",
        title: ["SEMICOLON", "فاصلة منقوطة", "SEMICOLON"],
        message: [
            "Kalam doesn't use semicolons: a statement ends at the end of its line.",
            "كلام لا يستخدم الفاصلة المنقوطة: الجملة تنتهي بنهاية سطرها.",
            "Kalam mabyesta5demsh semicolon: el statement bte5las m3a akher el satr bta3ha.",
        ],
        help: [
            "Delete the `;` (or `؛`). If two statements share the line, put the second one on its own line. `kalam fix` does both.",
            "احذف `;` (أو `؛`). وإذا اجتمعت جملتان في سطر واحد فضع الثانية في سطر مستقل. الأمر `kalam fix` يفعل الأمرين.",
            "Emsa7 el `;` (aw `؛`). W law fe statementein f nafs el satr, 7ot el tanya f satr lewa7daha. `kalam fix` bey3mel el etnein.",
        ],
        explain: [
            "Why: lines already end statements, so a semicolon would be a second way to say the same thing, and code would stop being canonical.\n\nInstead of:\n    let a = 1; let b = 2;\n\nwrite:\n    let a = 1\n    let b = 2",
            "السبب: الأسطر تُنهي الجمل أصلًا، فتكون الفاصلة المنقوطة طريقة ثانية لقول الشيء نفسه، ويفقد الكود صورته القياسية.\n\nبدلًا من:\n    ليكن أ = 1؛ ليكن ب = 2؛\n\nاكتب:\n    ليكن أ = 1\n    ليكن ب = 2",
            "Leh: el sotoor bte5allas el statements aslan, fa el semicolon teb2a tare2a tanya l nafs el 7aga, wel code yeb2a mesh canonical.\n\nBadal:\n    khali a = 1; khali b = 2;\n\nEkteb:\n    khali a = 1\n    khali b = 2",
        ],
        proposed: false,
    },
    Entry {
        code: "K0905",
        title: ["BRACES AS A BLOCK", "أقواس معقوفة ككتلة", "AQWAS KA BLOCK"],
        message: [
            "Kalam blocks don't use `{ … }`. A block starts with `:` at the end of a line and continues on the lines indented one level deeper.",
            "الكتل في كلام لا تستخدم `{ … }`. تبدأ الكتلة بـ `:` في نهاية السطر وتستمر في الأسطر المُزاحة مستوى واحدًا أعمق.",
            "El blocks f Kalam mabtesta5demsh `{ … }`. El block beyebda2 b `:` f akher el satr w beykammel f el sotoor el maza7a mostawa wa7ed agwa.",
        ],
        help: [
            "Replace `{` with `:` at the end of the line, indent the body by 4 spaces, and delete the closing `}`. `kalam fix` does this for you.",
            "استبدل بالقوس `{` النقطتين `:` في نهاية السطر، وأزح الجسم 4 مسافات، واحذف القوس الختامي `}`. الأمر `kalam fix` يفعل ذلك لك.",
            "Ghayar el `{` b `:` f akher el satr, w zo77 el gesm 4 masafat, w emsa7 el `}` elly f el akher. `kalam fix` bey3mel keda lak.",
        ],
        explain: [
            "Why: blocks are marked by indentation (decision D15). Braces would let the visible indentation and the real structure disagree; with `:` they are the same thing. Braces in Kalam mean string holes and maps.\n\nInstead of:\n    if ready {\n        start()\n    }\n\nwrite:\n    if ready:\n        start()",
            "السبب: الكتل تُحدَّد بالإزاحة (القرار D15). والأقواس تسمح بأن تختلف الإزاحة الظاهرة عن البنية الحقيقية، أما مع `:` فهما شيء واحد. والأقواس المعقوفة في كلام لفجوات النصوص وللقواميس.\n\nبدلًا من:\n    إذا جاهز {\n        ابدأ()\n    }\n\nاكتب:\n    إذا جاهز:\n        ابدأ()",
            "Leh: el blocks btet7added bel za7a (qarar D15). El aqwas betsma7 enn el za7a el bayna tekhtelef 3an el shakl el 7a2i2i, laken ma3 `:` homa 7aga wa7da. W el aqwas el ma3kofa f Kalam lel holes f el strings wel maps.\n\nBadal:\n    law gahez {\n        ebda2()\n    }\n\nEkteb:\n    law gahez:\n        ebda2()",
        ],
        proposed: false,
    },
    Entry {
        code: "K0906",
        title: ["NULL FROM ANOTHER LANGUAGE", "null من لغة أخرى", "NULL MEN LOGHA TANYA"],
        message: [
            "`null`, `nil` and `undefined` don't exist in Kalam. A value that may be missing has the type `{core_option}[T]`, and the missing case is written `{core_none}`.",
            "لا وجود في كلام لـ `null` ولا `nil` ولا `undefined`. القيمة التي قد تكون غائبة نوعها `{core_option}[T]`، وحالة الغياب تُكتب `{core_none}`.",
            "Mafeesh f Kalam `null` wala `nil` wala `undefined`. El qeema elly momken tekoon mesh mawgooda no3ha `{core_option}[T]`, w 7alet el gheyab tetketeb `{core_none}`.",
        ],
        help: [
            "Write `{core_none}`, and give the variable the type `{core_option}[T]`, so the compiler checks every place that reads it.",
            "اكتب `{core_none}`، واجعل نوع المتغير `{core_option}[T]`، حتى يفحص المترجم كل موضع يقرؤه.",
            "Ekteb `{core_none}`, w ekhali no3 el variable `{core_option}[T]`, 3ashan el compiler yeraga3 kol makan beye2rah.",
        ],
        explain: [
            "Why: `null` can hide in any variable, which is why null errors are so common. In Kalam only an `Option` can be empty, the type says so, and `match` makes you handle the empty case.\n\nInstead of:\n    let user = null\n\nwrite:\n    let user: Option[User] = None",
            "السبب: يمكن أن يختبئ `null` في أي متغير، ولهذا تكثر أخطاؤه. أما في كلام فلا يكون فارغًا إلا `اختياري`، ونوعه يقول ذلك، و`حسب` تُلزمك بمعالجة حالة الفراغ.\n\nبدلًا من:\n    ليكن المستخدم = null\n\nاكتب:\n    ليكن المستخدم: اختياري[مستخدم] = لا_شيء",
            "Leh: `null` momken yestakhabba f ay variable, w 3ashan keda akhta2o keteera. F Kalam mafeesh 7aga tenfa3 teb2a fadya ella `ekhtiyari`, w no3ha be2ool keda, w `hasab` betlazmak tet3amel ma3 el 7ala el fadya.\n\nBadal:\n    khali mostakhdem = null\n\nEkteb:\n    khali mostakhdem: ekhtiyari[User] = mafeesh",
        ],
        proposed: false,
    },
    Entry {
        code: "K0907",
        title: ["ASYNC OR AWAIT", "async أو await", "ASYNC AW AWAIT"],
        message: [
            "Kalam has no `async` or `await`. The compiler works out which functions wait for input or output and handles the waiting for you (decision D12).",
            "ليس في كلام `async` ولا `await`. المترجم يعرف بنفسه أي الدوال تنتظر إدخالًا أو إخراجًا، ويتولى الانتظار عنك (القرار D12).",
            "Kalam mafeehosh `async` wala `await`. El compiler beyefham lewa7do anhy dawal bteestanna input aw output, w beyestanna badalak (qarar D12).",
        ],
        help: [
            "Delete `async` and `await` and write an ordinary call. `kalam fix` does this for you.",
            "احذف `async` و`await` واكتب استدعاءً عاديًا. الأمر `kalam fix` يفعل ذلك لك.",
            "Emsa7 `async` w `await` w ekteb call 3adi. `kalam fix` bey3mel keda lak.",
        ],
        explain: [
            "Why: `async` spreads through a codebase: one waiting function forces every caller to change (the \"colored functions\" problem). Kalam code looks synchronous and the compiler inserts the waiting (spec §15).\n\nInstead of:\n    let page = await fetch(url)\n\nwrite:\n    let page = fetch(url)",
            "السبب: الكلمة `async` تنتشر في الكود: دالة واحدة تنتظر تُجبر كل من يستدعيها على التغيير (مشكلة \"الدوال الملوّنة\"). أما كود كلام فيبدو متزامنًا، والمترجم يضيف الانتظار (القسم 15 من المواصفة).\n\nبدلًا من:\n    ليكن الصفحة = await جلب(الرابط)\n\nاكتب:\n    ليكن الصفحة = جلب(الرابط)",
            "Leh: `async` betentesher f el code: dala wa7da bteestanna betgbar kol elly beyenadeeha yetghayar (moshkelet el \"colored functions\"). Laken code Kalam beyban synchronous, wel compiler howa elly beyzawwed el entezar (spec §15).\n\nBadal:\n    khali safha = await fetch(link)\n\nEkteb:\n    khali safha = fetch(link)",
        ],
        proposed: false,
    },
    Entry {
        code: "K0908",
        title: ["EXCEPTIONS FROM ANOTHER LANGUAGE", "استثناءات من لغة أخرى", "EXCEPTIONS MEN LOGHA TANYA"],
        message: [
            "`try`, `catch`, `throw` and `raise` aren't Kalam keywords. Expected errors are values of type `{core_result}`, so every function that can fail says so in its type.",
            "`try` و`catch` و`throw` و`raise` ليست كلمات محجوزة في كلام. الأخطاء المتوقعة قيم من النوع `{core_result}`، فكل دالة يمكن أن تفشل تقول ذلك في نوعها.",
            "`try` w `catch` w `throw` w `raise` mesh keywords f Kalam. El errors el metwa22a3a qeyam men no3 `{core_result}`, fa kol dala momken tefshal bet2ool keda f no3ha.",
        ],
        help: [
            "To fail, write `{kw_return} {core_err}(…)`. To pass a call's error up to your caller, put `{punct_try}` after the call. To handle it, use `{kw_match}` on the result.",
            "للفشل اكتب `{kw_return} {core_err}(…)`. ولتمرير خطأ استدعاء إلى من استدعاك ضع `{punct_try}` بعد الاستدعاء. ولمعالجته استخدم `{kw_match}` على النتيجة.",
            "3ashan tefshal ekteb `{kw_return} {core_err}(…)`. W 3ashan tewassal error el call lel elly nadak 7ot `{punct_try}` ba3d el call. W 3ashan tet3amel ma3ah esta5dem `{kw_match}` 3ala el natiga.",
        ],
        explain: [
            "Why: exceptions are invisible in a function's signature, so callers can't see what might fail. With `Result`, failure is part of the type, `?` passes it on in one character, and the compiler warns about a `Result` you ignore (K1303). Bugs still panic (decision D16).\n\nInstead of:\n    try:\n        let n = parse(text)\n    catch e:\n        print(\"not a number\")\n\nwrite:\n    match parse(text):\n        case Ok(n): print(\"ok\")\n        case Err(e): print(\"not a number\")",
            "السبب: الاستثناءات لا تظهر في توقيع الدالة، فلا يرى من يستدعيها ما قد يفشل. أما مع `نتيجة` فالفشل جزء من النوع، و`؟` تمرره بحرف واحد، والمترجم ينبّهك إلى `نتيجة` تتجاهلها (K1303). أما الأخطاء البرمجية فما زالت تُسبب انهيارًا (القرار D16).\n\nبدلًا من:\n    try:\n        ليكن ع = حلل(النص)\n    catch خ:\n        اطبع(\"not a number\")\n\nاكتب:\n    حسب حلل(النص):\n        عندما نجاح(ع): اطبع(\"ok\")\n        عندما فشل(خ): اطبع(\"not a number\")",
            "Leh: el exceptions mesh bayna f tawqee3 el dala, fa elly beyenadeeha mesh shayef eh momken yefshal. Laken ma3 `natiga` el fashal goz2 men el no3, w `?` bet3addeeh b 7arf wa7ed, wel compiler beynabbahak law fe `natiga` enta mesh wakhed balak menha (K1303). W el bugs lessa bte3mel panic (qarar D16).\n\nBadal:\n    try:\n        khali n = parse(text)\n    catch e:\n        ektb(\"not a number\")\n\nEkteb:\n    hasab parse(text):\n        lama naga7(n): ektb(\"ok\")\n        lama fashal(e): ektb(\"not a number\")",
        ],
        proposed: false,
    },
    Entry {
        code: "K0909",
        title: ["INCREMENT OPERATOR", "عامل الزيادة", "OPERATOR EL ZYADA"],
        message: [
            "Kalam has no `++` or `--`. An update is written as an assignment, so every change to a variable is visible.",
            "ليس في كلام `++` ولا `--`. التحديث يُكتب كإسناد، فيظهر كل تغيير في المتغير.",
            "Kalam mafeehosh `++` wala `--`. El ta7dees yetketeb assignment, fa kol taghyeer f el variable yeb2a bayen.",
        ],
        help: [
            "Write `x += 1` instead of `x++`, and `x -= 1` instead of `x--`; the variable must be declared with `{kw_var}`. `kalam fix` does this for you.",
            "اكتب `x += 1` بدلًا من `x++`، و`x -= 1` بدلًا من `x--`، ويجب أن يكون المتغير مُعرَّفًا بـ `{kw_var}`. الأمر `kalam fix` يفعل ذلك لك.",
            "Ekteb `x += 1` badal `x++`, w `x -= 1` badal `x--`, w lazem el variable yeb2a met3arraf b `{kw_var}`. `kalam fix` bey3mel keda lak.",
        ],
        explain: [
            "Why: `++` hides an assignment inside an expression, and `i++` versus `++i` give different values in subtle ways. Kalam keeps assignment a statement of its own.\n\nInstead of:\n    count++\n\nwrite:\n    count += 1",
            "السبب: `++` يخفي إسنادًا داخل تعبير، و`i++` و`++i` تعطيان قيمتين مختلفتين بطريقة دقيقة. لذلك يُبقي كلام الإسناد جملة مستقلة.\n\nبدلًا من:\n    العداد++\n\nاكتب:\n    العداد += 1",
            "Leh: `++` beyekhabbi assignment gowa expression, w `i++` w `++i` bydo qeyam mokhtalfa b shakl mesh wade7. 3ashan keda Kalam beyekhalli el assignment statement lewa7do.\n\nBadal:\n    counter++\n\nEkteb:\n    counter += 1",
        ],
        proposed: false,
    },
    Entry {
        code: "K0910",
        title: ["TERNARY OPERATOR", "العامل الثلاثي", "EL TERNARY OPERATOR"],
        message: [
            "Kalam has no `c ? a : b`. `{kw_if}` is an expression, so you can use it anywhere a value goes.",
            "ليس في كلام `c ? a : b`. فالشرط `{kw_if}` تعبير، ويمكنك استخدامه في أي مكان تُوضع فيه قيمة.",
            "Kalam mafeehosh `c ? a : b`. El `{kw_if}` expression, fa te2dar testa5demha f ay makan beyeegi feeh qeema.",
        ],
        help: [
            "Write `{kw_if} c: a {kw_else}: b`. `kalam fix` does this for you.",
            "اكتب `{kw_if} c: a {kw_else}: b`. الأمر `kalam fix` يفعل ذلك لك.",
            "Ekteb `{kw_if} c: a {kw_else}: b`. `kalam fix` bey3mel keda lak.",
        ],
        explain: [
            "Why: `?` already means \"pass the error up\" in Kalam, and a second meaning would be confusing. One conditional form works everywhere (spec §8.3).\n\nInstead of:\n    let label = n > 0 ? \"yes\" : \"no\"\n\nwrite:\n    let label = if n > 0: \"yes\" else: \"no\"",
            "السبب: علامة `؟` تعني في كلام \"مرّر الخطأ\"، ومعنى ثانٍ لها سيُربك القارئ. وصيغة شرط واحدة تصلح في كل مكان (القسم 8.3 من المواصفة).\n\nبدلًا من:\n    ليكن الوصف = n > 0 ? \"yes\" : \"no\"\n\nاكتب:\n    ليكن الوصف = إذا n > 0: \"yes\" وإلا: \"no\"",
            "Leh: el `?` f Kalam ma3naha \"wassal el error\", w ma3na tany leeha hayelkhbat elly beye2ra. W shakl wa7ed lel shart maashy f kol makan (spec §8.3).\n\nBadal:\n    khali wasf = n > 0 ? \"yes\" : \"no\"\n\nEkteb:\n    khali wasf = law n > 0: \"yes\" walla: \"no\"",
        ],
        proposed: false,
    },
    Entry {
        code: "K0911",
        title: ["SYMBOLIC LOGIC OPERATOR", "رمز منطقي", "ROMOOZ MANTE2EYA"],
        message: [
            "Kalam writes `{found}` as `{expected}`. Logic is spelled with words so it reads the same as the rest of the line.",
            "في كلام يُكتب `{found}` هكذا: `{expected}`. المنطق يُكتب بكلمات حتى يُقرأ مثل بقية السطر.",
            "F Kalam `{found}` btetketeb keda: `{expected}`. El mante2 betetketeb b kelmat 3ashan tet2ery zay ba2eit el satr.",
        ],
        help: [
            "Replace `{found}` with `{expected}`. In general, `&&` is `{kw_and}`, `||` is `{kw_or}` and `!` is `{kw_not}`. `kalam fix` does this for you.",
            "استبدل بـ `{found}` الكلمة `{expected}`. وعمومًا `&&` هي `{kw_and}`، و`||` هي `{kw_or}`، و`!` هي `{kw_not}`. الأمر `kalam fix` يفعل ذلك لك.",
            "Ghayar `{found}` b `{expected}`. W 3omooman `&&` heya `{kw_and}`, w `||` heya `{kw_or}`, w `!` heya `{kw_not}`. `kalam fix` bey3mel keda lak.",
        ],
        explain: [
            "Why: `!` is easy to miss next to a name, and `&&` and `||` look alike at a glance. Words are harder to misread, and they translate into each skin.\n\nInstead of:\n    if ready && !late:\n\nwrite:\n    if ready and not late:",
            "السبب: من السهل ألا تلاحظ `!` بجوار اسم، و`&&` و`||` متشابهان للنظرة السريعة. أما الكلمات فيصعب أن تُقرأ خطأً، ولها ترجمة في كل واجهة.\n\nبدلًا من:\n    إذا جاهز && !متأخر:\n\nاكتب:\n    إذا جاهز و ليس متأخر:",
            "Leh: sahl matakhodsh balak men `!` gamb esm, w `&&` w `||` shabah ba3d f el nazra el sare3a. Laken el kelmat sa3b tet2ery ghalat, w leeha targama f kol skin.\n\nBadal:\n    law gahez && !met2akhar:\n\nEkteb:\n    law gahez we mesh met2akhar:",
        ],
        proposed: false,
    },
    Entry {
        code: "K0912",
        title: ["MUTABILITY KEYWORDS FROM ANOTHER LANGUAGE", "كلمات التغيّر من لغة أخرى", "KELMAT EL TAGHYEER MEN LOGHA TANYA"],
        message: [
            "`let mut` and `const` come from other languages. In Kalam, `{kw_let}` declares a name you can't reassign, and `{kw_var}` declares one you can.",
            "`let mut` و`const` من لغات أخرى. في كلام تعرّف `{kw_let}` اسمًا لا تُعاد قيمته، وتعرّف `{kw_var}` اسمًا يمكن تغيير قيمته.",
            "`let mut` w `const` men loghat tanya. F Kalam `{kw_let}` bet3arraf esm mayetghayarsh, w `{kw_var}` bet3arraf esm yenfa3 yetghayar.",
        ],
        help: [
            "Write `{kw_var} x` instead of `let mut x`, and `{kw_let} x` instead of `const x`. `kalam fix` does this for you.",
            "اكتب `{kw_var} x` بدلًا من `let mut x`، و`{kw_let} x` بدلًا من `const x`. الأمر `kalam fix` يفعل ذلك لك.",
            "Ekteb `{kw_var} x` badal `let mut x`, w `{kw_let} x` badal `const x`. `kalam fix` bey3mel keda lak.",
        ],
        explain: [
            "Why: two keywords cover every case. `let` is the default and `var` marks the few names that change, so a reader can spot them.\n\nInstead of:\n    let mut total = 0\n    const limit = 10\n\nwrite:\n    var total = 0\n    let limit = 10",
            "السبب: كلمتان تكفيان لكل الحالات. `ليكن` هي الأصل، و`متغير` تميّز الأسماء القليلة التي تتغير، فيلاحظها القارئ.\n\nبدلًا من:\n    let mut المجموع = 0\n    const الحد = 10\n\nاكتب:\n    متغير المجموع = 0\n    ليكن الحد = 10",
            "Leh: kelmetein kefaya l kol el 7alat. `khali` heya el asl, w `motghayer` betmayez el asami el 2olayela elly btetghayar, fa elly beye2ra yla7ezha.\n\nBadal:\n    let mut magmoo3 = 0\n    const hadd = 10\n\nEkteb:\n    motghayer magmoo3 = 0\n    khali hadd = 10",
        ],
        proposed: false,
    },
    Entry {
        code: "K0913",
        title: ["STRING PREFIX OR TEMPLATE", "بادئة نص أو قالب", "PREFIX AW TEMPLATE STRING"],
        message: [
            "Kalam strings need no `f` prefix, and no backticks with `${…}`. Every `\"…\"` string already fills holes written as `{…}`.",
            "نصوص كلام لا تحتاج إلى البادئة `f`، ولا إلى علامات backtick مع `${…}`. فكل نص `\"…\"` يملأ الفجوات المكتوبة `{…}` أصلًا.",
            "Strings Kalam mesh me7taga `f` f el awel, wala backticks ma3 `${…}`. Kol string `\"…\"` betmla el holes elly maktooba `{…}` aslan.",
        ],
        help: [
            "Delete the `f` prefix, or change the backticks to `\"` and each `${` to `{`. `kalam fix` does this for you.",
            "احذف البادئة `f`، أو استبدل بعلامات backtick العلامة `\"` وبكل `${` القوس `{`. الأمر `kalam fix` يفعل ذلك لك.",
            "Emsa7 el `f`, aw ghayar el backticks b `\"` w kol `${` b `{`. `kalam fix` bey3mel keda lak.",
        ],
        explain: [
            "Why: one string syntax with holes everywhere means there is nothing to forget. A literal brace is written `\\{`.\n\nInstead of:\n    print(f\"Hi {user.name}\")\n\nwrite:\n    print(\"Hi {user.name}\")",
            "السبب: صيغة نص واحدة فيها فجوات في كل مكان، فلا يوجد ما تنساه. والقوس العادي يُكتب `\\{`.\n\nبدلًا من:\n    اطبع(f\"Hi {المستخدم.الاسم}\")\n\nاكتب:\n    اطبع(\"Hi {المستخدم.الاسم}\")",
            "Leh: shakl wa7ed lel string w feeh holes f kol makan, fa mafeesh 7aga tensaha. W el qaws el 3adi yetketeb `\\{`.\n\nBadal:\n    ektb(f\"Hi {user.esm}\")\n\nEkteb:\n    ektb(\"Hi {user.esm}\")",
        ],
        proposed: false,
    },
    Entry {
        code: "K0914",
        title: ["ANGLE-BRACKET TYPE ARGUMENTS", "معاملات نوع بين علامتي < >", "TYPE ARGUMENTS BEN < >"],
        message: [
            "Kalam writes type arguments in square brackets, `{core_list}[T]`, not `List<T>`, `Vec<T>`, `Array<T>` or `T[]`.",
            "يكتب كلام معاملات النوع بين قوسين مربعين، `{core_list}[T]`، لا `List<T>` ولا `Vec<T>` ولا `Array<T>` ولا `T[]`.",
            "Kalam beyekteb el type arguments ben aqwas morab3a, `{core_list}[T]`, mesh `List<T>` wala `Vec<T>` wala `Array<T>` wala `T[]`.",
        ],
        help: [
            "Write `{core_list}[T]`, and use square brackets for every other type argument too, such as `{core_map}[K, V]`. `kalam fix` does this for you.",
            "اكتب `{core_list}[T]`، واستخدم القوسين المربعين لكل معاملات النوع الأخرى أيضًا، مثل `{core_map}[K، V]`. الأمر `kalam fix` يفعل ذلك لك.",
            "Ekteb `{core_list}[T]`, w esta5dem el aqwas el morab3a l kol el type arguments el tanya bardo, zay `{core_map}[K, V]`. `kalam fix` bey3mel keda lak.",
        ],
        explain: [
            "Why: `<` and `>` are comparisons, so `f(a < b, c > d)` could be read two ways. Square brackets are never ambiguous, and there is one list type instead of several.\n\nInstead of:\n    let names: Vec<String> = []\n\nwrite:\n    let names: List[String] = []",
            "السبب: `<` و`>` للمقارنة، فيمكن أن يُقرأ `f(a < b، c > d)` بطريقتين. أما القوسان المربعان فلا لبس فيهما أبدًا، وفي كلام نوع قائمة واحد بدلًا من عدة أنواع.\n\nبدلًا من:\n    ليكن الأسماء: Vec<نص> = []\n\nاكتب:\n    ليكن الأسماء: قائمة[نص] = []",
            "Leh: `<` w `>` lel mo2arna, fa `f(a < b, c > d)` momken tet2ery b tare2tein. Laken el aqwas el morab3a 3omraha ma btelkhbat, w f Kalam no3 list wa7ed badal kaza no3.\n\nBadal:\n    khali asami: Vec<nass> = []\n\nEkteb:\n    khali asami: qayma[nass] = []",
        ],
        proposed: false,
    },
    Entry {
        code: "K0915",
        title: ["UNWRAP", "unwrap", "UNWRAP"],
        message: [
            "`.unwrap()` stops the program without saying why. Kalam asks you to state what you expected, with `.expect(\"…\")`.",
            "`.unwrap()` يوقف البرنامج دون أن يذكر السبب. لذلك يطلب منك كلام أن تذكر ما توقعته، بـ `.expect(\"…\")`.",
            "`.unwrap()` beyewa22af el program men gher ma y2ool leh. 3ashan keda Kalam beytlob menak t2ool kont mestanni eh, b `.expect(\"…\")`.",
        ],
        help: [
            "Replace `.unwrap()` with `.expect(\"why this can't fail\")`, or handle the missing case with `{kw_match}` or `{punct_try}`.",
            "استبدل بـ `.unwrap()` الاستدعاء `.expect(\"لماذا لا يمكن أن يفشل\")`، أو عالج حالة الغياب بـ `{kw_match}` أو `{punct_try}`.",
            "Ghayar `.unwrap()` b `.expect(\"leh mayenfa3sh yefshal\")`, aw et3amel ma3 7alet el gheyab b `{kw_match}` aw `{punct_try}`.",
        ],
        explain: [
            "Why: when an `unwrap` fails in production, the log says only that a value was missing. The message you give `expect` records the invariant you relied on, which is what the person fixing it needs.\n\nInstead of:\n    let port = env.get(\"PORT\").unwrap()\n\nwrite:\n    let port = env.get(\"PORT\").expect(\"PORT is set in every deployment\")",
            "السبب: حين يفشل `unwrap` في بيئة الإنتاج لا يقول السجل إلا إن قيمة كانت غائبة. أما الرسالة التي تعطيها لـ `expect` فتسجّل الافتراض الذي اعتمدت عليه، وهو ما يحتاجه من سيصلح الخطأ.\n\nبدلًا من:\n    ليكن المنفذ = env.get(\"PORT\").unwrap()\n\nاكتب:\n    ليكن المنفذ = env.get(\"PORT\").expect(\"PORT is set in every deployment\")",
            "Leh: lamma `unwrap` yefshal f el production, el log beye2ool bas enn fe qeema kanet na2sa. Laken el resala elly btedeeha l `expect` betsaggel el efterad elly enta e3tamadt 3aleeh, w da elly me7tago elly hayesalla7 el moshkela.\n\nBadal:\n    khali port = env.get(\"PORT\").unwrap()\n\nEkteb:\n    khali port = env.get(\"PORT\").expect(\"PORT is set in every deployment\")",
        ],
        proposed: false,
    },
    Entry {
        code: "K0916",
        title: ["PRINT WITH SEVERAL ARGUMENTS", "طباعة بعدة معاملات", "PRINT B KAZA ARGUMENT"],
        message: [
            "`{core_print}` takes one string in Kalam. To print several values, put them in one string with holes.",
            "الدالة `{core_print}` في كلام تأخذ نصًا واحدًا. ولطباعة عدة قيم ضعها في نص واحد بفجوات.",
            "`{core_print}` f Kalam bteakhod string wa7da. W 3ashan tetba3 kaza qeema 7ottohom f string wa7da b holes.",
        ],
        help: [
            "Join the values in one string, for example `{core_print}(\"{order.id} {order.total}\")`. `kalam fix` does this for you.",
            "اجمع القيم في نص واحد، مثل `{core_print}(\"{الطلب.الرقم} {الطلب.المجموع}\")`. الأمر `kalam fix` يفعل ذلك لك.",
            "Egma3 el qeyam f string wa7da, zay `{core_print}(\"{order.id} {order.total}\")`. `kalam fix` bey3mel keda lak.",
        ],
        explain: [
            "Why: languages disagree on what `print(a, b)` puts between values (a space, a tab, nothing). With one string you see the exact output in the code.\n\nInstead of:\n    print(\"Total:\", order.total)\n\nwrite:\n    print(\"Total: {order.total}\")",
            "السبب: تختلف اللغات فيما يضعه `print(a, b)` بين القيم (مسافة أو tab أو لا شيء). ومع نص واحد ترى المخرجات بالضبط في الكود.\n\nبدلًا من:\n    اطبع(\"Total:\"، الطلب.المجموع)\n\nاكتب:\n    اطبع(\"Total: {الطلب.المجموع}\")",
            "Leh: el loghat mekhtelfa `print(a, b)` bet7ott eh ben el qeyam (masafa aw tab aw wala 7aga). Laken b string wa7da betshoof el output bel zabt f el code.\n\nBadal:\n    ektb(\"Total:\", order.total)\n\nEkteb:\n    ektb(\"Total: {order.total}\")",
        ],
        proposed: false,
    },
    Entry {
        code: "K0917",
        title: ["VARIANT WITHOUT A DOT", "حالة تعداد بلا نقطة", "VARIANT MEN GHER NO2TA"],
        message: [
            "An enum variant is written with a leading `.`: `.Circle(r)`, not `Circle(r)`. The dot means \"a variant of the type expected here\".",
            "تُكتب حالة التعداد بنقطة في أولها: `.Circle(r)` لا `Circle(r)`. والنقطة تعني \"حالة من النوع المتوقع هنا\".",
            "El variant bta3 el enum yetketeb b no2ta f awelo: `.Circle(r)` mesh `Circle(r)`. W el no2ta ma3naha \"variant men el no3 el metwa22a3 hena\".",
        ],
        help: [
            "Add `.` before the variant name. `kalam fix` does this for you.",
            "أضف `.` قبل اسم الحالة. الأمر `kalam fix` يفعل ذلك لك.",
            "Zawwed `.` 2abl esm el variant. `kalam fix` bey3mel keda lak.",
        ],
        explain: [
            "Why: the dot tells the compiler to look the name up in the expected enum, so two enums can both have a `Pending` variant without clashing, and a bare `Circle` always means a type or a function.\n\nInstead of:\n    match shape:\n        case Circle(r): 3.14 * r * r\n\nwrite:\n    match shape:\n        case .Circle(r): 3.14 * r * r",
            "السبب: النقطة تجعل المترجم يبحث عن الاسم في التعداد المتوقع، فيمكن أن يكون في تعدادين حالة اسمها `معلق` دون تعارض، ويبقى الاسم المجرد `دائرة` دائمًا نوعًا أو دالة.\n\nبدلًا من:\n    حسب الشكل:\n        عندما دائرة(ن): 3.14 * ن * ن\n\nاكتب:\n    حسب الشكل:\n        عندما .دائرة(ن): 3.14 * ن * ن",
            "Leh: el no2ta betkhalli el compiler ydawwar 3ala el esm f el enum el metwa22a3, fa momken enumein yeb2a fihom variant esmo `pending` men gher mashakel, w el esm men gher no2ta `dayra` dayman yeb2a no3 aw dala.\n\nBadal:\n    hasab shakl:\n        lama dayra(r): 3.14 * r * r\n\nEkteb:\n    hasab shakl:\n        lama .dayra(r): 3.14 * r * r",
        ],
        proposed: false,
    },
    Entry {
        code: "K0918",
        title: ["BUILT-IN FUNCTION FROM ANOTHER LANGUAGE", "دالة مدمجة من لغة أخرى", "DALA GAHZA MEN LOGHA TANYA"],
        message: [
            "`len(xs)`, `str(x)` and `int(s)` are built-in functions in other languages. In Kalam these are methods or string holes, so they appear in autocomplete after the `.`.",
            "`len(xs)` و`str(x)` و`int(s)` دوال مدمجة في لغات أخرى. أما في كلام فهي دوال أعضاء أو فجوات نصوص، فتظهر في الإكمال التلقائي بعد `.`.",
            "`len(xs)` w `str(x)` w `int(s)` dawal gahza f loghat tanya. F Kalam homa methods aw holes f string, fa betban f el autocomplete ba3d el `.`.",
        ],
        help: [
            "Write `xs.len()` for `len(xs)`, `\"{ x }\"` for `str(x)`, and `s.to_int(){punct_try}` for `int(s)`. `kalam fix` does this for you.",
            "اكتب `xs.len()` بدلًا من `len(xs)`، و`\"{ x }\"` بدلًا من `str(x)`، و`s.to_int(){punct_try}` بدلًا من `int(s)`. الأمر `kalam fix` يفعل ذلك لك.",
            "Ekteb `xs.len()` badal `len(xs)`, w `\"{ x }\"` badal `str(x)`, w `s.to_int(){punct_try}` badal `int(s)`. `kalam fix` bey3mel keda lak.",
        ],
        explain: [
            "Why: methods keep related operations next to their type and make them discoverable. Converting text to a number can fail, so `to_int` returns a `Result` and `?` passes the failure up.\n\nInstead of:\n    let n = int(text)\n    print(len(items))\n\nwrite:\n    let n = text.to_int()?\n    print(\"{items.len()}\")",
            "السبب: الدوال الأعضاء تُبقي العمليات بجوار نوعها وتسهّل اكتشافها. وتحويل النص إلى عدد قد يفشل، لذلك تُرجع `to_int` قيمة `نتيجة` وتمرّر `؟` الفشل إلى أعلى.\n\nبدلًا من:\n    ليكن ع = int(النص)\n    اطبع(len(العناصر))\n\nاكتب:\n    ليكن ع = النص.to_int()؟\n    اطبع(\"{العناصر.len()}\")",
            "Leh: el methods betkhalli el 3amaleyat gamb el no3 bta3ha w btsahhel tela2eeha. W ta7weel el kalam l rakam momken yefshal, 3ashan keda `to_int` betraga3 `natiga` w `?` betwassal el fashal le fo2.\n\nBadal:\n    khali n = int(text)\n    ektb(len(items))\n\nEkteb:\n    khali n = text.to_int()?\n    ektb(\"{items.len()}\")",
        ],
        proposed: false,
    },
    Entry {
        code: "K0919",
        title: ["IMPL WORD ORDER", "ترتيب كلمات التنفيذ", "TARTEEB EL IMPL"],
        message: [
            "`impl Trait for Type` is Rust's word order. Kalam names the type first: `{kw_impl} Money {kw_as} Show:`.",
            "`impl Trait for Type` هو ترتيب لغة Rust. أما كلام فيذكر النوع أولًا: `{kw_impl} Money {kw_as} Show:`.",
            "`impl Trait for Type` da tarteeb Rust. Kalam beyekteb el no3 el awel: `{kw_impl} Money {kw_as} Show:`.",
        ],
        help: [
            "Swap the two names and use `{kw_as}`: `{kw_impl} Type {kw_as} Trait:`. `kalam fix` does this for you.",
            "بدّل الاسمين واستخدم `{kw_as}`: `{kw_impl} النوع {kw_as} السمة:`. الأمر `kalam fix` يفعل ذلك لك.",
            "Badel el esmein w esta5dem `{kw_as}`: `{kw_impl} Type {kw_as} Trait:`. `kalam fix` bey3mel keda lak.",
        ],
        explain: [
            "Why: \"implement Money as Show\" reads as a sentence, and the same order works in Arabic and Franco (decision DR-10). The type you are working on comes first, as in every other declaration.\n\nInstead of:\n    impl Show for Money:\n\nwrite:\n    impl Money as Show:",
            "السبب: \"نفّذ مال بصفة قابل_للعرض\" تُقرأ جملةً، والترتيب نفسه يصلح في العربية والفرانكو (القرار DR-10). والنوع الذي تعمل عليه يأتي أولًا، كما في كل تعريف آخر.\n\nبدلًا من:\n    impl قابل_للعرض for مال:\n\nاكتب:\n    نفذ مال بصفة قابل_للعرض:",
            "Leh: \"naffez Money ka Show\" btet2ery gomla, w nafs el tarteeb maashy f el 3arabi wel Franco (qarar DR-10). W el no3 elly beshtaghal 3aleeh beyeegi el awel, zay ay ta3reef tany.\n\nBadal:\n    impl Show for Money:\n\nEkteb:\n    naffez Money ka Show:",
        ],
        proposed: false,
    },
    Entry {
        code: "K0920",
        title: ["CLASS OR STRUCT KEYWORD", "كلمة class أو struct", "KELMET CLASS AW STRUCT"],
        message: [
            "`class`, `struct`, `record` and `interface` come from other languages. Kalam declares a record type with `{kw_type}` and an interface with `{kw_trait}`.",
            "`class` و`struct` و`record` و`interface` من لغات أخرى. أما كلام فيعرّف نوع السجل بـ `{kw_type}`، والواجهة البرمجية بـ `{kw_trait}`.",
            "`class` w `struct` w `record` w `interface` men loghat tanya. Kalam bey3arraf el record b `{kw_type}` wel interface b `{kw_trait}`.",
        ],
        help: [
            "Write `{kw_type} Name:` with one `field: Type` per indented line, or `{kw_trait} Name:` for an interface. `kalam fix` rewrites the keyword.",
            "اكتب `{kw_type} الاسم:` مع `الحقل: النوع` في كل سطر مُزاح، أو `{kw_trait} الاسم:` للواجهة البرمجية. الأمر `kalam fix` يغيّر الكلمة.",
            "Ekteb `{kw_type} Esm:` w `field: Type` f kol satr maza7, aw `{kw_trait} Esm:` lel interface. `kalam fix` beyghayar el kelma.",
        ],
        explain: [
            "Why: Kalam has no classes or inheritance. Data is a `type`, behavior is a function or a `trait` implementation, and one keyword for each keeps code canonical.\n\nInstead of:\n    class User:\n        name: String\n\nwrite:\n    type User:\n        name: String",
            "السبب: ليس في كلام أصناف ولا وراثة. البيانات `هيكل`، والسلوك دالة أو تنفيذ لـ `سمة`، وكلمة واحدة لكل منها تُبقي الكود قياسيًا.\n\nبدلًا من:\n    class مستخدم:\n        الاسم: نص\n\nاكتب:\n    هيكل مستخدم:\n        الاسم: نص",
            "Leh: Kalam mafeehosh classes wala werasa. El data `haykal`, wel soluk dala aw tanfeez l `sefa`, w kelma wa7da l kol wa7ed betkhalli el code canonical.\n\nBadal:\n    class User:\n        esm: nass\n\nEkteb:\n    haykal User:\n        esm: nass",
        ],
        proposed: false,
    },
    Entry {
        code: "K0921",
        title: ["MULTI-LINE LAMBDA", "دالة مجهولة متعددة الأسطر", "LAMBDA 3ALA KAZA SATR"],
        message: [
            "A lambda body is a single expression in Kalam, and this one spans several lines.",
            "جسم الدالة المجهولة في كلام تعبير واحد فقط، وهذا الجسم يمتد على عدة أسطر.",
            "Gesm el lambda f Kalam expression wa7ed bas, w da mamdood 3ala kaza satr.",
        ],
        help: [
            "Declare a local `{kw_fn}` with a name just before this call, and pass the name.",
            "عرّف دالة محلية بـ `{kw_fn}` ولها اسم قبل هذا الاستدعاء مباشرةً، ومرّر الاسم.",
            "3arraf dala local b `{kw_fn}` w leeha esm 2abl el call da 3ala tool, w ba3at el esm.",
        ],
        explain: [
            "Why: a multi-line lambda inside a call mixes two indentation levels in one expression and hides logic in an argument list (decision DR-15). A named local function is easier to read, test and reuse.\n\nInstead of:\n    let out = items.map(fn(x):\n        let doubled = x * 2\n        doubled + 1)\n\nwrite:\n    fn step(x: Int) -> Int:\n        let doubled = x * 2\n        return doubled + 1\n    let out = items.map(step)",
            "السبب: الدالة المجهولة متعددة الأسطر داخل استدعاء تخلط مستويين من الإزاحة في تعبير واحد، وتخفي المنطق داخل قائمة المعاملات (القرار DR-15). أما الدالة المحلية المسمّاة فأسهل في القراءة والاختبار وإعادة الاستخدام.\n\nبدلًا من:\n    ليكن الناتج = العناصر.map(دالة(س):\n        ليكن الضعف = س * 2\n        الضعف + 1)\n\nاكتب:\n    دالة خطوة(س: عدد) -> عدد:\n        ليكن الضعف = س * 2\n        أرجع الضعف + 1\n    ليكن الناتج = العناصر.map(خطوة)",
            "Leh: el lambda elly 3ala kaza satr gowa call betkhallat mostaweyein za7a f expression wa7ed, w betkhabbi el logic gowa el arguments (qarar DR-15). Laken el dala el local elly leeha esm as-hal f el 2eraya wel test wel esta5dam tany.\n\nBadal:\n    khali natiga_kolaha = items.map(dala(x):\n        khali deff = x * 2\n        deff + 1)\n\nEkteb:\n    dala khatwa(x: adad) -> adad:\n        khali deff = x * 2\n        rag3 deff + 1\n    khali natiga_kolaha = items.map(khatwa)",
        ],
        proposed: false,
    },
    Entry {
        code: "K0922",
        title: ["EXPLICIT TYPE ARGUMENTS", "معاملات نوع صريحة", "TYPE ARGUMENTS SAREE7A"],
        message: [
            "Kalam doesn't take explicit type arguments in a call such as `f[T](x)`. It infers them from the arguments and from the type you expect back.",
            "لا يقبل كلام معاملات نوع صريحة في الاستدعاء مثل `f[T](x)`، بل يستنتجها من المعاملات ومن النوع الذي تتوقعه.",
            "Kalam mabeyqbalsh type arguments saree7a f el call zay `f[T](x)`. Howa beyestantegha men el arguments w men el no3 elly enta mestanneeh.",
        ],
        help: [
            "Remove `[T]` from the call and annotate the result instead: `{kw_let} r: T = f(x)`. `kalam fix` does this for you.",
            "احذف `[T]` من الاستدعاء واكتب نوع النتيجة بدلًا من ذلك: `{kw_let} r: T = f(x)`. الأمر `kalam fix` يفعل ذلك لك.",
            "Emsa7 `[T]` men el call w ekteb no3 el natiga badal keda: `{kw_let} r: T = f(x)`. `kalam fix` bey3mel keda lak.",
        ],
        explain: [
            "Why: `f[T](x)` looks exactly like indexing `f[T]` and then calling the result. An annotated binding says the same thing without the ambiguity (decision DR-25).\n\nInstead of:\n    let user = decode[User](body)\n\nwrite:\n    let user: User = decode(body)",
            "السبب: `f[T](x)` يشبه تمامًا الفهرسة `f[T]` ثم استدعاء النتيجة. أما كتابة النوع عند الربط فتقول الشيء نفسه دون لبس (القرار DR-25).\n\nبدلًا من:\n    ليكن المستخدم = فك[مستخدم](الجسم)\n\nاكتب:\n    ليكن المستخدم: مستخدم = فك(الجسم)",
            "Leh: `f[T](x)` shabah bel zabt el indexing `f[T]` w ba3dein call lel natiga. Laken ketabet el no3 3and el rabt bet2ool nafs el 7aga men gher lakhbata (qarar DR-25).\n\nBadal:\n    khali mostakhdem = decode[User](body)\n\nEkteb:\n    khali mostakhdem: User = decode(body)",
        ],
        proposed: false,
    },
    Entry {
        code: "K0923",
        title: ["POWER OPERATOR", "عامل الأس", "OPERATOR EL OS"],
        message: [
            "Kalam has no `**` operator. Raise a number to a power with the method `.pow()`.",
            "ليس في كلام العامل `**`. ارفع العدد إلى قوة بالدالة العضو `.pow()`.",
            "Kalam mafeehosh `**`. Erfa3 el rakam l os b el method `.pow()`.",
        ],
        help: [
            "Write `a.pow(b)` instead of `a ** b`. `kalam fix` does this for you.",
            "اكتب `a.pow(b)` بدلًا من `a ** b`. الأمر `kalam fix` يفعل ذلك لك.",
            "Ekteb `a.pow(b)` badal `a ** b`. `kalam fix` bey3mel keda lak.",
        ],
        explain: [
            "Why: languages disagree about `-2 ** 2` (is it 4 or -4?) and about whether `^` means power or XOR. A method call has no precedence to remember.\n\nInstead of:\n    let area = side ** 2\n\nwrite:\n    let area = side.pow(2)",
            "السبب: تختلف اللغات في `-2 ** 2` (هل الناتج 4 أم -4؟) وفي معنى `^` أهو الأس أم XOR. أما استدعاء الدالة العضو فليس فيه أسبقية تحتاج إلى تذكّرها.\n\nبدلًا من:\n    ليكن المساحة = الضلع ** 2\n\nاكتب:\n    ليكن المساحة = الضلع.pow(2)",
            "Leh: el loghat mekhtelfa f `-2 ** 2` (4 walla -4?) w f `^` ma3naha os walla XOR. Laken el method call mafeehash awlaweya me7tag tftekerha.\n\nBadal:\n    khali mesa7a = del3 ** 2\n\nEkteb:\n    khali mesa7a = del3.pow(2)",
        ],
        proposed: false,
    },
    Entry {
        code: "K0924",
        title: ["SINGLE QUOTES", "علامات تنصيص مفردة", "SINGLE QUOTES"],
        message: [
            "Strings use double quotes in Kalam; single quotes don't start a string.",
            "النصوص في كلام تُكتب بين علامتي تنصيص مزدوجتين، والعلامة المفردة لا تبدأ نصًا.",
            "El strings f Kalam bein double quotes, wel single quote mabteftahsh string.",
        ],
        help: [
            "Change the single quotes to double quotes: `\"...\"`. `kalam fix` does this for you.",
            "استبدل بعلامتي التنصيص المفردتين علامتين مزدوجتين: `\"...\"`. الأمر `kalam fix` يفعل ذلك لك.",
            "Ghayar el single quotes b double quotes: `\"...\"`. `kalam fix` bey3mel keda lak.",
        ],
        explain: [
            "Why: one string syntax means one way to write text, and an apostrophe inside text (`\"it's\"`) never needs an escape. Kalam has no separate character type, so `'a'` would have no other meaning.\n\nInstead of:\n    let name = 'Sara'\n\nwrite:\n    let name = \"Sara\"",
            "السبب: صيغة نص واحدة تعني طريقة واحدة لكتابة النص، والفاصلة العليا داخل النص (`\"it's\"`) لا تحتاج إلى تهريب أبدًا. وليس في كلام نوع منفصل للحرف الواحد، فلن يكون لـ `'a'` معنى آخر.\n\nبدلًا من:\n    ليكن الاسم = 'Sara'\n\nاكتب:\n    ليكن الاسم = \"Sara\"",
            "Leh: shakl wa7ed lel string ma3nah tare2a wa7da tekteb beeha kalam, wel apostrophe gowa el kalam (`\"it's\"`) 3omraha ma te7tag escape. W Kalam mafeehosh no3 lel 7arf el wa7ed, fa `'a'` malhash ma3na tany.\n\nBadal:\n    khali esm = 'Sara'\n\nEkteb:\n    khali esm = \"Sara\"",
        ],
        proposed: false,
    },
    Entry {
        code: "K0925",
        title: ["SWITCH STATEMENT", "جملة switch", "SWITCH"],
        message: [
            "`switch`, `case X ->` and `x => …` arms come from other languages. Kalam branches on a value with `{kw_match}` and one `{kw_case} pattern:` line per arm.",
            "`switch` و`case X ->` والفروع `x => …` من لغات أخرى. أما كلام فيتفرع على القيمة بـ `{kw_match}`، مع سطر `{kw_case} النمط:` لكل فرع.",
            "`switch` w `case X ->` wel arms `x => …` men loghat tanya. Kalam beyetfarra3 3ala el qeema b `{kw_match}`, w satr `{kw_case} pattern:` l kol far3.",
        ],
        help: [
            "Write `{kw_match} x:` and, indented under it, `{kw_case} pattern: result` for each arm. Use `_` as the pattern for \"anything else\". `kalam fix` does this for you.",
            "اكتب `{kw_match} x:` وتحتها، بإزاحة، `{kw_case} النمط: النتيجة` لكل فرع. واستخدم `_` نمطًا لـ \"أي شيء آخر\". الأمر `kalam fix` يفعل ذلك لك.",
            "Ekteb `{kw_match} x:` w ta7taha, maza7a, `{kw_case} pattern: natiga` l kol far3. W esta5dem `_` ka pattern l \"ay 7aga tanya\". `kalam fix` bey3mel keda lak.",
        ],
        explain: [
            "Why: `match` checks that every case is covered (K1001) and never falls through to the next arm, which avoids the classic missing-`break` bug.\n\nInstead of:\n    switch status {\n        case 200 -> print(\"ok\")\n        default -> print(\"error\")\n    }\n\nwrite:\n    match status:\n        case 200: print(\"ok\")\n        case _: print(\"error\")",
            "السبب: `حسب` تتحقق من تغطية كل الحالات (K1001)، ولا تنتقل أبدًا إلى الفرع التالي دون قصد، فتتجنب الخطأ المشهور الناتج عن نسيان `break`.\n\nبدلًا من:\n    switch الحالة {\n        case 200 -> اطبع(\"ok\")\n        default -> اطبع(\"error\")\n    }\n\nاكتب:\n    حسب الحالة:\n        عندما 200: اطبع(\"ok\")\n        عندما _: اطبع(\"error\")",
            "Leh: `hasab` betet2akked enn kol el 7alat mat8atteya (K1001), w 3omraha ma btenzel lel far3 elly ba3do men gher ma te2sod, fa bet3addi el bug el mashhoor bta3 nesyan el `break`.\n\nBadal:\n    switch status {\n        case 200 -> ektb(\"ok\")\n        default -> ektb(\"error\")\n    }\n\nEkteb:\n    hasab status:\n        lama 200: ektb(\"ok\")\n        lama _: ektb(\"error\")",
        ],
        proposed: false,
    },
    Entry {
        code: "K0926",
        title: ["C-STYLE LOOP", "حلقة بأسلوب C", "LOOP ZAY C"],
        message: [
            "`for (let i = 0; i < n; i++)` and `for i in range(n)` come from other languages. Kalam loops over a range written `0..n`.",
            "`for (let i = 0; i < n; i++)` و`for i in range(n)` من لغات أخرى. أما كلام فيكرر على مدى يُكتب `0..n`.",
            "`for (let i = 0; i < n; i++)` w `for i in range(n)` men loghat tanya. Kalam bey3mel loop 3ala range maktoob `0..n`.",
        ],
        help: [
            "Write `{kw_for} i {kw_in} 0..n:`. Use `0..=n` to include `n` itself. `kalam fix` does this for you.",
            "اكتب `{kw_for} i {kw_in} 0..n:`. واستخدم `0..=n` إن أردت أن يدخل `n` نفسه. الأمر `kalam fix` يفعل ذلك لك.",
            "Ekteb `{kw_for} i {kw_in} 0..n:`. W esta5dem `0..=n` law 3ayez `n` nafsaha teb2a gowa. `kalam fix` bey3mel keda lak.",
        ],
        explain: [
            "Why: a range shows both bounds at a glance, with no counter to update by hand and no `<` versus `<=` mistake. To loop over a list, loop over the list itself.\n\nInstead of:\n    for i in range(10):\n        print(\"{ i }\")\n\nwrite:\n    for i in 0..10:\n        print(\"{ i }\")",
            "السبب: المدى يُظهر الحدين بنظرة واحدة، دون عدّاد تحدّثه بيدك ودون خطأ الخلط بين `<` و`<=`. وللتكرار على قائمة كرّر على القائمة نفسها.\n\nبدلًا من:\n    لكل i في range(10):\n        اطبع(\"{ i }\")\n\nاكتب:\n    لكل i في 0..10:\n        اطبع(\"{ i }\")",
            "Leh: el range beyewarrik el 7addein f nazra wa7da, men gher 3addad te7addeso b eedak w men gher ghaltet `<` w `<=`. W law 3ayez loop 3ala list, e3mel loop 3ala el list nafsaha.\n\nBadal:\n    lekol i fi range(10):\n        ektb(\"{ i }\")\n\nEkteb:\n    lekol i fi 0..10:\n        ektb(\"{ i }\")",
        ],
        proposed: false,
    },
    // ---- Modules and globals (K12xx, spec §13.1; kalam-resolve). ar/franco: pending
    // native review.
    Entry {
        code: "K1201",
        title: ["IMPORT CYCLE", "استيراد دائري", "IMPORT DAYRI"],
        message: [
            "These modules import each other in a cycle: {cycle}. Module constants are evaluated once, in dependency order, so imports must not loop back.",
            "هذه الوحدات يستورد بعضها بعضًا في دائرة: {cycle}. تُحسب ثوابت الوحدات مرة واحدة بترتيب الاعتماد، فلا يجوز أن تعود الاستيرادات إلى نقطة بدايتها.",
            "El modules di bet3mel import l ba3d f dayra: {cycle}. El constants bta3et el modules betet7seb marra wa7da b tarteeb el e3temad, fa el imports mayenfa3sh terga3 l ba3d.",
        ],
        help: [
            "Move what both modules need into a third module that both import, or pass it in as a parameter.",
            "انقل ما تحتاجه الوحدتان إلى وحدة ثالثة تستوردها كلتاهما، أو مرّره إليهما كمعامل.",
            "Na2el elly el modulein me7tageeno l module talet el etnein ye3melo leeh import, aw ba3ato ka parameter.",
        ],
        explain: [
            "Why: with a cycle there is no order in which every module's constants are ready before they are used, and neither module can be read without the other (spec §13.1).\n\nInstead of:\n    # app/orders.kl\n    use app/users\n    # app/users.kl\n    use app/orders\n\nwrite (app/model.kl holds what both need):\n    # app/orders.kl\n    use app/model\n    # app/users.kl\n    use app/model",
            "السبب: مع الدائرة لا يوجد ترتيب تكون فيه ثوابت كل وحدة جاهزة قبل استخدامها، ولا يمكن قراءة أي من الوحدتين دون الأخرى (المواصفة §13.1).\n\nبدلًا من:\n    # app/orders.ar.kl\n    استخدم app/users\n    # app/users.ar.kl\n    استخدم app/orders\n\nاكتب (والوحدة app/model.ar.kl فيها ما تحتاجه الوحدتان):\n    # app/orders.ar.kl\n    استخدم app/model\n    # app/users.ar.kl\n    استخدم app/model",
            "Leh: ma3 el dayra mafeesh tarteeb tekoon feeh el constants bta3et kol module gahza 2abl ma testa5demha, w mafeesh module yet2ere men gher el tany (el spec §13.1).\n\nBadal:\n    # app/orders.franco.kl\n    hat app/users\n    # app/users.franco.kl\n    hat app/orders\n\nEkteb (w app/model.franco.kl feeh elly el etnein me7tageeno):\n    # app/orders.franco.kl\n    hat app/model\n    # app/users.franco.kl\n    hat app/model",
        ],
        proposed: false,
    },
    // ---- Names, records and members found by the type checker (K04xx, spec §9.2, §12.2,
    // §21.3; kalam-typeck) and private items (K0418, FOUNDER-QUEUE Q23; kalam-resolve).
    // ar/franco: pending native review.
    Entry {
        code: "K0416",
        title: ["RECORD NEEDS NAMED FIELDS", "السجل يحتاج أسماء الحقول", "EL RECORD 3AYEZ ASAMY EL FIELDS"],
        message: [
            "I can't build this record from positional arguments. A record is always built with every field named, so a value can never land in the wrong field.",
            "لا أستطيع بناء هذا السجل من معاملات بالترتيب. يُبنى السجل دائمًا بتسمية كل حقل، حتى لا تذهب قيمة إلى الحقل الخطأ أبدًا.",
            "Mesh ha2dar abni el record da men arguments bel tarteeb. El record dayman byetbene b esm kol field, 3ashan mafeesh qeema terou7 lel field el ghalat abadan.",
        ],
        help: [
            "Name each field, for example `User(id: 1, name: \"Sara\")`. Fields with a default value may be left out.",
            "سمِّ كل حقل، مثل `مستخدم(الرقم: 1، الاسم: \"سارة\")`. ويمكن ترك الحقول التي لها قيمة افتراضية.",
            "Sammy kol field, zay `User(id: 1, esm: \"Sara\")`. W el fields elly leeha qeema default momken tesebha.",
        ],
        explain: [
            "Why: positional arguments depend on the order of the fields in the declaration. If someone swaps two fields of the same type, every positional construction would silently swap their values too. With named fields, each construction stays correct when the declaration changes (spec §9.4).\n\nInstead of:\n    let u = User(1, \"Sara\", email)\n\nwrite:\n    let u = User(id: 1, name: \"Sara\", email: email)",
            "السبب: المعاملات بالترتيب تعتمد على ترتيب الحقول في التعريف. فلو بدّل أحدهم حقلين من النوع نفسه لتبدلت قيمتاهما بصمت في كل بناء بالترتيب. أما مع تسمية الحقول فيبقى كل بناء صحيحًا حين يتغير التعريف (المواصفة §9.4).\n\nبدلًا من:\n    ليكن م = مستخدم(1، \"سارة\"، البريد)\n\nاكتب:\n    ليكن م = مستخدم(الرقم: 1، الاسم: \"سارة\"، البريد: البريد)",
            "Leh: el arguments bel tarteeb mesh bteshtaghal ella b tarteeb el fields f el ta3reef. Law 7ad baddal field-ein men nafs el no3, kol bena2 bel tarteeb hayebaddel qeyamhom f el sekket. Laken b asamy el fields kol bena2 byefdal sa7 lamma el ta3reef yetghayar (el spec §9.4).\n\nBadal:\n    khali u = User(1, \"Sara\", email)\n\nEkteb:\n    khali u = User(id: 1, esm: \"Sara\", email: email)",
        ],
        proposed: false,
    },
    Entry {
        code: "K0418",
        title: ["ITEM IS PRIVATE", "العنصر خاص", "EL 7AGA DI PRIVATE"],
        message: [
            "{name} exists in {module}, but it is private to that module, so I can't use it from here. Only items marked `{kw_export}` can be imported or reached through a module name.",
            "{name} موجود في {module}، لكنه خاص بتلك الوحدة، فلا أستطيع استخدامه من هنا. لا يمكن استيراد عنصر أو الوصول إليه عبر اسم وحدته إلا إذا كان معلَّمًا بـ `{kw_export}`.",
            "{name} mawgood f {module}, bas howa private lel module di, fa mesh ha2dar asta5demo men hena. Mafeesh 7aga tet3amallaha import aw tewsalaha 3an taree2 esm el module ella law 3aleiha `{kw_export}`.",
        ],
        help: [
            "Use something {module} exports instead, or, if {name} is meant to be public, mark it with `{kw_export}` in {module}.",
            "استخدم بدلًا منه شيئًا تصدّره {module}، أو إن كان {name} مقصودًا أن يكون عامًا فعلّمه بـ `{kw_export}` في {module}.",
            "Esta5dem badalo 7aga {module} bte3melaha export, aw law {name} el mafrood yeb2a public, 7ott 3aleih `{kw_export}` f {module}.",
        ],
        explain: [
            "Why: private items are a module's implementation details, so the module can change them without breaking anyone. If other modules could import them, every internal change would become a breaking change. I say \"private\" rather than \"not found\" because the name is right: only its visibility stops you.\n\nInstead of:\n    # app/billing.kl\n    type Rate:\n        percent: Decimal\n    # app/main.kl\n    use app/billing.{Rate}\n\nwrite (in app/billing.kl):\n    export type Rate:\n        percent: Decimal",
            "السبب: العناصر الخاصة تفاصيل داخلية للوحدة، فتستطيع الوحدة تغييرها دون أن تكسر شيئًا عند غيرها. ولو أمكن للوحدات الأخرى استيرادها لصار كل تغيير داخلي تغييرًا يكسر غيره. وأقول \"خاص\" لا \"غير موجود\" لأن الاسم صحيح، وإنما تمنعك رؤيته فقط.\n\nبدلًا من:\n    # app/billing.ar.kl\n    هيكل نسبة:\n        القيمة: عشري\n    # app/main.ar.kl\n    استخدم app/billing.{نسبة}\n\nاكتب (في app/billing.ar.kl):\n    علني هيكل نسبة:\n        القيمة: عشري",
            "Leh: el 7agat el private tafaseel dakhleya lel module, fa el module te2dar tghayarha men gher ma tekassar 7aga 3and 7ad. Law el modules el tanya te2dar te3melaha import, kol taghyeer dakhly hayeb2a taghyeer bykassar. W ana ba2ool \"private\" mesh \"mesh mawgood\" 3ashan el esm sa7: el visibility bas heya elly mana3ak.\n\nBadal:\n    # app/billing.franco.kl\n    haykal Rate:\n        percent: ashri\n    # app/main.franco.kl\n    hat app/billing.{Rate}\n\nEkteb (f app/billing.franco.kl):\n    talla3 haykal Rate:\n        percent: ashri",
        ],
        proposed: false,
    },
    Entry {
        code: "K0420",
        title: ["UNKNOWN FIELD OR METHOD", "حقل أو دالة غير معروفة", "FIELD AW METHOD MESH MA3ROOFA"],
        message: [
            "`{type}` has no field or method named `{name}`. I check every call against the real API, so a guessed name stops here instead of failing while the program runs.",
            "النوع `{type}` ليس فيه حقل أو دالة اسمها `{name}`. أتحقق من كل استدعاء مقابل الواجهة الحقيقية، فيتوقف الاسم المخمَّن هنا بدل أن يفشل أثناء تشغيل البرنامج.",
            "`{type}` mafeehoosh field aw method esmaha `{name}`. Ana ba-check kol call 3ala el API el 7a2ee2y, fa el esm el mettakhammen beyo2af hena badal ma yo2a3 w el program shaghal.",
        ],
        help: [
            "Check the spelling against the type's fields and methods. Names from other languages map like this: `push` → `append`, `length` and `size` → `len`, `includes` → `contains`.",
            "راجع الكتابة مقابل حقول النوع ودواله. وهذه مقابلات أسماء من لغات أخرى: `push` ← `append`، و`length` و`size` ← `len`، و`includes` ← `contains`.",
            "Raga3 el ketaba 3ala el fields wel methods bta3et el no3. Asamy men loghat tanya: `push` → `append`, `length` w `size` → `len`, `includes` → `contains`.",
        ],
        explain: [
            "Why: a method name that looks plausible but does not exist is one of the most common mistakes, from people and from code generators alike. I list the closest real names, by spelling and by a table of names from other languages. Collections are immutable, so a method such as `append` returns a new list instead of changing the old one (spec §21.3).\n\nInstead of:\n    items.push(x)\n\nwrite (with `items` declared as `var`):\n    items = items.append(x)",
            "السبب: اسم الدالة الذي يبدو معقولًا لكنه غير موجود من أكثر الأخطاء شيوعًا، عند الناس وعند مولّدات الكود. لذلك أعرض أقرب الأسماء الحقيقية، حسب الكتابة وحسب جدول لأسماء من لغات أخرى. والمجموعات لا تتغير، فالدالة `append` مثلًا ترجع قائمة جديدة ولا تغيّر القديمة (المواصفة §21.3).\n\nبدلًا من:\n    العناصر.push(س)\n\nاكتب (مع تعريف `العناصر` بـ `متغير`):\n    العناصر = العناصر.append(س)",
            "Leh: esm method shaklo ma3qool bas mesh mawgood men akter el aghlat, 3and el nas w 3and el code generators. 3ashan keda ba3red a2rab asamy 7a2ee2ya, bel ketaba w b gadwal asamy men loghat tanya. W el collections mabtetghayarsh, fa method zay `append` betraga3 list gedeeda w mesh betghayar el adeema (el spec §21.3).\n\nBadal:\n    items.push(x)\n\nEkteb (w `items` mota3arafa b `motghayer`):\n    items = items.append(x)",
        ],
        proposed: false,
    },
    // ---- Types and operators (K06xx, spec §9, §10.6, §18; kalam-typeck). K0600 and K0603 are
    // provisional codes the type checker uses until the founder assigns them (spec erratum
    // proposal). ar/franco: pending native review.
    Entry {
        code: "K0600",
        title: ["TYPES DON'T MATCH", "الأنواع غير متوافقة", "EL TYPES MESH MAZBOOTA"],
        message: [
            "The type of this value doesn't fit the place where it is used. Kalam never converts between types by itself, so the two have to agree exactly.",
            "نوع هذه القيمة لا يناسب المكان الذي استُخدمت فيه. لا يحوّل كلام بين الأنواع من تلقاء نفسه، فلا بد أن يتطابق النوعان تمامًا.",
            "No3 el qeema di mesh mnaseb el makan elly etsta5demet feeh. Kalam 3omro ma by7awel ben el types lewa7do, fa lazem el etnein yetaba2o bel zabt.",
        ],
        help: [
            "Check what this place needs (a parameter, a return type, an annotation, a function to call or something to loop over); the note under the code says which types don't fit. Then change the value, or convert it explicitly, for example with `.to_float()` or `.round()` between number types.",
            "تحقق مما يحتاجه هذا المكان (معامل، أو نوع إرجاع، أو تعليق نوع، أو دالة تُستدعى، أو شيء يُكرَّر عليه)؛ والملاحظة تحت الكود تذكر النوعين غير المتوافقين. ثم غيّر القيمة أو حوّلها صراحة، مثلًا بـ `.to_float()` أو `.round()` بين أنواع الأرقام.",
            "Shoof el makan da me7tag eh (parameter, aw return type, aw annotation, aw dala tet3amalha call, aw 7aga te3mel 3aleiha loop); el molaheza ta7t el code bet2ol anhy anwa3 mesh mtab2a. W ba3dein ghayar el qeema aw 7awelha sara7a, masalan b `.to_float()` aw `.round()` ben anwa3 el arqam.",
        ],
        explain: [
            "Why: every value has exactly one type, and every place (a parameter, a variable with an annotation, a return) says which type it accepts. When the two differ, guessing a conversion could lose data or hide a bug, so I stop and show both.\n\nInstead of:\n    fn half(x: Float) -> Float:\n        return x / 2.0\n    let count: Int = 3\n    let h = half(count)\n\nwrite:\n    let h = half(count.to_float())",
            "السبب: لكل قيمة نوع واحد بالضبط، وكل مكان (معامل، أو متغير بتعليق نوع، أو إرجاع) يحدد النوع الذي يقبله. فإذا اختلف النوعان فإن تخمين تحويل قد يضيّع بيانات أو يخفي خطأ، لذلك أتوقف وأعرض النوعين.\n\nبدلًا من:\n    دالة نصف(س: كسري) -> كسري:\n        أرجع س / 2.0\n    ليكن العدد: عدد = 3\n    ليكن ن = نصف(العدد)\n\nاكتب:\n    ليكن ن = نصف(العدد.to_float())",
            "Leh: kol qeema leeha no3 wa7ed bel zabt, w kol makan (parameter, aw motghayer b annotation, aw return) bey2ool beyqbal anhy no3. Law el etnein mokhtalfeen, takhmeen ay ta7weel momken ydaya3 data aw ykhabby ghalta, 3ashan keda bawa2af w a3redlak el etnein.\n\nBadal:\n    dala nos(x: kasri) -> kasri:\n        rag3 x / 2.0\n    khali 3add: adad = 3\n    khali n = nos(3add)\n\nEkteb:\n    khali n = nos(3add.to_float())",
        ],
        proposed: true,
    },
    Entry {
        code: "K0601",
        title: ["NUMBER TYPES DON'T MIX", "أنواع الأرقام لا تختلط", "ANWA3 EL ARQAM MABTETKHALATSH"],
        message: [
            "I can't combine a value of type `{left}` with one of type `{right}`. Kalam never converts numbers by itself, so you choose how.",
            "لا أستطيع الجمع بين `{left}` و`{right}`. لا يحوّل كلام الأرقام من تلقاء نفسه، فأنت من يختار طريقة التحويل.",
            "Mesh ha2dar agma3 ben `{left}` w `{right}`. Kalam 3omro ma by7awel el arqam lewa7do, fa enta elly tekhtar ezay.",
        ],
        help: [
            "Convert one side explicitly: `.to_float()` keeps the fraction, `.round()` drops it.",
            "حوّل أحد الطرفين صراحة: `.to_float()` تحتفظ بالكسر، و`.round()` تحذفه.",
            "7awel taraf wa7ed sara7a: `.to_float()` bete7tafez bel kasr, w `.round()` betsheelo.",
        ],
        explain: [
            "Why: silent numeric conversions lose information (a fraction, a large value, decimal exactness) without telling you. A plain number literal adapts to the other side, so `1 + 2.5` is a `Float`; variables never convert by themselves.\n\nInstead of:\n    let total = count + price\n\nwrite one of:\n    let total = count.to_float() + price     # keep the fraction\n    let total = count + price.round()        # drop the fraction",
            "السبب: التحويل الصامت بين الأرقام يضيّع معلومات (كسرًا، أو قيمة كبيرة، أو دقة عشرية) دون أن يخبرك. والرقم المكتوب مباشرة يتكيف مع الطرف الآخر، فيكون `1 + 2.5` من النوع `كسري`، أما المتغيرات فلا تتحول من تلقاء نفسها أبدًا.\n\nبدلًا من:\n    ليكن المجموع = العدد + السعر\n\nاكتب أحد السطرين:\n    ليكن المجموع = العدد.to_float() + السعر     # تحتفظ بالكسر\n    ليكن المجموع = العدد + السعر.round()        # تحذف الكسر",
            "Leh: el ta7weel el saket ben el arqam bydaya3 ma3lomat (kasr, aw qeema kebeera, aw da22a 3ashreya) men gher ma y2ollak. El rakam el maktoob 3ala tool beyetkayef ma3 el taraf el tany, fa `1 + 2.5` yeb2a `kasri`; laken el motghayerat 3omraha ma tet7awel lewa7daha.\n\nBadal:\n    khali magmoo3 = 3add + se3r\n\nEkteb wa7ed men dol:\n    khali magmoo3 = 3add.to_float() + se3r     # bte7tafez bel kasr\n    khali magmoo3 = 3add + se3r.round()        # betsheel el kasr",
        ],
        proposed: false,
    },
    Entry {
        code: "K0602",
        title: ["COMPARING DIFFERENT TYPES", "مقارنة أنواع مختلفة", "MOQARNA BEN TYPES MOKHTALFA"],
        message: [
            "I can't compare a value of type `{left}` with one of type `{right}` using `==`. Equality needs two values of the same type, and that type must support equality (`Eq`).",
            "لا أستطيع مقارنة `{left}` بـ `{right}` باستخدام `==`. المساواة تحتاج قيمتين من النوع نفسه، ويجب أن يدعم هذا النوع المساواة (`Eq`).",
            "Mesh ha2dar a2aren `{left}` b `{right}` b `==`. El mosawah me7taga qeemtein men nafs el no3, w el no3 da lazem yed3am el mosawah (`Eq`).",
        ],
        help: [
            "Convert one side so both have the same type, or compare fields that support equality. Compare secrets with `crypto.equal(a, b)`.",
            "حوّل أحد الطرفين ليصبح للطرفين النوع نفسه، أو قارن حقولًا تدعم المساواة. وقارن الأسرار بـ `crypto.equal(a, b)`.",
            "7awel taraf 3ashan el etnein yeb2o nafs el no3, aw qaren fields bted3am el mosawah. W qaren el asrar b `crypto.equal(a, b)`.",
        ],
        explain: [
            "Why: in many languages `1 == \"1\"` quietly answers false, or true, and either way the comparison hides a bug. In Kalam, `==` only compares values of one type that supports equality. `Secret` values have no `==` at all: `crypto.equal` compares them in constant time.\n\nInstead of:\n    if order.id == input:\n\nwrite:\n    if order.id == input.to_int()?:",
            "السبب: في لغات كثيرة يجيب `1 == \"1\"` بخطأ أو بصح دون أن ينبهك، وفي الحالتين تخفي المقارنة خطأً. أما في كلام فالعامل `==` يقارن فقط قيمًا من نوع واحد يدعم المساواة. وقيم `Secret` ليس لها `==` أصلًا، بل تقارنها `crypto.equal` في زمن ثابت.\n\nبدلًا من:\n    إذا الطلب.الرقم == المدخل:\n\nاكتب:\n    إذا الطلب.الرقم == المدخل.to_int()؟:",
            "Leh: f loghat keteer `1 == \"1\"` betrod ghalat aw sa7 men gher ma tenabbahak, w f el 7altein el moqarna betkhabby ghalta. F Kalam, `==` bey2aren bas qeyam men no3 wa7ed byed3am el mosawah. W qeyam `Secret` malhash `==` aslan: `crypto.equal` bet2arenhom f wa2t sabet.\n\nBadal:\n    law order.id == input:\n\nEkteb:\n    law order.id == input.to_int()?:",
        ],
        proposed: false,
    },
    Entry {
        code: "K0603",
        title: ["OPERATION NOT DEFINED FOR TYPE", "عملية غير معرّفة لهذا النوع", "3AMALEYA MESH MAWGOODA LEL TYPE"],
        message: [
            "`{type}` doesn't support what this code asks of it: the operator or function here needs a trait (such as `Ord` for `<`, or `Show` for printing) that `{type}` doesn't have.",
            "النوع `{type}` لا يدعم ما يطلبه هذا الكود: العامل أو الدالة هنا تحتاج سمة (مثل `Ord` للعامل `<`، أو `Show` للطباعة) ليست في `{type}`.",
            "`{type}` mesh byed3am elly el code da 3ayzo: el operator aw el dala hena me7taga trait (zay `Ord` lel `<`, aw `Show` lel teba3a) mesh mawgooda f `{type}`.",
        ],
        help: [
            "Use a value whose type supports it, or give `{type}` the trait: add it to `@derive(...)`, or write `{kw_impl} {type} {kw_as} Show:` with the methods.",
            "استخدم قيمة يدعم نوعها ذلك، أو أعطِ `{type}` السمة: أضفها إلى `@derive(...)`، أو اكتب `{kw_impl} {type} {kw_as} Show:` مع دوالها.",
            "Esta5dem qeema no3aha byed3am da, aw edy `{type}` el trait: zawedha f `@derive(...)`, aw ekteb `{kw_impl} {type} {kw_as} Show:` ma3 el methods bta3etha.",
        ],
        explain: [
            "Why: an operator means something only for types that define it. `<` needs an order (`Ord`), printing needs a text form (`Show`), and `/` is not defined where it would silently round. Saying which trait is missing tells you exactly what to add.\n\nInstead of:\n    type Point:\n        x: Int\n        y: Int\n    print(\"{ p }\")\n\nwrite:\n    @derive(Show)\n    type Point:\n        x: Int\n        y: Int",
            "السبب: للعامل معنى فقط في الأنواع التي تعرّفه. فالعامل `<` يحتاج ترتيبًا (`Ord`)، والطباعة تحتاج صورة نصية (`Show`)، والعامل `/` غير معرّف حيث كان سيقرّب بصمت. وذكر السمة الناقصة يخبرك بالضبط بما يجب إضافته.\n\nبدلًا من:\n    هيكل نقطة:\n        س: عدد\n        ص: عدد\n    اطبع(\"{ ن }\")\n\nاكتب:\n    @derive(Show)\n    هيكل نقطة:\n        س: عدد\n        ص: عدد",
            "Leh: el operator leeh ma3na bas f el types elly bet3arrafo. `<` me7tag tarteeb (`Ord`), w el teba3a me7taga shakl nassy (`Show`), w `/` mesh mota3araf f el amaken elly kan hay2arrab feeha f el sekket. W lamma a2ollak anhy trait na2es, betb2a 3aref bel zabt te-zawed eh.\n\nBadal:\n    haykal No2ta:\n        x: adad\n        y: adad\n    ektb(\"{ p }\")\n\nEkteb:\n    @derive(Show)\n    haykal No2ta:\n        x: adad\n        y: adad",
        ],
        proposed: true,
    },
    Entry {
        code: "K0612",
        title: ["SECRET IN AN ENCODED TYPE", "سرّ في نوع قابل للترميز", "SECRET F TYPE BYET3AMALLO ENCODE"],
        message: [
            "This type derives `Encode`, but one of its fields is a `Secret`. Encoding it would write the secret into JSON or onto the wire, so I stop here rather than hide it behind a redaction.",
            "هذا النوع يشتق `Encode`، لكن أحد حقوله `Secret`. وترميزه كان سيكتب السر في JSON أو يرسله عبر الشبكة، لذلك أتوقف هنا بدل أن أخفيه بتمويه.",
            "El no3 da bey3mel derive l `Encode`, bas field menno `Secret`. W el encode kan hayekteb el secret f JSON aw yeb3ato 3al network, 3ashan keda bawa2af hena badal ma akhabbih b redaction.",
        ],
        help: [
            "Remove `Encode` from the `@derive` list, or define a separate output type without the secret field and encode that one.",
            "احذف `Encode` من قائمة `@derive`، أو عرّف نوع مخرجات منفصلًا بلا حقل السر ورمّزه هو.",
            "Shil `Encode` men list el `@derive`, aw 3arraf no3 output monfasel men gher field el secret w e3mel encode leeh howa.",
        ],
        explain: [
            "Why: a secret that reaches JSON is a leak, whether it lands in a log, an API response or a cache. `Secret` has no `Encode` on purpose, so the leak is a compile error instead of a value that is quietly blanked out (spec §18.1).\n\nInstead of:\n    @derive(Encode)\n    type Account:\n        email: String\n        api_key: Secret[String]\n\nwrite:\n    type Account:\n        email: String\n        api_key: Secret[String]\n    @derive(Encode)\n    type AccountView:\n        email: String",
            "السبب: السر الذي يصل إلى JSON تسريب، سواء انتهى في سجل أو في رد واجهة أو في ذاكرة مؤقتة. وليس للنوع `Secret` ترميز `Encode` عن قصد، فيكون التسريب خطأ ترجمة لا قيمة تُمحى بصمت (المواصفة §18.1).\n\nبدلًا من:\n    @derive(Encode)\n    هيكل حساب:\n        البريد: نص\n        المفتاح: Secret[نص]\n\nاكتب:\n    هيكل حساب:\n        البريد: نص\n        المفتاح: Secret[نص]\n    @derive(Encode)\n    هيكل عرض_الحساب:\n        البريد: نص",
            "Leh: el secret elly yewsal JSON yeb2a tasreeb, sawa2 khallas f log aw f response aw f cache. W `Secret` malosh `Encode` 3an 2asd, fa el tasreeb yeb2a compile error mesh qeema btetmese7 f el sekket (el spec §18.1).\n\nBadal:\n    @derive(Encode)\n    haykal Account:\n        email: nass\n        api_key: Secret[nass]\n\nEkteb:\n    haykal Account:\n        email: nass\n        api_key: Secret[nass]\n    @derive(Encode)\n    haykal AccountView:\n        email: nass",
        ],
        proposed: false,
    },
    Entry {
        code: "K0650",
        title: ["STRINGS CAN'T BE INDEXED", "النص لا يُفهرس", "EL STRING MABTET3AMALHASH INDEX"],
        message: [
            "I can't index a string with `[...]`. A position in text is ambiguous: bytes, code points and visible characters all count differently, and in Arabic text the difference shows.",
            "لا أستطيع فهرسة نص بـ `[...]`. الموضع في النص غامض: البايتات ونقاط الترميز والحروف الظاهرة كلها تُعدّ بطرق مختلفة، ويظهر الفرق في النص العربي.",
            "Mesh ha2dar a3mel index l string b `[...]`. El makan f el kalam mesh wade7: el bytes wel code points wel 7oroof el bayna kol wa7ed byet3ad b shakl, w f el kalam el 3araby el far2 bayen.",
        ],
        help: [
            "Use `.chars()` or `.graphemes()` to go through the characters, or `.slice(from, to)` to take part of the string (positions count visible characters).",
            "استخدم `.chars()` أو `.graphemes()` للمرور على الحروف، أو `.slice(from, to)` لأخذ جزء من النص (والمواضع تعدّ الحروف الظاهرة).",
            "Esta5dem `.chars()` aw `.graphemes()` 3ashan temshy 3ala el 7oroof, aw `.slice(from, to)` 3ashan takhod 7etta men el string (el amaken bet3ed el 7oroof el bayna).",
        ],
        explain: [
            "Why: `s[3]` means a different thing in every language: the fourth byte, the fourth UTF-16 unit or the fourth code point. With Arabic harakat or an emoji, none of these is the fourth character a reader sees. Kalam asks you to say which one you mean.\n\nInstead of:\n    let first = name[0]\n\nwrite:\n    let first = name.slice(0, 1)",
            "السبب: `s[3]` يعني شيئًا مختلفًا في كل لغة: البايت الرابع، أو وحدة UTF-16 الرابعة، أو نقطة الترميز الرابعة. ومع الحركات العربية أو الرموز التعبيرية لا يكون أي منها الحرف الرابع الذي يراه القارئ. لذلك يطلب منك كلام أن تحدد ما تقصده.\n\nبدلًا من:\n    ليكن الأول = الاسم[0]\n\nاكتب:\n    ليكن الأول = الاسم.slice(0، 1)",
            "Leh: `s[3]` ma3naha 7aga mokhtalfa f kol logha: el byte el rabe3, aw el UTF-16 unit el rab3a, aw el code point el rabe3. W ma3 el tashkeel el 3araby aw el emoji, wala wa7da menhom heya el 7arf el rabe3 elly el 2ari beyshoofo. 3ashan keda Kalam 3ayzak t2ool ana 2asdak anhy wa7da.\n\nBadal:\n    khali awel = esm[0]\n\nEkteb:\n    khali awel = esm.slice(0, 1)",
        ],
        proposed: false,
    },
    // ---- Statements and inference (K07xx, K08xx, spec §10; kalam-typeck). ar/franco: pending
    // native review.
    Entry {
        code: "K0730",
        title: ["UNUSED VALUE", "قيمة غير مستخدمة", "QEEMA MESH MOSTA5DAMA"],
        message: [
            "This expression gives a value of type `{type}`, and nothing uses it. A value that is computed and dropped is usually a mistake, such as calling `items.append(x)` and expecting `items` to change.",
            "هذا التعبير يعطي قيمة من النوع `{type}`، ولا شيء يستخدمها. والقيمة التي تُحسب ثم تُترك غالبًا خطأ، مثل استدعاء `items.append(x)` وتوقّع أن تتغير `items`.",
            "El expression da bydy qeema men no3 `{type}`, w mafeesh 7aga betesta5demha. W el qeema elly btet7seb w tetrmy ghaleban ghalta, zay ennak tenady `items.append(x)` w testanna `items` tetghayar.",
        ],
        help: [
            "Use the value, for example keep it with `{kw_let}` or assign it back, or write `{kw_let} _ = ...` to drop it on purpose.",
            "استخدم القيمة، مثلًا احفظها بـ `{kw_let}` أو أسندها من جديد، أو اكتب `{kw_let} _ = ...` لتتركها عن قصد.",
            "Esta5dem el qeema, masalan e7fazha b `{kw_let}` aw raga3ha lel motghayer, aw ekteb `{kw_let} _ = ...` 3ashan tesebha 3an 2asd.",
        ],
        explain: [
            "Why: values in Kalam are immutable, so methods such as `append` or `trim` return a new value instead of changing the old one. Calling one and ignoring the result does nothing, and that is almost never what was meant.\n\nInstead of:\n    items.append(x)\n\nwrite (with `items` declared as `var`):\n    items = items.append(x)\n\nor, to drop a value on purpose:\n    let _ = compute_total(cart)",
            "السبب: القيم في كلام لا تتغير، فالدوال مثل `append` أو `trim` ترجع قيمة جديدة ولا تغيّر القديمة. واستدعاء إحداها مع تجاهل النتيجة لا يفعل شيئًا، ونادرًا ما يكون هذا هو المقصود.\n\nبدلًا من:\n    العناصر.append(س)\n\nاكتب (مع تعريف `العناصر` بـ `متغير`):\n    العناصر = العناصر.append(س)\n\nأو لترك قيمة عن قصد:\n    ليكن _ = احسب_المجموع(السلة)",
            "Leh: el qeyam f Kalam mabtetghayarsh, fa methods zay `append` aw `trim` betraga3 qeema gedeeda w mesh betghayar el adeema. W ennak tenady wa7da w tetgahel el natiga mesh bey3mel 7aga, w nadran ma yeb2a da el ma2sood.\n\nBadal:\n    items.append(x)\n\nEkteb (w `items` mota3arafa b `motghayer`):\n    items = items.append(x)\n\naw, 3ashan tesseb qeema 3an 2asd:\n    khali _ = compute_total(cart)",
        ],
        proposed: false,
    },
    Entry {
        code: "K0801",
        title: ["LAMBDA PARAMETER TYPES UNKNOWN", "أنواع معاملات الدالة المجهولة غير معروفة", "TYPES EL LAMBDA MESH MA3ROOFA"],
        message: [
            "I can't tell the types of this lambda's parameters. A lambda takes its parameter types from the place it is passed to, and nothing here says what they are.",
            "لا أستطيع معرفة أنواع معاملات هذه الدالة المجهولة. تأخذ الدالة المجهولة أنواع معاملاتها من المكان الذي تُمرَّر إليه، ولا شيء هنا يحددها.",
            "Mesh 3aref a7added types el parameters bta3et el lambda di. El lambda betakhod types parameters-ha men el makan elly etba3atet leeh, w mafeesh 7aga hena bet2ool homa eh.",
        ],
        help: [
            "Write the parameter types, for example `{kw_fn}(x: {core_int}) => x + 1`, or pass the lambda directly to a function that expects one.",
            "اكتب أنواع المعاملات، مثل `{kw_fn}(x: {core_int}) => x + 1`، أو مرّر الدالة المجهولة مباشرة إلى دالة تنتظرها.",
            "Ekteb types el parameters, zay `{kw_fn}(x: {core_int}) => x + 1`, aw ba3at el lambda 3ala tool l dala mestanyaha.",
        ],
        explain: [
            "Why: Kalam infers types inside a function body, but only from what is known at each point. A lambda stored in a variable has no expected function type to learn from, and guessing would make the meaning of the code depend on its later uses.\n\nInstead of:\n    let double = fn(x) => x * 2\n\nwrite:\n    let double = fn(x: Int) => x * 2\n\nor pass it where its type is known:\n    let doubled = numbers.map(fn(x) => x * 2)",
            "السبب: يستنتج كلام الأنواع داخل جسم الدالة، لكن فقط مما هو معروف عند كل نقطة. والدالة المجهولة المحفوظة في متغير ليس لها نوع دالة متوقع تتعلم منه، والتخمين كان سيجعل معنى الكود معتمدًا على استخداماته اللاحقة.\n\nبدلًا من:\n    ليكن ضعف = دالة(س) => س * 2\n\nاكتب:\n    ليكن ضعف = دالة(س: عدد) => س * 2\n\nأو مرّرها حيث يكون نوعها معروفًا:\n    ليكن المضاعف = الأرقام.map(دالة(س) => س * 2)",
            "Leh: Kalam byestantag el types gowa gesm el dala, bas men elly ma3roof 3and kol no2ta. W el lambda el mat7oota f motghayer malhash no3 dala mestanny tet3allem menno, wel takhmeen kan hayekhalli ma3na el code mo3tamed 3ala esta5damato ba3dein.\n\nBadal:\n    khali do3f = dala(x) => x * 2\n\nEkteb:\n    khali do3f = dala(x: adad) => x * 2\n\naw ba3atha f makan no3aha ma3roof feeh:\n    khali mod3af = numbers.map(dala(x) => x * 2)",
        ],
        proposed: false,
    },
    Entry {
        code: "K0802",
        title: ["EMPTY COLLECTION NEEDS A TYPE", "مجموعة فارغة تحتاج نوعًا", "COLLECTION FADYA ME7TAGA TYPE"],
        message: [
            "I can't tell what this empty collection will hold. An empty `[]` or `{}` has no element type of its own, and nothing around it says what it should be.",
            "لا أستطيع معرفة ما ستحويه هذه المجموعة الفارغة. فالمجموعة الفارغة `[]` أو `{}` ليس لها نوع عناصر في ذاتها، ولا شيء حولها يحدده.",
            "Mesh 3aref el collection el fadya di hatsheel eh. `[]` aw `{}` el fadyeen malhomsh no3 3anaser lewa7dohom, w mafeesh 7aga 7awaleihom bet2ool lazem yeb2o eh.",
        ],
        help: [
            "Add a type, for example `{kw_let} names: {core_list}[{core_string}] = []`.",
            "أضف نوعًا، مثل `{kw_let} الأسماء: {core_list}[{core_string}] = []`.",
            "Zawed type, zay `{kw_let} asamy: {core_list}[{core_string}] = []`.",
        ],
        explain: [
            "Why: the element type decides what you may add later and which methods exist. If I guessed it from later lines, moving one line could change the type of the whole collection. One annotation keeps it fixed.\n\nInstead of:\n    var names = []\n\nwrite:\n    var names: List[String] = []",
            "السبب: نوع العناصر يحدد ما يمكنك إضافته لاحقًا وما الدوال المتاحة. ولو خمّنته من الأسطر اللاحقة لأمكن أن يغيّر نقلُ سطر واحد نوعَ المجموعة كلها. وتعليق نوع واحد يثبّته.\n\nبدلًا من:\n    متغير الأسماء = []\n\nاكتب:\n    متغير الأسماء: قائمة[نص] = []",
            "Leh: no3 el 3anaser howa elly bey7added te2dar tzawed eh ba3dein w anhy methods mawgooda. W law khammanto men el sotoor elly ba3d, na2l satr wa7ed momken yghayar no3 el collection kollaha. W annotation wa7da betsabbeto.\n\nBadal:\n    motghayer asamy = []\n\nEkteb:\n    motghayer asamy: qayma[nass] = []",
        ],
        proposed: false,
    },
    Entry {
        code: "K0803",
        title: ["SHORT MEMBER NEEDS A TYPE", "اختصار العضو يحتاج نوعًا", "EL MEMBER EL MOKHTASAR ME7TAG TYPE"],
        message: [
            "I can't tell which type `.{name}` belongs to. A short member such as `.{name}` is looked up in the type expected at this place, and nothing here says which type that is.",
            "لا أستطيع معرفة النوع الذي ينتمي إليه `.{name}`. يُبحث عن العضو المختصر مثل `.{name}` في النوع المتوقع في هذا المكان، ولا شيء هنا يحدد هذا النوع.",
            "Mesh 3aref `.{name}` tab3a anhy type. El member el mokhtasar zay `.{name}` byetdawar 3aleih f el type el mestanny f el makan da, w mafeesh 7aga hena bet2ool howa anhy type.",
        ],
        help: [
            "Write the type in front, `TypeName.{name}`, or add a type annotation so the expected type is known.",
            "اكتب النوع قبله، `اسم_النوع.{name}`، أو أضف تعليق نوع ليصبح النوع المتوقع معروفًا.",
            "Ekteb el type 2oddamo, `EsmElType.{name}`, aw zawed annotation 3ashan el type el mestanny yeb2a ma3roof.",
        ],
        explain: [
            "Why: the leading dot is a shortcut that works because the expected type is known, as in a `match` arm or an annotated variable. Without one, several enums could have a variant with that name, and picking one would be a guess.\n\nInstead of:\n    let s = .Circle(radius: 1.0)\n\nwrite:\n    let s: Shape = .Circle(radius: 1.0)\n\nor:\n    let s = Shape.Circle(radius: 1.0)",
            "السبب: النقطة في البداية اختصار يعمل لأن النوع المتوقع معروف، كما في فرع `حسب` أو متغير بتعليق نوع. وبدونه قد يكون في عدة تعدادات عضو بهذا الاسم، واختيار أحدها تخمين.\n\nبدلًا من:\n    ليكن ش = .دائرة(نصف_القطر: 1.0)\n\nاكتب:\n    ليكن ش: شكل = .دائرة(نصف_القطر: 1.0)\n\nأو:\n    ليكن ش = شكل.دائرة(نصف_القطر: 1.0)",
            "Leh: el no2ta f el awel ekhtesar byeshtaghal 3ashan el type el mestanny ma3roof, zay f far3 `hasab` aw motghayer b annotation. Men gheiro momken kaza enum yeb2a feehom variant bel esm da, w ekhtiyar wa7ed yeb2a takhmeen.\n\nBadal:\n    khali s = .Circle(radius: 1.0)\n\nEkteb:\n    khali s: Shape = .Circle(radius: 1.0)\n\naw:\n    khali s = Shape.Circle(radius: 1.0)",
        ],
        proposed: false,
    },
    // K0804: proposed (M2 iteration 4, kalam-typeck). A type nothing fixes is `Never` and
    // harmless, unless a trait requirement depends on it (spec §9.9 rules 4–5: the choice
    // would change behavior) or it is part of a module constant's type (§9.9 rule 1: other
    // code uses it). The specific reason is a label. ar/franco: pending native review.
    Entry {
        code: "K0804",
        title: ["TYPE CAN'T BE WORKED OUT", "لا يمكن استنتاج النوع", "MESH 3AREF ASTANTEG EL TYPE"],
        message: [
            "I can't tell which type this is: all I know is `{type}`, and nothing in this code says what `_` stands for. The marked code needs that type, so I won't guess it.",
            "لا أستطيع معرفة هذا النوع: كل ما أعرفه أنه `{type}`، ولا شيء في هذا الكود يحدد ما يمثله `_`. والكود المعلَّم يحتاج هذا النوع، فلن أخمّنه.",
            "Mesh 3aref el type da: kol elly a3rafo enno `{type}`, w mafeesh 7aga f el code bet2ool `_` ma3naha eh. El code el mt3allem me7tag el type da, fa mesh hakhammeno.",
        ],
        help: [
            "Write the type where the value is bound, for example `{kw_let} config: Config = json.decode(text){punct_try}`. A module constant takes its type from its value alone, so write it on the constant: `{kw_let} NOTHING: {core_option}[{core_int}] = {core_none}`.",
            "اكتب النوع حيث تُربط القيمة، مثل `{kw_let} الإعداد: Config = json.decode(النص){punct_try}`. وثابت الوحدة يأخذ نوعه من قيمته وحدها، فاكتب النوع على الثابت: `{kw_let} بلا_قيمة: {core_option}[{core_int}] = {core_none}`.",
            "Ekteb el type f el makan elly el qeema betet3arraf feeh, zay `{kw_let} config: Config = json.decode(text){punct_try}`. W sabet el module beyakhod no3o men qeemto bas, fa ekteb el type 3al sabet: `{kw_let} NOTHING: {core_option}[{core_int}] = {core_none}`.",
        ],
        explain: [
            "Why: some code does different things for different types. `json.decode` reads text into the type you ask for, and printing a value shows it the way its type does. When nothing in the code says which type is meant, picking one would be a guess that changes what the program does, so the type has to be written (spec §9.9). A module constant is used by other code, so its whole type must come from its value (§9.9 rule 1). A type that nothing needs is no problem: the error type of `Ok(1)` in a function, when nothing looks at the error, for example.

Instead of:
    let config = json.decode(text)?
    let NOTHING = None

write:
    let config: Config = json.decode(text)?
    let NOTHING: Option[Int] = None",
            "السبب: بعض الكود يتصرف بحسب النوع. فـ `json.decode` تقرأ النص إلى النوع الذي تطلبه، وطباعة قيمة تعرضها كما يعرضها نوعها. فإذا لم يذكر الكود أي نوع هو المقصود، كان اختيار أحدها تخمينًا يغيّر ما يفعله البرنامج، لذلك يجب أن يُكتب النوع (المواصفة §9.9). وثابت الوحدة يستخدمه كود آخر، فلا بد أن يأتي نوعه كله من قيمته (§9.9 القاعدة 1). أما النوع الذي لا يحتاجه شيء فلا مشكلة فيه، مثل نوع الخطأ في `نجاح(1)` داخل دالة حين لا ينظر شيء إلى الخطأ.

بدلًا من:
    ليكن الإعداد = json.decode(النص)؟
    ليكن بلا_قيمة = لا_شيء

اكتب:
    ليكن الإعداد: Config = json.decode(النص)؟
    ليكن بلا_قيمة: اختياري[عدد] = لا_شيء",
            "Leh: fe code bey3mel 7agat mokhtalfa 3ala 7asab el type. `json.decode` bte2ra el nass w t7awelo lel type elly enta talbo, w tab3et qeema bet3redha zay ma el type bta3ha bey3redha. Law mafeesh 7aga f el code bet2ool anhy type el ma2sood, ekhtiyar wa7ed yeb2a takhmeen beyghayar el program bey3mel eh, 3ashan keda lazem el type yetketeb (el spec §9.9). W sabet el module code tany beyesta5demo, fa lazem no3o kollo yeegy men qeemto (§9.9 2a3da 1). Ama el type elly mafeesh 7aga me7tagah fa mafeehoosh moshkela, zay type el error f `naga7(1)` gowa dala lamma mafeesh 7aga betbos 3al error.

Badal:
    khali config = json.decode(text)?
    khali NOTHING = mafeesh

Ekteb:
    khali config: Config = json.decode(text)?
    khali NOTHING: ekhtiyari[adad] = mafeesh",
        ],
        proposed: true,
    },
    // ---- Patterns (K10xx, spec §11; kalam-typeck). ar/franco: pending native review.
    Entry {
        code: "K1001",
        title: ["MATCH DOESN'T COVER EVERY CASE", "المطابقة لا تغطي كل الحالات", "EL MATCH MESH GAYEB KOL EL 7ALAT"],
        message: [
            "This `{kw_match}` doesn't handle every possible value. Missing: {missing}.",
            "هذه `{kw_match}` لا تعالج كل القيم الممكنة. الناقص: {missing}.",
            "El `{kw_match}` da mesh bey3alag kol el qeyam el momkena. El na2es: {missing}.",
        ],
        help: [
            "Add a `{kw_case}` for each missing value. Listing them is better than a catch-all `_`: a variant added later is then reported here instead of slipping into the catch-all.",
            "أضف `{kw_case}` لكل قيمة ناقصة. وذكرها أفضل من فرع عام `_`، فالعضو الذي يضاف لاحقًا يُبلَّغ عنه هنا بدل أن يذهب بصمت إلى الفرع العام.",
            "Zawed `{kw_case}` l kol qeema na2sa. W enak tektebhom a7san men far3 3am `_`: el variant elly yetzad ba3dein yetballagh 3anno hena badal ma yerou7 lel far3 el 3am f el sekket.",
        ],
        explain: [
            "Why: a value that no arm handles would have nowhere to go at run time. Checking every variant at compile time also means that adding a variant to an enum shows you every `match` that must now handle it (spec §11).\n\nInstead of:\n    match shape:\n        case .Circle(r): 3.14159 * r * r\n\nwrite:\n    match shape:\n        case .Circle(r): 3.14159 * r * r\n        case .Rect(w, h): w * h",
            "السبب: القيمة التي لا يعالجها أي فرع لن يكون لها مكان تذهب إليه وقت التشغيل. والتحقق من كل عضو وقت الترجمة يعني أيضًا أن إضافة عضو إلى تعداد تريك كل `حسب` يجب أن تعالجه الآن (المواصفة §11).\n\nبدلًا من:\n    حسب الشكل:\n        عندما .دائرة(ن): 3.14159 * ن * ن\n\nاكتب:\n    حسب الشكل:\n        عندما .دائرة(ن): 3.14159 * ن * ن\n        عندما .مستطيل(ع، ط): ع * ط",
            "Leh: el qeema elly mafeesh far3 bey3alagha mesh hayeb2a leeha makan terou7lo w el program shaghal. W el check 3ala kol variant w2t el compile ma3nah kaman en lamma tzawed variant l enum, hatshoof kol `hasab` lazem t3alego delwa2ty (el spec §11).\n\nBadal:\n    hasab shape:\n        lama .Circle(r): 3.14159 * r * r\n\nEkteb:\n    hasab shape:\n        lama .Circle(r): 3.14159 * r * r\n        lama .Rect(w, h): w * h",
        ],
        proposed: false,
    },
    Entry {
        code: "K1003",
        title: ["PATTERN CAN FAIL HERE", "النمط قد لا يطابق هنا", "EL PATTERN MOMKEN MAYETABE2SH HENA"],
        message: [
            "This pattern doesn't match every value, but `{kw_let}` and `{kw_for}` have nowhere to go when a pattern fails.",
            "هذا النمط لا يطابق كل القيم، لكن `{kw_let}` و`{kw_for}` ليس لهما مكان يذهبان إليه حين لا يطابق النمط.",
            "El pattern da mesh byetabe2 kol el qeyam, bas `{kw_let}` w `{kw_for}` malhomsh makan yerou7olo lamma el pattern mayetabe2sh.",
        ],
        help: [
            "Use `{kw_match}` with a `{kw_case}` for each possibility.",
            "استخدم `{kw_match}` مع `{kw_case}` لكل احتمال.",
            "Esta5dem `{kw_match}` ma3 `{kw_case}` l kol e7temal.",
        ],
        explain: [
            "Why: a pattern such as `Some(x)` only matches some values. In a `match`, the other arms say what happens otherwise; in a `let` there is no otherwise, so a failed match would have to crash.\n\nInstead of:\n    let Some(user) = find(id)\n\nwrite:\n    match find(id):\n        case Some(user): greet(user)\n        case None: print(\"no such user\")",
            "السبب: النمط مثل `موجود(س)` يطابق بعض القيم فقط. وفي `حسب` تقول الفروع الأخرى ما يحدث في غير ذلك، أما في `ليكن` فلا يوجد غير ذلك، فكان عدم المطابقة سيوقف البرنامج.\n\nبدلًا من:\n    ليكن موجود(المستخدم) = ابحث(الرقم)\n\nاكتب:\n    حسب ابحث(الرقم):\n        عندما موجود(المستخدم): رحب(المستخدم)\n        عندما لا_شيء: اطبع(\"لا يوجد مستخدم\")",
            "Leh: pattern zay `mawgood(x)` byetabe2 ba3d el qeyam bas. F `hasab` el foroo3 el tanya bet2ool eh elly ye7sal gher keda; laken f `khali` mafeesh gher keda, fa 3adam el mutab2a kan hayewa22a3 el program.\n\nBadal:\n    khali mawgood(user) = dawar(id)\n\nEkteb:\n    hasab dawar(id):\n        lama mawgood(user): rahhab(user)\n        lama mafeesh: ektb(\"mafeesh user\")",
        ],
        proposed: false,
    },
    Entry {
        code: "K1005",
        title: ["FLOAT LITERAL PATTERN", "نمط برقم كسري", "PATTERN B RAKAM KASRI"],
        message: [
            "I can't match on a `{core_float}` literal. Floating-point arithmetic rounds, so `0.1 + 0.2` is not exactly `0.3`, and an arm for `0.3` would quietly never match.",
            "لا أستطيع المطابقة على رقم `{core_float}` مكتوب. حساب الأرقام الكسرية يقرّب، فلا يساوي `0.1 + 0.2` القيمة `0.3` بالضبط، وكان فرع `0.3` لن يطابق أبدًا دون أن تلاحظ.",
            "Mesh ha2dar a3mel match 3ala rakam `{core_float}` maktoob. 7esab el arqam el kasreya bey2arrab, fa `0.1 + 0.2` mesh `0.3` bel zabt, w far3 l `0.3` 3omro ma kan hayetabe2 men gher ma takhod balak.",
        ],
        help: [
            "Compare with a tolerance in a guard, such as `{kw_case} x {kw_if} x.near(0.3, tol: 0.0001):`, or use `{core_decimal}` for exact values.",
            "قارن بهامش سماح في شرط الفرع، مثل `{kw_case} x {kw_if} x.near(0.3, tol: 0.0001):`، أو استخدم `{core_decimal}` للقيم الدقيقة.",
            "Qaren b hamesh sama7 f shart el far3, zay `{kw_case} x {kw_if} x.near(0.3, tol: 0.0001):`, aw esta5dem `{core_decimal}` lel qeyam el daqee2a.",
        ],
        explain: [
            "Why: two `Float` values that print the same can differ in their last bit, so an exact pattern for a `Float` would match by luck. `Decimal` is exact and can be matched; a `Float` is compared with `.near(other, tol:)`.\n\nInstead of:\n    match ratio:\n        case 0.5: \"half\"\n        case _: \"other\"\n\nwrite:\n    match ratio:\n        case r if r.near(0.5, tol: 0.0001): \"half\"\n        case _: \"other\"",
            "السبب: قيمتان من النوع `كسري` تُطبعان بالشكل نفسه قد تختلفان في آخر بت، فكان النمط الدقيق لقيمة كسرية سيطابق بالصدفة. أما `عشري` فدقيق ويمكن المطابقة عليه، والقيمة الكسرية تُقارن بهامش سماح.\n\nبدلًا من:\n    حسب النسبة:\n        عندما 0.5: \"نصف\"\n        عندما _: \"غير ذلك\"\n\nاكتب:\n    حسب النسبة:\n        عندما ن إذا ن.near(0.5، tol: 0.0001): \"نصف\"\n        عندما _: \"غير ذلك\"",
            "Leh: qeemtein `kasri` byetteb3o b nafs el shakl momken yekhtelfo f akher bit, fa pattern da2ee2 l qeema kasreya kan hayetabe2 bel sodfa. Laken `ashri` da2ee2 w momken te3mel 3aleih match; wel `kasri` byet2aren b hamesh sama7.\n\nBadal:\n    hasab nesba:\n        lama 0.5: \"nos\"\n        lama _: \"tany\"\n\nEkteb:\n    hasab nesba:\n        lama r law r.near(0.5, tol: 0.0001): \"nos\"\n        lama _: \"tany\"",
        ],
        proposed: false,
    },
    // ---- Functions (K11xx, spec §12; kalam-typeck). K1105 and K1106 are provisional codes
    // (spec erratum proposal). ar/franco: pending native review.
    Entry {
        code: "K1101",
        title: ["MISSING RETURN", "إرجاع ناقص", "RETURN NA2ES"],
        message: [
            "This function promises to return a value, but at least one path through its body reaches the end without `{kw_return}`.",
            "هذه الدالة تَعِد بإرجاع قيمة، لكن مسارًا واحدًا على الأقل في جسمها يصل إلى النهاية دون `{kw_return}`.",
            "El dala di bet-wa3ed enaha traga3 qeema, bas fe masar wa7ed 3ala el a2al f gesmaha byewsal lel akher men gher `{kw_return}`.",
        ],
        help: [
            "End every path with `{kw_return}`, for example add the missing `{kw_else}` branch, or a `{kw_return}` after the last block.",
            "أنهِ كل مسار بـ `{kw_return}`، مثلًا أضف فرع `{kw_else}` الناقص، أو `{kw_return}` بعد آخر كتلة.",
            "Khalles kol masar b `{kw_return}`, masalan zawed far3 `{kw_else}` el na2es, aw `{kw_return}` ba3d akher block.",
        ],
        explain: [
            "Why: a function body made of statements has no value of its own at the end. If some path falls off the end, the caller would receive nothing where it was promised a value. Kalam never returns a default value silently.\n\nInstead of:\n    fn sign(x: Int) -> String:\n        if x < 0:\n            return \"negative\"\n        else if x > 0:\n            return \"positive\"\n\nwrite:\n    fn sign(x: Int) -> String:\n        if x < 0:\n            return \"negative\"\n        else if x > 0:\n            return \"positive\"\n        return \"zero\"",
            "السبب: جسم الدالة المكوّن من جمل ليست له قيمة خاصة به عند نهايته. فلو وصل مسار إلى النهاية لاستلم المستدعي لا شيء حيث وُعد بقيمة. ولا يرجع كلام قيمة افتراضية بصمت أبدًا.\n\nبدلًا من:\n    دالة الإشارة(س: عدد) -> نص:\n        إذا س < 0:\n            أرجع \"سالب\"\n        وإلا إذا س > 0:\n            أرجع \"موجب\"\n\nاكتب:\n    دالة الإشارة(س: عدد) -> نص:\n        إذا س < 0:\n            أرجع \"سالب\"\n        وإلا إذا س > 0:\n            أرجع \"موجب\"\n        أرجع \"صفر\"",
            "Leh: gesm el dala elly kollo statements malosh qeema lewa7do f el akher. Law fe masar wesel lel akher, elly nada el dala mesh hayestalem 7aga f makan et-wa3ad feeh b qeema. W Kalam 3omro ma beyraga3 qeema default f el sekket.\n\nBadal:\n    dala eshara(x: adad) -> nass:\n        law x < 0:\n            rag3 \"saleb\"\n        walla law x > 0:\n            rag3 \"mogab\"\n\nEkteb:\n    dala eshara(x: adad) -> nass:\n        law x < 0:\n            rag3 \"saleb\"\n        walla law x > 0:\n            rag3 \"mogab\"\n        rag3 \"sefr\"",
        ],
        proposed: false,
    },
    Entry {
        code: "K1103",
        title: ["AMBIGUOUS METHOD", "دالة ملتبسة", "METHOD MESH WADE7A"],
        message: [
            "More than one method named `{name}` applies here: the type itself, or several of its traits, define one. I won't pick one for you.",
            "تنطبق هنا أكثر من دالة اسمها `{name}`: يعرّفها النوع نفسه أو أكثر من سمة من سماته. ولن أختار إحداها نيابة عنك.",
            "Fe aktar men method esmaha `{name}` tenfa3 hena: el type nafso aw kaza trait men el traits bta3to bey3arrafoha. W ana mesh ha5tar wa7da badalak.",
        ],
        help: [
            "Name the trait you mean: write `Trait.{name}(x)` instead of `x.{name}()`.",
            "سمِّ السمة التي تقصدها: اكتب `السمة.{name}(x)` بدل `x.{name}()`.",
            "Sammy el trait elly 2asdak 3aleiha: ekteb `Trait.{name}(x)` badal `x.{name}()`.",
        ],
        explain: [
            "Why: method lookup in Kalam does not depend on imports, so the set of candidates is the same everywhere in the program. When two traits define the same name for one type, any choice I made could change meaning after a dependency upgrade. Naming the trait makes the call exact (spec §12.2).\n\nInstead of:\n    let text = price.show()\n\nwrite:\n    let text = Show.show(price)",
            "السبب: البحث عن الدوال في كلام لا يعتمد على الاستيرادات، فمجموعة المرشحين واحدة في كل البرنامج. وحين تعرّف سمتان الاسم نفسه لنوع واحد، فإن أي اختيار مني قد يغيّر المعنى بعد تحديث مكتبة. وتسمية السمة تجعل الاستدعاء محددًا تمامًا (المواصفة §12.2).\n\nبدلًا من:\n    ليكن النص = السعر.show()\n\nاكتب:\n    ليكن النص = Show.show(السعر)",
            "Leh: el dawar 3ala el methods f Kalam mesh mo3tamed 3ala el imports, fa magmoo3et el morasha7een wa7da f kol el program. W lamma traitein y3arrafo nafs el esm l type wa7ed, ay ekhtiyar menni momken yghayar el ma3na ba3d update l library. W lamma tesammy el trait el call yeb2a m7added bel zabt (el spec §12.2).\n\nBadal:\n    khali text = price.show()\n\nEkteb:\n    khali text = Show.show(price)",
        ],
        proposed: false,
    },
    Entry {
        code: "K1105",
        title: ["ARGUMENTS DON'T MATCH", "المعاملات غير متطابقة", "EL ARGUMENTS MESH MAZBOOTA"],
        message: [
            "The arguments of this call don't match the function's parameters: {detail}.",
            "معاملات هذا الاستدعاء لا تطابق معاملات الدالة: {detail}.",
            "El arguments bta3et el call da mesh mtab2a ma3 parameters el dala: {detail}.",
        ],
        help: [
            "Pass one argument for each parameter, in order or by name, and use the parameter names as labels where the function requires them.",
            "مرّر معاملًا واحدًا لكل معامل في التعريف، بالترتيب أو بالاسم، واستخدم أسماء المعاملات كعناوين حيث تشترطها الدالة.",
            "Ba3at argument wa7ed l kol parameter, bel tarteeb aw bel esm, w esta5dem asamy el parameters ka labels f el amaken elly el dala 3ayzaha feeha.",
        ],
        explain: [
            "Why: Kalam has no overloading, no variadic functions and no hidden default arguments, so a call must match the declaration exactly: one argument per parameter, each label a real parameter name, and no parameter given twice.\n\nInstead of:\n    fn area(width: Float, height: Float) -> Float:\n        return width * height\n    let a = area(2.0)\n\nwrite:\n    let a = area(width: 2.0, height: 3.0)",
            "السبب: لا يوجد في كلام تحميل زائد للدوال، ولا دوال بعدد معاملات متغير، ولا قيم افتراضية خفية، فيجب أن يطابق الاستدعاء التعريف تمامًا: معامل واحد لكل معامل، وكل عنوان اسم معامل حقيقي، ولا يُعطى معامل مرتين.\n\nبدلًا من:\n    دالة المساحة(العرض: كسري، الطول: كسري) -> كسري:\n        أرجع العرض * الطول\n    ليكن م = المساحة(2.0)\n\nاكتب:\n    ليكن م = المساحة(العرض: 2.0، الطول: 3.0)",
            "Leh: Kalam mafeehoosh overloading, wala dawal b 3adad arguments motghayer, wala qeyam default mestakhabeya, fa el call lazem yetabe2 el ta3reef bel zabt: argument wa7ed l kol parameter, w kol label esm parameter 7a2ee2y, w mafeesh parameter yetedy marratein.\n\nBadal:\n    dala mesa7a(3ard: kasri, tool: kasri) -> kasri:\n        rag3 3ard * tool\n    khali m = mesa7a(2.0)\n\nEkteb:\n    khali m = mesa7a(3ard: 2.0, tool: 3.0)",
        ],
        proposed: true,
    },
    Entry {
        code: "K1106",
        title: ["IMPLEMENTATION IN THE WRONG PLACE", "تنفيذ في غير مكانه", "IMPL F MAKAN GHALAT"],
        message: [
            "I can't accept this `{kw_impl}`: either another implementation of the same trait for the same type already exists, or it lives outside both the trait's module and the type's module (the orphan rule).",
            "لا أستطيع قبول هذا الـ `{kw_impl}`: فإما أن تنفيذًا آخر للسمة نفسها للنوع نفسه موجود بالفعل، وإما أنه خارج وحدة السمة ووحدة النوع كلتيهما (قاعدة اليتيم).",
            "Mesh ha2dar a2bal el `{kw_impl}` da: ya ema fe implementation tany l nafs el trait l nafs el type mawgood aslan, ya ema howa bara module el trait w module el type el etnein (orphan rule).",
        ],
        help: [
            "Keep one implementation per trait and type, in the module that defines the trait or the one that defines the type. To add behavior to another package's type, wrap it in a type of your own.",
            "أبقِ تنفيذًا واحدًا لكل سمة ونوع، في الوحدة التي تعرّف السمة أو التي تعرّف النوع. ولإضافة سلوك إلى نوع من حزمة أخرى، غلّفه في نوع خاص بك.",
            "Khalli implementation wa7ed l kol trait w type, f el module elly bet3arraf el trait aw elly bet3arraf el type. W 3ashan tzawed behavior l type men package tanya, laffo f type bta3ak.",
        ],
        explain: [
            "Why: with at most one implementation in the whole program, `x.show()` means the same thing everywhere, whatever is imported. The orphan rule keeps it that way when packages are combined: two libraries can never both implement a third library's trait for a fourth library's type (spec §9.7).\n\nInstead of (in your package):\n    impl Url as Show:\n        ...\n\nwrite:\n    type Link:\n        url: Url\n    impl Link as Show:\n        ...",
            "السبب: مع تنفيذ واحد على الأكثر في البرنامج كله، يعني `x.show()` الشيء نفسه في كل مكان مهما كانت الاستيرادات. وقاعدة اليتيم تحفظ ذلك عند جمع الحزم، فلا يمكن لمكتبتين أن تنفّذا كلتاهما سمة مكتبة ثالثة لنوع مكتبة رابعة (المواصفة §9.7).\n\nبدلًا من (في حزمتك):\n    نفذ Url بصفة Show:\n        ...\n\nاكتب:\n    هيكل رابط:\n        العنوان: Url\n    نفذ رابط بصفة Show:\n        ...",
            "Leh: ma3 implementation wa7ed bel aktar f el program kollo, `x.show()` ma3naha nafs el 7aga f kol makan mahma kanet el imports. W el orphan rule bete7fez da lamma el packages tetgeme3: mafeesh libraryein y2daro y3melo implement l trait men library talta l type men library rab3a (el spec §9.7).\n\nBadal (f el package bta3ak):\n    naffez Url ka Show:\n        ...\n\nEkteb:\n    haykal Link:\n        url: Url\n    naffez Link ka Show:\n        ...",
        ],
        proposed: true,
    },
    // K1107: spec §27 erratum 2026-10-25 (FOUNDER-QUEUE Q27). `{detail}` is the type checker's
    // English description of the first difference. ar/franco: pending native review.
    Entry {
        code: "K1107",
        title: ["IMPLEMENTATION DOESN'T MATCH ITS TRAIT", "التنفيذ لا يطابق سمته", "EL IMPLEMENTATION MESH MTAB2 EL TRAIT"],
        message: [
            "This `{kw_impl}` doesn't match the trait `{trait}` it implements: {detail}. Code that relies on the trait calls the trait's version of each method, so the two have to agree exactly.",
            "هذا الـ `{kw_impl}` لا يطابق السمة `{trait}` التي ينفّذها: {detail}. الكود الذي يعتمد على السمة يستدعي نسختها من كل دالة، فلا بد أن يتطابق الاثنان تمامًا.",
            "El `{kw_impl}` da mesh mtab2 el trait `{trait}` elly bey3mlo implement: {detail}. El code elly mesh3'al 3ala el trait beyendah nos5et el trait men kol method, fa lazem el etnein yetab2o bel zabt.",
        ],
        help: [
            "Write each method of the trait once, with the trait's name for it, `self` exactly where the trait has it, and the trait's parameter and return types (with its type parameters and `Self` filled in). Put any other method in a plain `{kw_impl}` block.",
            "اكتب كل دالة من دوال السمة مرة واحدة، باسمها في السمة، ومع `self` حيث تضعه السمة فقط، وبأنواع المعاملات والإرجاع التي في السمة (بعد تعويض معاملات النوع و`Self`). وضع أي دالة أخرى في كتلة `{kw_impl}` عادية.",
            "Ekteb kol method men el trait marra wa7da, b esmaha f el trait, w b `self` bas f el makan elly el trait 7atetha feeh, w b anwa3 el parameters wel return elly f el trait (ba3d ma t7ot el type parameters w `Self`). W 7ot ay method tanya f block `{kw_impl}` 3ady.",
        ],
        explain: [
            "Why: generic code reaches a method through its trait. A function with a `T: Named` bound calls `x.name()` as the trait declares it, and `?` converts an error with `F.from(e)` as `From[E]` declares it. If an implementation returned another type, took other parameters or left a method out, a value of the wrong type would cross that call unchecked. So an implementation has to match its trait exactly (spec §9.7).\n\nInstead of:\n    trait Named:\n        fn name(self) -> String\n\n    impl Box as Named:\n        fn name(self) -> Int => self.size\n\nwrite:\n    impl Box as Named:\n        fn name(self) -> String => \"box {self.size}\"",
            "السبب: يصل الكود العام إلى الدالة عبر سمتها. فالدالة التي تشترط `T: Named` تستدعي `x.name()` كما تعلنها السمة، و`؟` يحوّل الخطأ بـ `F.from(e)` كما تعلنها `From[E]`. ولو أرجع التنفيذ نوعًا آخر، أو أخذ معاملات أخرى، أو أغفل دالة، لعبرت قيمة من النوع الخطأ ذلك الاستدعاء دون فحص. لذلك يجب أن يطابق التنفيذ سمته تمامًا (المواصفة §9.7).\n\nبدلًا من:\n    سمة Named:\n        دالة name(ذات) -> نص\n\n    نفذ Box بصفة Named:\n        دالة name(ذات) -> عدد => ذات.size\n\nاكتب:\n    نفذ Box بصفة Named:\n        دالة name(ذات) -> نص => \"box {ذات.size}\"",
            "Leh: el code el generic beyewsal lel method 3an tare2 el trait bta3etha. El dala elly 3andaha bound `T: Named` betendah `x.name()` zay ma el trait m3lenaha, w `?` bey7awel el error b `F.from(e)` zay ma `From[E]` m3lenaha. Law el implementation ragga3 type tany, aw a5ad parameters tanya, aw nesy method, qeema men el type el ghalat hate3dy el call da men gher fa7s. 3ashan keda lazem el implementation yetabe2 el trait bta3o bel zabt (el spec §9.7).\n\nBadal:\n    sefa Named:\n        dala name(nafso) -> nass\n\n    naffez Box ka Named:\n        dala name(nafso) -> adad => nafso.size\n\nEkteb:\n    naffez Box ka Named:\n        dala name(nafso) -> nass => \"box {nafso.size}\"",
        ],
        proposed: false,
    },
    // K1108: proposed (M2 iteration 4, kalam-typeck; FOUNDER-QUEUE Q35). The machine fix
    // wraps the field access in parentheses. ar/franco: pending native review.
    Entry {
        code: "K1108",
        title: ["FIELD CALLED LIKE A METHOD", "حقل يُستدعى كأنه دالة نوع", "FIELD BYET3AMALLO CALL KA2ENNO METHOD"],
        message: [
            "`{name}` is a field of `{type}` that holds a function, not a method of `{type}`. `value.{name}(…)` calls a method, so to call the function stored in the field, put the field access in parentheses first.",
            "`{name}` حقل في `{type}` يحمل دالة، وليس دالة من دوال `{type}`. فالصيغة `القيمة.{name}(…)` تستدعي دالة النوع، ولاستدعاء الدالة المخزنة في الحقل ضع الوصول إلى الحقل بين قوسين أولًا.",
            "`{name}` field f `{type}` shayel dala, mesh method men methods `{type}`. `qeema.{name}(…)` betendah method, fa 3ashan tendah el dala elly mt5azzena f el field, 7ot el field bein a2was el awel.",
        ],
        help: [
            "Write `(value.{name})(…)`; `kalam fix` does this for you.",
            "اكتب `(القيمة.{name})(…)`؛ ويفعل `kalam fix` ذلك نيابة عنك.",
            "Ekteb `(qeema.{name})(…)`; w `kalam fix` bey3mel keda badalak.",
        ],
        explain: [
            "Why: `x.m(…)` always calls a method of `x`'s type (spec §12.2), so a reader can tell a method call from a call of a function that is stored as data. A function in a field is a value like any other: read the field, then call the value.\n\nInstead of:\n    let n = job.run(1)\n\nwrite:\n    let n = (job.run)(1)",
            "السبب: `x.m(…)` تستدعي دائمًا دالة من دوال نوع `x` (المواصفة §12.2)، فيميّز القارئ استدعاء دالة النوع من استدعاء دالة مخزنة كبيانات. والدالة في الحقل قيمة كأي قيمة: اقرأ الحقل، ثم استدعِ القيمة.\n\nبدلًا من:\n    ليكن ن = المهمة.run(1)\n\nاكتب:\n    ليكن ن = (المهمة.run)(1)",
            "Leh: `x.m(…)` dayman betendah method men el type bta3 `x` (el spec §12.2), fa elly beye2ra ye2dar yefre2 ben call l method w call l dala mt5azzena ka data. El dala elly f field qeema zay ay qeema: e2ra el field, w ba3dein endah el qeema.\n\nBadal:\n    khali n = job.run(1)\n\nEkteb:\n    khali n = (job.run)(1)",
        ],
        proposed: true,
    },
    // ---- Modules and globals (K12xx, spec §13; the parser). ar/franco: pending native review.
    Entry {
        code: "K1202",
        title: ["MODULE-LEVEL VAR", "متغير على مستوى الوحدة", "MOTGHAYER 3ALA MOSTAWA EL MODULE"],
        message: [
            "`{kw_var}` can't be declared at module level. Kalam has no global mutable state: a module-level value is a constant.",
            "لا يمكن تعريف `{kw_var}` على مستوى الوحدة. ليس في كلام حالة عامة قابلة للتغيير: القيمة على مستوى الوحدة ثابتة.",
            "Mayenfa3sh tet3arraf `{kw_var}` 3ala mostawa el module. Kalam mafeehoosh state 3am byetghayar: el qeema 3ala mostawa el module sabta.",
        ],
        help: [
            "Use `{kw_let}` for a constant. For state that changes, create a `Cell` in `{core_main}` and pass it to the functions that need it.",
            "استخدم `{kw_let}` للثابت. وللحالة التي تتغير، أنشئ `Cell` في `{core_main}` ومرّرها إلى الدوال التي تحتاجها.",
            "Esta5dem `{kw_let}` lel sabet. W lel state elly btetghayar, e3mel `Cell` f `{core_main}` w ba3atha lel dawal elly me7tagaha.",
        ],
        explain: [
            "Why: a global variable can be changed by any function, so you can never tell from a call what it touches, and two tasks could change it at once. Passing a `Cell` down makes shared state visible in every signature that uses it (spec §15.6).\n\nInstead of:\n    var hits = 0\n\nwrite:\n    fn main():\n        let hits = Cell.new(0)\n        serve(hits)",
            "السبب: المتغير العام يمكن لأي دالة أن تغيّره، فلا تعرف أبدًا من الاستدعاء ما الذي يمسّه، وقد تغيّره مهمتان في الوقت نفسه. وتمرير `Cell` يجعل الحالة المشتركة ظاهرة في توقيع كل دالة تستخدمها (المواصفة §15.6).\n\nبدلًا من:\n    متغير الزيارات = 0\n\nاكتب:\n    دالة رئيسي():\n        ليكن الزيارات = Cell.new(0)\n        اخدم(الزيارات)",
            "Leh: el motghayer el 3am ay dala te2dar tghayaro, fa 3omrak ma hate3raf men el call howa byelmes eh, w momken etnein tasks yghayaroh f nafs el wa2t. W lamma teba3at `Cell` lta7t, el state el moshtarak byeb2a bayen f signature kol dala betesta5demo (el spec §15.6).\n\nBadal:\n    motghayer ziyarat = 0\n\nEkteb:\n    dala ra2isi():\n        khali ziyarat = Cell.new(0)\n        ekhdem(ziyarat)",
        ],
        proposed: false,
    },
    Entry {
        code: "K1203",
        title: ["GLOB IMPORT", "استيراد بالجملة", "IMPORT BEL GOMLA"],
        message: [
            "I don't accept glob imports (`*`). A glob hides where each name comes from, and a new export upstream could silently change what a name in this file means.",
            "لا أقبل الاستيراد بالجملة (`*`). فهو يخفي مصدر كل اسم، وقد يغيّر تصديرٌ جديد في الوحدة المستورَدة معنى اسم في هذا الملف دون أن تلاحظ.",
            "Mesh ba2bal el import bel gomla (`*`). Howa bykhabby kol esm gay menein, w export gedeed f el module el tanya momken yghayar ma3na esm f el file da men gher ma takhod balak.",
        ],
        help: [
            "List the names you use, for example `{kw_use} std/http.{Request, Response}`.",
            "اذكر الأسماء التي تستخدمها، مثل `{kw_use} std/http.{Request، Response}`.",
            "Ekteb el asamy elly betesta5demha, zay `{kw_use} std/http.{Request, Response}`.",
        ],
        explain: [
            "Why: with explicit imports, every name in a file can be traced to its module by reading the file alone, by you, a reviewer or a tool. Globs also invite guessed names: a name that seems to come from a glob may not exist at all.\n\nInstead of:\n    use std/http.*\n\nwrite:\n    use std/http.{Request, Response}",
            "السبب: مع الاستيراد الصريح يمكن تتبع كل اسم في الملف إلى وحدته بقراءة الملف وحده، سواء قرأته أنت أو مراجع أو أداة. والاستيراد بالجملة يشجع على تخمين الأسماء أيضًا: فاسم يبدو أنه آتٍ منه قد لا يكون موجودًا أصلًا.\n\nبدلًا من:\n    استخدم std/http.*\n\nاكتب:\n    استخدم std/http.{Request، Response}",
            "Leh: ma3 el import el sare7, kol esm f el file momken tet-tabba3o lel module bta3o men 2erayet el file lewa7do, sawa2 enta aw reviewer aw tool. W el import bel gomla bysha33a3 3ala takhmeen el asamy: esm shaklo gay menno momken mayeb2ash mawgood aslan.\n\nBadal:\n    hat std/http.*\n\nEkteb:\n    hat std/http.{Request, Response}",
        ],
        proposed: false,
    },
    // K1204: spec §27 erratum 2026-10-25 (FOUNDER-QUEUE Q33; kalam-typeck). The reason is a
    // label at the part that breaks a rule. ar/franco: pending native review.
    Entry {
        code: "K1204",
        title: ["NOT A CONSTANT", "ليس ثابتًا", "MESH SABET"],
        message: [
            "This has to be a constant, and the marked part isn't. A module-level `{kw_let}` and a field's default may use only literals, constructors, other constants and operators, and must not panic; a module constant also can't hold a capability or a `Cell`.",
            "يجب أن يكون هذا ثابتًا، والجزء المعلَّم ليس كذلك. فـ `{kw_let}` على مستوى الوحدة والقيمة الافتراضية لحقل لا يستخدمان إلا القيم الحرفية والمُنشئات والثوابت الأخرى والعوامل، ولا يجوز أن ينهارا؛ ولا يحمل ثابت الوحدة صلاحية أو `Cell`.",
            "Lazem da yeb2a sabet, wel goz2 el mt3allem mesh keda. `{kw_let}` 3ala mostawa el module wel default bta3 field mayesta5demosh gheir el literals wel constructors wel sawabet el tanya wel operators, w mayenfa3sh yo2a3o; w sabet el module kaman mayshelsh capability aw `Cell`.",
        ],
        help: [
            "Compute the value inside `{core_main}` (or a function) and pass it down, or write it as a literal. For shared state, create the `Cell` in `{core_main}`.",
            "احسب القيمة داخل `{core_main}` (أو داخل دالة) ومرّرها، أو اكتبها قيمة حرفية. وللحالة المشتركة أنشئ `Cell` داخل `{core_main}`.",
            "E7seb el qeema gowa `{core_main}` (aw gowa dala) w ba3atha, aw ekteb-ha literal. W lel state el moshtarak e3mel el `Cell` gowa `{core_main}`.",
        ],
        explain: [
            "Why: module constants are computed once, before `main` runs, in dependency order (spec §13.4). If one could call a function, wait for I/O, fail, or hold a capability or a `Cell`, merely importing a module could run code, stop the program or hand out a power that never came from `main`. Keeping them constant means a module has no start-up effects, and a value that would panic is reported here, at compile time.\n\nInstead of:\n    let LIMIT = config_limit()\n    let COUNTER = Cell.new(0)\n\nwrite:\n    let LIMIT = 100\n\n    fn main():\n        let counter = Cell.new(0)",
            "السبب: تُحسب ثوابت الوحدة مرة واحدة قبل أن يبدأ `رئيسي`، بترتيب اعتمادها (المواصفة §13.4). ولو أمكن لثابت أن يستدعي دالة، أو ينتظر إدخالًا أو إخراجًا، أو يفشل، أو يحمل صلاحية أو `Cell`، لكان مجرد استيراد وحدة كافيًا لتشغيل كود أو إيقاف البرنامج أو منح صلاحية لم تأتِ من `رئيسي`. وبقاؤها ثابتة يعني أن الوحدة لا أثر لها عند البدء، وأن القيمة التي كانت ستنهار تُكتشف هنا، عند الترجمة.\n\nبدلًا من:\n    ليكن الحد = حد_الإعداد()\n    ليكن العداد = Cell.new(0)\n\nاكتب:\n    ليكن الحد = 100\n\n    دالة رئيسي():\n        ليكن العداد = Cell.new(0)",
            "Leh: sawabet el module bet-t7seb marra wa7da, 2abl ma `ra2isi` yebda2, b tarteeb e3temadha (el spec §13.4). Law sabet ye2dar yendah dala, aw yestanna I/O, aw yefshal, aw yeshil capability aw `Cell`, mogarrad import l module momken yeshaghal code, aw yowa22af el program, aw yeddy salahiya 3omraha ma gat men `ra2isi`. W enaha tefdal sabta ma3naha en el module malhash ay asar 3and el bedaya, w en el qeema elly kanet hato2a3 bet-tekteshef hena, wa2t el compile.\n\nBadal:\n    khali LIMIT = config_limit()\n    khali COUNTER = Cell.new(0)\n\nEkteb:\n    khali LIMIT = 100\n\n    dala ra2isi():\n        khali counter = Cell.new(0)",
        ],
        proposed: false,
    },
    // ---- Errors (K13xx, spec §14; kalam-typeck). ar/franco: pending native review.
    Entry {
        code: "K1303",
        title: ["UNUSED RESULT", "نتيجة غير مستخدمة", "NATIGA MESH MOSTA5DAMA"],
        message: [
            "This call returns a `{core_result}`, and nothing checks it. An ignored error is a silent failure: the program carries on as if the call had worked.",
            "هذا الاستدعاء يرجع `{core_result}`، ولا شيء يفحصها. والخطأ المتجاهَل فشل صامت: يكمل البرنامج كأن الاستدعاء نجح.",
            "El call da beyraga3 `{core_result}`, w mafeesh 7aga bet-check-ha. W el error elly etgahel fashal saket: el program bykammel ka2en el call nege7.",
        ],
        help: [
            "Handle it with `{kw_match}`, pass the error on with `{punct_try}`, or write `{kw_let} _ = ...` to drop it on purpose (`kalam audit` counts those).",
            "عالجها بـ `{kw_match}`، أو مرّر الخطأ إلى أعلى بـ `{punct_try}`، أو اكتب `{kw_let} _ = ...` لتتركها عن قصد (والأمر `kalam audit` يعدّ هذه الحالات).",
            "3algha b `{kw_match}`, aw ba3at el error lfo2 b `{punct_try}`, aw ekteb `{kw_let} _ = ...` 3ashan tesebha 3an 2asd (w `kalam audit` bey3ed el 7alat di).",
        ],
        explain: [
            "Why: errors in Kalam are values, not exceptions, so a dropped `Result` is a dropped error. That is why it is an error rather than a warning (spec §14.2).\n\nInstead of:\n    db.exec(sql\"DELETE FROM sessions\")\n\nwrite:\n    db.exec(sql\"DELETE FROM sessions\")?",
            "السبب: الأخطاء في كلام قيم وليست استثناءات، فالنتيجة `نتيجة` المتروكة خطأ متروك. لهذا كان هذا خطأ لا تحذيرًا (المواصفة §14.2).\n\nبدلًا من:\n    db.exec(sql\"DELETE FROM sessions\")\n\nاكتب:\n    db.exec(sql\"DELETE FROM sessions\")؟",
            "Leh: el errors f Kalam qeyam mesh exceptions, fa el `natiga` elly etsabet error etsab. 3ashan keda di error mesh warning (el spec §14.2).\n\nBadal:\n    db.exec(sql\"DELETE FROM sessions\")\n\nEkteb:\n    db.exec(sql\"DELETE FROM sessions\")?",
        ],
        proposed: false,
    },
    // ---- Sinks (K16xx, spec §17; kalam-typeck for K1601). ar/franco: pending native review.
    Entry {
        code: "K1601",
        title: ["STRING PASSED TO A SINK", "نص مُمرَّر إلى مصبّ", "STRING RAYEH L SINK"],
        message: [
            "This place expects `{expected}`, but this is `{found}`. A plain string here is how injection happens, so a sink never takes one.",
            "هذا المكان ينتظر `{expected}`، لكن هذه القيمة `{found}`. والنص العادي هنا هو باب الحقن، لذلك لا يقبل المصبّ نصًا أبدًا.",
            "El makan da mestanny `{expected}`, bas di `{found}`. W el string el 3adi hena howa bab el injection, 3ashan keda el sink 3omro ma bey2bal string.",
        ],
        help: [
            "Write the text as a tagged literal (`sql\"...\"`, `html\"...\"` or `cmd\"...\"`) and put the values in holes: each hole is escaped for its context.",
            "اكتب النص كقيمة حرفية موسومة (`sql\"...\"` أو `html\"...\"` أو `cmd\"...\"`) وضع القيم في فجوات: كل فجوة تُهرَّب حسب سياقها.",
            "Ekteb el kalam ka tagged literal (`sql\"...\"` aw `html\"...\"` aw `cmd\"...\"`) w 7ott el qeyam f holes: kol hole byet3amallaha escape 7asab makanha.",
        ],
        explain: [
            "Why: SQL, HTML and shell injection all start with a string built by concatenation. Sinks such as `db.all` take `Sql`, `Html` or `Command`, never `String`, and the only way to build those is a tagged literal whose holes are escaped by the compiler (spec §17).\n\nInstead of:\n    db.all(\"SELECT * FROM users WHERE name = '\" + name + \"'\")\n\nwrite:\n    db.all(sql\"SELECT * FROM users WHERE name = { name }\")",
            "السبب: حقن SQL وHTML والطرفية كلها تبدأ بنص مبني بالوصل. والمصبّات مثل `db.all` تأخذ `Sql` أو `Html` أو `Command` ولا تأخذ `نص` أبدًا، والطريقة الوحيدة لبناء هذه الأنواع قيمة حرفية موسومة يهرّب المترجم فجواتها (المواصفة §17).\n\nبدلًا من:\n    db.all(\"SELECT * FROM users WHERE name = '\" + الاسم + \"'\")\n\nاكتب:\n    db.all(sql\"SELECT * FROM users WHERE name = { الاسم }\")",
            "Leh: el SQL injection wel HTML injection wel shell injection kollohom byebda2o b string mabny bel wasl. W el sinks zay `db.all` betakhod `Sql` aw `Html` aw `Command`, w 3omraha ma betakhod `nass`, w el taree2a el wa7eeda tebnihom tagged literal el compiler bey3mel escape l holes-ha (el spec §17).\n\nBadal:\n    db.all(\"SELECT * FROM users WHERE name = '\" + esm + \"'\")\n\nEkteb:\n    db.all(sql\"SELECT * FROM users WHERE name = { esm }\")",
        ],
        proposed: false,
    },
    // ---- Patterns and functions not yet emitted (K10xx, K11xx, spec §11, §12). ar/franco:
    // pending native review.
    Entry {
        code: "K1002",
        title: ["UNREACHABLE ARM", "فرع لا يُوصل إليه", "FAR3 MESH HAYETWESEL LEEH"],
        message: [
            "This `{kw_case}` can never run: the arms above it already match every value it would match.",
            "هذا الـ `{kw_case}` لن يعمل أبدًا: الفروع التي فوقه تطابق بالفعل كل قيمة كان سيطابقها.",
            "El `{kw_case}` da 3omro ma hayeshtaghal: el foroo3 elly fo2o betetabe2 aslan kol qeema kan hayetabe2ha.",
        ],
        help: [
            "Move this arm above the arm that covers it (often a catch-all `_` placed too early), or delete it.",
            "انقل هذا الفرع فوق الفرع الذي يغطيه (وغالبًا يكون فرعًا عامًا `_` وُضع مبكرًا)، أو احذفه.",
            "Na2al el far3 da fo2 el far3 elly byghatih (ghaleban far3 3am `_` et7att badry), aw emsa7o.",
        ],
        explain: [
            "Why: arms are tried from top to bottom. An arm that can never be reached is almost always a catch-all placed first, so the specific case you wrote is silently ignored. That is why this is an error, not a warning (spec §11).\n\nInstead of:\n    match role:\n        case _: \"user\"\n        case .Admin: \"admin\"\n\nwrite:\n    match role:\n        case .Admin: \"admin\"\n        case _: \"user\"",
            "السبب: تُجرَّب الفروع من أعلى إلى أسفل. والفرع الذي لا يمكن الوصول إليه يكون في الغالب فرعًا عامًا وُضع أولًا، فتُهمل الحالة المحددة التي كتبتها بصمت. لهذا كان هذا خطأ لا تحذيرًا (المواصفة §11).\n\nبدلًا من:\n    حسب الدور:\n        عندما _: \"مستخدم\"\n        عندما .مدير: \"مدير\"\n\nاكتب:\n    حسب الدور:\n        عندما .مدير: \"مدير\"\n        عندما _: \"مستخدم\"",
            "Leh: el foroo3 bettegarrab men fo2 l ta7t. W el far3 elly mayetweselsh leeh ghaleban far3 3am et7att el awel, fa el 7ala el m7addada elly katabtaha btetgahel f el sekket. 3ashan keda di error mesh warning (el spec §11).\n\nBadal:\n    hasab role:\n        lama _: \"user\"\n        lama .Admin: \"admin\"\n\nEkteb:\n    hasab role:\n        lama .Admin: \"admin\"\n        lama _: \"user\"",
        ],
        proposed: false,
    },
    Entry {
        code: "K1004",
        title: ["OR-PATTERN BINDS DIFFERENT NAMES", "بدائل النمط تربط أسماء مختلفة", "EL BADAYEL BTERBOT ASAMY MOKHTALFA"],
        message: [
            "The alternatives of this `|` pattern don't bind the same names with the same types, so after the match some names would have no value.",
            "بدائل النمط `|` هذا لا تربط الأسماء نفسها بالأنواع نفسها، فكانت بعض الأسماء ستبقى بلا قيمة بعد المطابقة.",
            "Badayel el pattern `|` da mesh berbot nafs el asamy b nafs el types, fa ba3d el asamy kanet hatefdal men gher qeema ba3d el match.",
        ],
        help: [
            "Bind the same names, with the same types, in every alternative, or split the alternatives into separate `{kw_case}` arms.",
            "اربط الأسماء نفسها بالأنواع نفسها في كل بديل، أو افصل البدائل في فروع `{kw_case}` مستقلة.",
            "Orbot nafs el asamy b nafs el types f kol badeel, aw efsel el badayel f foroo3 `{kw_case}` monfasla.",
        ],
        explain: [
            "Why: the body of an arm runs whichever alternative matched, so every name it uses must have a value in every alternative.\n\nInstead of:\n    case .Card(number) | .Wallet: charge(number)\n\nwrite:\n    case .Card(number): charge(number)\n    case .Wallet: charge_wallet()",
            "السبب: جسم الفرع يعمل أيًّا كان البديل الذي طابق، فيجب أن يكون لكل اسم يستخدمه قيمة في كل بديل.\n\nبدلًا من:\n    عندما .بطاقة(الرقم) | .محفظة: ادفع(الرقم)\n\nاكتب:\n    عندما .بطاقة(الرقم): ادفع(الرقم)\n    عندما .محفظة: ادفع_بالمحفظة()",
            "Leh: gesm el far3 byeshtaghal ayan kan el badeel elly etaba2, fa kol esm byesta5demo lazem yeb2a leeh qeema f kol badeel.\n\nBadal:\n    lama .Card(number) | .Wallet: edfa3(number)\n\nEkteb:\n    lama .Card(number): edfa3(number)\n    lama .Wallet: edfa3_bel_wallet()",
        ],
        proposed: false,
    },
    Entry {
        code: "K1104",
        title: ["LABELS NEEDED FOR SAME-TYPE PARAMETERS", "عناوين مطلوبة لمعاملات من النوع نفسه", "LABELS LAZMA L PARAMETERS NAFS EL TYPE"],
        message: [
            "This exported function has two neighbouring parameters of the same type, so a call by position could swap them without any error. Their arguments need labels.",
            "لهذه الدالة المصدَّرة معاملان متجاوران من النوع نفسه، فقد يبدّلهما استدعاء بالترتيب دون أي خطأ. لذلك تحتاج معاملاتهما إلى عناوين.",
            "El dala el exported di leeha parameterein gamb ba3d men nafs el type, fa call bel tarteeb momken ybaddelhom men gher ay error. 3ashan keda arguments-hom me7taga labels.",
        ],
        help: [
            "Label the arguments with the parameter names, for example `transfer(from: a, to: b)`. `kalam fix` inserts the labels.",
            "سمِّ المعاملات بأسماء معاملات الدالة، مثل `حوّل(من: أ، إلى: ب)`. والأمر `kalam fix` يضيف العناوين.",
            "Sammy el arguments b asamy el parameters, zay `7awel(men: a, le: b)`. W `kalam fix` byzawed el labels.",
        ],
        explain: [
            "Why: `transfer(a, b)` and `transfer(b, a)` both type-check when `from` and `to` are both `Account`, and one of them moves money the wrong way. With labels, the reader and the compiler both see which is which (spec §12.1).\n\nInstead of:\n    transfer(db, alice, bob, amount)\n\nwrite:\n    transfer(db, from: alice, to: bob, amount: amount)",
            "السبب: يجتاز `حوّل(أ، ب)` و`حوّل(ب، أ)` فحص الأنواع كلاهما حين يكون `من` و`إلى` من النوع `حساب`، وأحدهما ينقل المال في الاتجاه الخطأ. ومع العناوين يرى القارئ والمترجم أيهما أيهما (المواصفة §12.1).\n\nبدلًا من:\n    حوّل(القاعدة، علياء، باسم، المبلغ)\n\nاكتب:\n    حوّل(القاعدة، من: علياء، إلى: باسم، المبلغ: المبلغ)",
            "Leh: `7awel(a, b)` w `7awel(b, a)` el etnein bey3addo el type check lamma `men` w `le` el etnein `Account`, w wa7ed menhom bey7awel el flous f el etgah el ghalat. Ma3 el labels el 2ari wel compiler el etnein byshoofo meen meen (el spec §12.1).\n\nBadal:\n    7awel(db, alice, bob, mablagh)\n\nEkteb:\n    7awel(db, men: alice, le: bob, mablagh: mablagh)",
        ],
        proposed: false,
    },
    Entry {
        code: "K1110",
        title: ["CLOSURE CAPTURES A VAR", "دالة مجهولة تلتقط متغيرًا", "LAMBDA BTEMSEK MOTGHAYER"],
        message: [
            "This lambda uses a `{kw_var}` from the enclosing function. Closures capture values, never variables that may change later.",
            "هذه الدالة المجهولة تستخدم `{kw_var}` من الدالة المحيطة بها. والدوال المجهولة تلتقط قيمًا، ولا تلتقط أبدًا متغيرات قد تتغير لاحقًا.",
            "El lambda di betesta5dem `{kw_var}` men el dala elly 7awaleiha. El lambdas betemsek qeyam, w 3omraha ma betemsek motghayerat momken tetghayar ba3dein.",
        ],
        help: [
            "Copy the current value into a constant first, `{kw_let} snapshot = v`, and use `snapshot` inside the lambda. `kalam fix` inserts the copy.",
            "انسخ القيمة الحالية في ثابت أولًا، `{kw_let} لقطة = v`، واستخدم `لقطة` داخل الدالة المجهولة. والأمر `kalam fix` يضيف النسخة.",
            "Ensakh el qeema el 7alya f sabet el awel, `{kw_let} snapshot = v`, w esta5dem `snapshot` gowa el lambda. W `kalam fix` byzawed el noskha.",
        ],
        explain: [
            "Why: if a closure could see a `var`, it could observe the variable change after the closure was created, and two tasks running the closure could race. Capturing only `let` values keeps closures predictable and makes concurrency race-free (spec §12.3, §15.6).\n\nInstead of:\n    var total = 0\n    let report = fn() => print(\"{ total }\")\n\nwrite:\n    var total = 0\n    let snapshot = total\n    let report = fn() => print(\"{ snapshot }\")",
            "السبب: لو استطاعت الدالة المجهولة رؤية `متغير` لأمكنها ملاحظة تغيّره بعد إنشائها، ولتسابقت مهمتان تشغّلانها. والتقاط قيم `ليكن` فقط يجعل سلوك الدوال المجهولة متوقعًا ويمنع السباقات في التزامن (المواصفة §12.3، §15.6).\n\nبدلًا من:\n    متغير المجموع = 0\n    ليكن التقرير = دالة() => اطبع(\"{ المجموع }\")\n\nاكتب:\n    متغير المجموع = 0\n    ليكن لقطة = المجموع\n    ليكن التقرير = دالة() => اطبع(\"{ لقطة }\")",
            "Leh: law el lambda te2dar tshoof `motghayer`, kanet hatla7ez taghayyoro ba3d ma et3amalet, w etnein tasks beyshaghalo el lambda kano momken yetsaba2o. W lamma temsek qeyam `khali` bas, el lambdas btfdal motawa22a3a wel concurrency men gher races (el spec §12.3, §15.6).\n\nBadal:\n    motghayer total = 0\n    khali report = dala() => ektb(\"{ total }\")\n\nEkteb:\n    motghayer total = 0\n    khali snapshot = total\n    khali report = dala() => ektb(\"{ snapshot }\")",
        ],
        proposed: false,
    },
    // ---- Errors not yet emitted (K13xx, spec §14.3). ar/franco: pending native review.
    Entry {
        code: "K1301",
        title: ["? CAN'T CONVERT THE ERROR", "؟ لا تستطيع تحويل الخطأ", "? MESH HA2DAR Y7AWEL EL ERROR"],
        message: [
            "`{punct_try}` would pass this error up, but its error type is not the one this function returns, and I know no conversion between the two.",
            "كانت `{punct_try}` ستمرر هذا الخطأ إلى أعلى، لكن نوع خطئه ليس النوع الذي ترجعه هذه الدالة، ولا أعرف تحويلًا بين النوعين.",
            "`{punct_try}` kan hayba3at el error da lfo2, bas no3 el error bta3o mesh howa elly el dala di betraga3o, w ana mesh 3aref ay ta7weel ben el etnein.",
        ],
        help: [
            "Convert the error at the call, for example `.map_err({kw_fn}(e) => ...){punct_try}`, or add `{kw_impl} F {kw_as} From[E]` for your error type `F`.",
            "حوّل الخطأ عند الاستدعاء، مثل `.map_err({kw_fn}(e) => ...){punct_try}`، أو أضف `{kw_impl} F {kw_as} From[E]` لنوع الخطأ `F` الخاص بك.",
            "7awel el error 3and el call, zay `.map_err({kw_fn}(e) => ...){punct_try}`, aw zawed `{kw_impl} F {kw_as} From[E]` l no3 el error bta3ak `F`.",
        ],
        explain: [
            "Why: `?` is the only hidden control flow in Kalam, so it stays simple: it passes the error unchanged, converts it with a `From` implementation you wrote, or boxes it into `AnyError`. Anything else must be written out, so you can see how each error changes on its way up (spec §14.3).\n\nInstead of:\n    fn load(id: Int) -> Result[Note, AppError]:\n        return db.one(sql\"SELECT * FROM notes WHERE id = { id }\")?\n\nwrite:\n    fn load(id: Int) -> Result[Note, AppError]:\n        return db.one(sql\"SELECT * FROM notes WHERE id = { id }\").map_err(fn(e) => AppError.Db(e))?",
            "السبب: `؟` هي التحكم الخفي الوحيد في كلام، لذلك تبقى بسيطة: تمرر الخطأ دون تغيير، أو تحوّله بتنفيذ `From` كتبته أنت، أو تغلّفه في `AnyError`. وأي شيء آخر يجب أن يُكتب صراحة، لترى كيف يتغير كل خطأ في طريقه إلى أعلى (المواصفة §14.3).\n\nبدلًا من:\n    دالة حمّل(الرقم: عدد) -> نتيجة[ملاحظة، خطأ_التطبيق]:\n        أرجع db.one(sql\"SELECT * FROM notes WHERE id = { الرقم }\")؟\n\nاكتب:\n    دالة حمّل(الرقم: عدد) -> نتيجة[ملاحظة، خطأ_التطبيق]:\n        أرجع db.one(sql\"SELECT * FROM notes WHERE id = { الرقم }\").map_err(دالة(خ) => خطأ_التطبيق.قاعدة(خ))؟",
            "Leh: `?` howa el control flow el mestakhabby el wa7eed f Kalam, 3ashan keda byfdal baseet: bey3addy el error zay ma howa, aw y7awelo b `From` implementation enta katabto, aw y7otto f `AnyError`. W ay 7aga tanya lazem tetketeb sara7a, 3ashan tshoof kol error byetghayar ezay f taree2o lfo2 (el spec §14.3).\n\nBadal:\n    dala 7ammel(id: adad) -> natiga[Note, AppError]:\n        rag3 db.one(sql\"SELECT * FROM notes WHERE id = { id }\")?\n\nEkteb:\n    dala 7ammel(id: adad) -> natiga[Note, AppError]:\n        rag3 db.one(sql\"SELECT * FROM notes WHERE id = { id }\").map_err(dala(e) => AppError.Db(e))?",
        ],
        proposed: false,
    },
    Entry {
        code: "K1302",
        title: ["? ON OPTION IN A RESULT FUNCTION", "؟ على اختياري في دالة ترجع نتيجة", "? 3ALA OPTION F DALA BTRAGA3 RESULT"],
        message: [
            "This `{punct_try}` is applied to an `{core_option}`, but the function returns a `{core_result}`. A missing value has no error to pass up, so you need to say which error it is.",
            "هذه `{punct_try}` مطبّقة على `{core_option}`، لكن الدالة ترجع `{core_result}`. والقيمة الغائبة ليس لها خطأ يُمرَّر إلى أعلى، فعليك أن تحدد أي خطأ هو.",
            "El `{punct_try}` di 3ala `{core_option}`, bas el dala betraga3 `{core_result}`. W el qeema el msh mawgooda malhash error yetba3at lfo2, fa lazem t2ool howa anhy error.",
        ],
        help: [
            "Turn the missing value into an error first: `.ok_or(error){punct_try}`.",
            "حوّل القيمة الغائبة إلى خطأ أولًا: `.ok_or(error){punct_try}`.",
            "7awel el qeema el msh mawgooda l error el awel: `.ok_or(error){punct_try}`.",
        ],
        explain: [
            "Why: `None` only says that something is missing. The caller of a `Result` function expects to learn why it failed, and only you know which error fits (spec §14.3).\n\nInstead of:\n    let user = users.get(id)?\n\nwrite:\n    let user = users.get(id).ok_or(AppError.NotFound)?",
            "السبب: `لا_شيء` تقول فقط إن شيئًا غائب. ومن يستدعي دالة ترجع `نتيجة` ينتظر أن يعرف سبب الفشل، وأنت وحدك تعرف أي خطأ يناسب (المواصفة §14.3).\n\nبدلًا من:\n    ليكن المستخدم = المستخدمون.get(الرقم)؟\n\nاكتب:\n    ليكن المستخدم = المستخدمون.get(الرقم).ok_or(خطأ_التطبيق.غير_موجود)؟",
            "Leh: `mafeesh` bet2ool bas en fe 7aga na2sa. W elly bynady dala betraga3 `natiga` mestanny ye3raf leh fashlet, w enta bas elly 3aref anhy error yenfa3 (el spec §14.3).\n\nBadal:\n    khali user = users.get(id)?\n\nEkteb:\n    khali user = users.get(id).ok_or(AppError.NotFound)?",
        ],
        proposed: false,
    },
    // ---- Concurrency (K14xx, W0421, spec §15). ar/franco: pending native review.
    Entry {
        code: "K1401",
        title: ["SUSPENDING FUNCTION WHERE @sync IS REQUIRED", "دالة معلِّقة حيث يُشترط @sync", "DALA BTE3MEL SUSPEND MAKAN @sync"],
        message: [
            "This function is passed where a `@sync` function is required, but it can suspend: somewhere in its calls it waits for I/O or a timer. The chain of calls shows where.",
            "تُمرَّر هذه الدالة حيث تُشترط دالة `@sync`، لكنها قد تتعلق: في مكان ما من استدعاءاتها تنتظر إدخالًا وإخراجًا أو مؤقتًا. وسلسلة الاستدعاءات تبيّن أين.",
            "El dala di metba3ata f makan 3ayez dala `@sync`, bas heya momken te3mel suspend: f 7etta men el calls bta3etha btestanna I/O aw timer. W selselet el calls bet-warrik fein.",
        ],
        help: [
            "Move the waiting call out of this function (do the I/O before or after, and pass the result in), or pass a function that doesn't suspend.",
            "انقل الاستدعاء المنتظِر خارج هذه الدالة (نفّذ الإدخال والإخراج قبلها أو بعدها ومرّر النتيجة إليها)، أو مرّر دالة لا تتعلق.",
            "Na2al el call elly bystanna bara el dala di (e3mel el I/O 2ablaha aw ba3daha w ba3at el natiga leeha), aw ba3at dala mabte3melsh suspend.",
        ],
        explain: [
            "Why: some callbacks must finish without waiting: a game's `update`, the function given to `Cell.update`, a sort comparator. If one of them waited for a database, other code could run in the middle and see half-updated state. Kalam infers which functions suspend, so the error can name the exact chain (spec §15.4).\n\nInstead of:\n    fn update(s: State, input: Input, dt: Float) -> State:\n        save_score(s.score)\n        return s\n\nwrite:\n    fn update(s: State, input: Input, dt: Float) -> State:\n        return s.with(pending_save: true)\n\nand save the score outside `update`.",
            "السبب: بعض الدوال يجب أن تنتهي دون انتظار: `update` في لعبة، والدالة الممرّرة إلى `Cell.update`، ودالة المقارنة في الترتيب. ولو انتظرت إحداها قاعدة بيانات لأمكن لكود آخر أن يعمل في المنتصف فيرى حالة نصف محدّثة. ويستنتج كلام أي الدوال تتعلق، فيستطيع الخطأ تسمية السلسلة بالضبط (المواصفة §15.4).\n\nبدلًا من:\n    دالة update(ح: حالة، م: مدخل، ز: كسري) -> حالة:\n        احفظ_النتيجة(ح.النتيجة)\n        أرجع ح\n\nاكتب:\n    دالة update(ح: حالة، م: مدخل، ز: كسري) -> حالة:\n        أرجع ح.with(حفظ_معلق: صح)\n\nواحفظ النتيجة خارج `update`.",
            "Leh: fe dawal lazem tekhallas men gher ma testanna: `update` f le3ba, wel dala elly betetba3at l `Cell.update`, w dala el moqarna f el tarteeb. Law wa7da menhom estannet database, code tany momken yeshtaghal f el nos w yshoof state metghayara nosaha. W Kalam byestantag anhy dawal bte3mel suspend, fa el error ye2dar yesammy el selsela bel zabt (el spec §15.4).\n\nBadal:\n    dala update(s: State, input: Input, dt: kasri) -> State:\n        save_score(s.score)\n        rag3 s\n\nEkteb:\n    dala update(s: State, input: Input, dt: kasri) -> State:\n        rag3 s.with(pending_save: sa7)\n\nw e7faz el score bara `update`.",
        ],
        proposed: false,
    },
    Entry {
        code: "K1402",
        title: ["@sync FUNCTION SUSPENDS", "دالة @sync تتعلق", "DALA @sync BTE3MEL SUSPEND"],
        message: [
            "This function is marked `@sync`, a promise that it never waits, but it calls something that can suspend. The chain of calls shows where.",
            "هذه الدالة معلَّمة بـ `@sync`، وهذا وعد بأنها لا تنتظر أبدًا، لكنها تستدعي شيئًا قد يتعلق. وسلسلة الاستدعاءات تبيّن أين.",
            "El dala di 3aleiha `@sync`, ya3ny wa3d enaha 3omraha ma testanna, bas heya btenady 7aga momken te3mel suspend. W selselet el calls bet-warrik fein.",
        ],
        help: [
            "Remove the waiting call from this function, or remove `@sync` if the function really needs to wait (callers that rely on it will then be reported).",
            "احذف الاستدعاء المنتظِر من هذه الدالة، أو احذف `@sync` إن كانت الدالة تحتاج فعلًا إلى الانتظار (وسيُبلَّغ حينها عن المستدعين الذين يعتمدون عليها).",
            "Shil el call elly bystanna men el dala di, aw shil `@sync` law el dala fe3lan me7taga testanna (w saa3etha el amaken elly mo3tamda 3aleiha hatetballagh).",
        ],
        explain: [
            "Why: `@sync` on a declaration freezes an API promise: callers may use the function where waiting is not allowed. Breaking the promise inside the function is caught here, at its source, instead of at every caller (spec §15.4).\n\nInstead of:\n    @sync\n    fn price(item: Item) -> Decimal:\n        return load_rate(item.currency)? * item.amount\n\nwrite:\n    @sync\n    fn price(item: Item, rate: Decimal) -> Decimal:\n        return rate * item.amount",
            "السبب: `@sync` على تعريف دالة يثبّت وعدًا في واجهتها: يمكن للمستدعين استخدامها حيث لا يُسمح بالانتظار. وكسر هذا الوعد داخل الدالة يُكتشف هنا عند مصدره، لا عند كل مستدعٍ (المواصفة §15.4).\n\nبدلًا من:\n    @sync\n    دالة السعر(البند: بند) -> عشري:\n        أرجع حمّل_المعدل(البند.العملة)؟ * البند.الكمية\n\nاكتب:\n    @sync\n    دالة السعر(البند: بند، المعدل: عشري) -> عشري:\n        أرجع المعدل * البند.الكمية",
            "Leh: `@sync` 3ala ta3reef dala bysabbet wa3d f el API: elly bynadoha y2daro yesta5demoha f amaken mamnoo3 feeha el estenna. W kasr el wa3d da gowa el dala byetmesek hena 3and masdaro, mesh 3and kol makan bynadeeha (el spec §15.4).\n\nBadal:\n    @sync\n    dala se3r(item: Item) -> ashri:\n        rag3 load_rate(item.currency)? * item.amount\n\nEkteb:\n    @sync\n    dala se3r(item: Item, rate: ashri) -> ashri:\n        rag3 rate * item.amount",
        ],
        proposed: false,
    },
    Entry {
        code: "W0421",
        title: ["CELL VALUE USED ACROSS A WAIT", "قيمة خلية مستخدمة عبر انتظار", "QEEMET CELL MOSTA5DAMA BA3D ESTENNA"],
        message: [
            "This value was read from a `Cell` before a call that waits, and it is used after that call. While this task waited, another task may have changed the `Cell`.",
            "قُرئت هذه القيمة من `Cell` قبل استدعاء ينتظر، وتُستخدم بعده. وأثناء انتظار هذه المهمة ربما غيّرت مهمة أخرى الـ `Cell`.",
            "El qeema di et2araet men `Cell` 2abl call bystanna, w btesta5dem ba3do. W f w2t ma el task di kanet mestanya, momken task tany ykoon ghayar el `Cell`.",
        ],
        help: [
            "Read the `Cell` again after the wait, or do the whole read-and-change in one `.update(...)`, which never waits.",
            "اقرأ الـ `Cell` من جديد بعد الانتظار، أو نفّذ القراءة والتغيير كليهما في `.update(...)` واحدة، فهي لا تنتظر أبدًا.",
            "E2ra el `Cell` tany ba3d el estenna, aw e3mel el 2eraya wel taghyeer el etnein f `.update(...)` wa7da, 3ashan heya 3omraha ma btestanna.",
        ],
        explain: [
            "Why: between two waits a task runs alone, so reading and writing a `Cell` without waiting in between is safe. A wait in the middle opens a gap where the value can go stale: the classic check-then-act race (spec §15.6).\n\nInstead of:\n    let n = stock.get()\n    notify(order)\n    stock.set(n - 1)\n\nwrite:\n    stock.update(fn(n) => n - 1)\n    notify(order)",
            "السبب: بين انتظارين تعمل المهمة وحدها، فقراءة `Cell` والكتابة فيها دون انتظار بينهما آمنة. أما الانتظار في المنتصف فيفتح فجوة قد تصبح فيها القيمة قديمة: وهو سباق \"افحص ثم نفّذ\" المعروف (المواصفة §15.6).\n\nبدلًا من:\n    ليكن ن = المخزون.get()\n    نبّه(الطلب)\n    المخزون.set(ن - 1)\n\nاكتب:\n    المخزون.update(دالة(ن) => ن - 1)\n    نبّه(الطلب)",
            "Leh: ben estennaein el task byeshtaghal lewa7do, fa enak te2ra w tekteb f `Cell` men gher estenna f el nos amaan. Laken estenna f el nos byefta7 fagwa el qeema momken teb2a adeema feeha: el race el ma3roof \"check ba3dein e3mel\" (el spec §15.6).\n\nBadal:\n    khali n = stock.get()\n    nabbeh(order)\n    stock.set(n - 1)\n\nEkteb:\n    stock.update(dala(n) => n - 1)\n    nabbeh(order)",
        ],
        proposed: false,
    },
    // ---- Capabilities (K15xx, spec §16). ar/franco: pending native review.
    Entry {
        code: "K1501",
        title: ["WORLD CAN'T BE CREATED", "لا يمكن إنشاء عالم", "DONYA MATET3AMALSH"],
        message: [
            "I can't let this code construct or decode a `{core_world}`. Only `{core_main}` receives the `{core_world}`, so every power a program has can be traced back to it.",
            "لا أستطيع السماح لهذا الكود بإنشاء `{core_world}` أو فك ترميزه. وحدها `{core_main}` تستلم `{core_world}`، حتى يمكن تتبع كل صلاحية في البرنامج إليها.",
            "Mesh ha2dar asma7 lel code da ye3mel aw ye3mel decode l `{core_world}`. `{core_main}` bas heya elly btestelem el `{core_world}`, 3ashan kol salaheya f el program yet-tabba3 leeha.",
        ],
        help: [
            "Take the capability you need as a parameter (`net: Net`, `fs: Fs`), and pass it down from `{core_main}`.",
            "خذ الصلاحية التي تحتاجها كمعامل (`net: Net`، `fs: Fs`)، ومرّرها إليك من `{core_main}`.",
            "Khod el salaheya elly me7tagha ka parameter (`net: Net`, `fs: Fs`), w ba3atha men `{core_main}` lta7t.",
        ],
        explain: [
            "Why: in Kalam, a function can reach the network, the files or the environment only if it was handed that capability. If any code could make a `World`, a library deep in your dependencies could grant itself everything (spec §16.1).\n\nInstead of:\n    fn fetch_rates() -> Result[Rates, HttpError]:\n        let world = World()\n        ...\n\nwrite:\n    fn fetch_rates(net: Net) -> Result[Rates, HttpError]:\n        ...",
            "السبب: في كلام لا تصل الدالة إلى الشبكة أو الملفات أو البيئة إلا إذا أُعطيت هذه الصلاحية. ولو أمكن لأي كود أن ينشئ `عالم` لاستطاعت مكتبة عميقة في اعتمادياتك أن تمنح نفسها كل شيء (المواصفة §16.1).\n\nبدلًا من:\n    دالة اجلب_الأسعار() -> نتيجة[أسعار، HttpError]:\n        ليكن ع = عالم()\n        ...\n\nاكتب:\n    دالة اجلب_الأسعار(net: Net) -> نتيجة[أسعار، HttpError]:\n        ...",
            "Leh: f Kalam el dala mabtewsalsh lel network aw el files aw el environment ella law et-ad-dalha el salaheya di. W law ay code ye2dar ye3mel `donya`, library ba3eeda f el dependencies bta3tak te2dar tedy nafsaha kol 7aga (el spec §16.1).\n\nBadal:\n    dala hat_rates() -> natiga[Rates, HttpError]:\n        khali w = donya()\n        ...\n\nEkteb:\n    dala hat_rates(net: Net) -> natiga[Rates, HttpError]:\n        ...",
        ],
        proposed: false,
    },
    Entry {
        code: "K1502",
        title: ["SCREEN NEEDS THE BROWSER TARGET", "الشاشة تحتاج هدف المتصفح", "EL SCREEN ME7TAGA EL BROWSER"],
        message: [
            "This code uses `Screen`, which exists only when the program runs in a browser (the `js-browser` target), and this build targets something else.",
            "هذا الكود يستخدم `Screen`، وهي موجودة فقط حين يعمل البرنامج في متصفح (الهدف `js-browser`)، وهذا البناء لهدف آخر.",
            "El code da besta5dem `Screen`, w di mawgooda bas lamma el program yeshtaghal f browser (el target `js-browser`), w el build da l target tany.",
        ],
        help: [
            "Build this program for the `js-browser` target (`targets` in `kalam.toml`), or move the drawing code to a package that is built only for the browser.",
            "ابنِ هذا البرنامج للهدف `js-browser` (الحقل `targets` في `kalam.toml`)، أو انقل كود الرسم إلى حزمة لا تُبنى إلا للمتصفح.",
            "E3mel build lel program da lel target `js-browser` (`targets` f `kalam.toml`), aw na2al code el rasm l package btetbene lel browser bas.",
        ],
        explain: [
            "Why: a server or a command-line program has no canvas and no mouse. Checking the target at compile time turns a crash at start-up into a message that names the line (spec §16.1).\n\nA game is built for the browser. In kalam.toml:\n    [package]\n    targets = [\"js-browser\"]",
            "السبب: البرنامج الذي يعمل على خادم أو في سطر الأوامر ليس له لوحة رسم ولا فأرة. والتحقق من الهدف وقت الترجمة يحوّل انهيارًا عند التشغيل إلى رسالة تسمّي السطر (المواصفة §16.1).\n\nاللعبة تُبنى للمتصفح. في kalam.toml:\n    [package]\n    targets = [\"js-browser\"]",
            "Leh: el program elly byeshtaghal 3ala server aw f command line malosh canvas wala mouse. W el check 3ala el target w2t el compile by7awel crash 3and el tashgheel l resala betsammy el satr (el spec §16.1).\n\nEl le3ba btetbene lel browser. F kalam.toml:\n    [package]\n    targets = [\"js-browser\"]",
        ],
        proposed: false,
    },
    Entry {
        code: "K1503",
        title: ["SECRET READ AS PLAIN TEXT", "سرّ يُقرأ كنص عادي", "SECRET BYET2ERI KA NASS 3ADI"],
        message: [
            "This key looks like a secret (it ends in `_KEY`, `_SECRET` or `_TOKEN`, or names a password), but `env.get` returns plain text, which can end up in logs or JSON.",
            "هذا المفتاح يبدو سرًا (ينتهي بـ `_KEY` أو `_SECRET` أو `_TOKEN`، أو يسمّي كلمة مرور)، لكن `env.get` ترجع نصًا عاديًا قد ينتهي في السجلات أو في JSON.",
            "El key da shaklo secret (byekhlas b `_KEY` aw `_SECRET` aw `_TOKEN`, aw fe esmo password), bas `env.get` betraga3 nass 3adi momken yekhlas f el logs aw f JSON.",
        ],
        help: [
            "Use `env.secret(key)`, which returns `Secret[{core_string}]`: it prints as `[REDACTED]` and can never be encoded.",
            "استخدم `env.secret(key)`، فهي ترجع `Secret[{core_string}]`: تُطبع `[REDACTED]` ولا يمكن ترميزها أبدًا.",
            "Esta5dem `env.secret(key)`, 3ashan betraga3 `Secret[{core_string}]`: btetteba3 `[REDACTED]` w 3omraha ma tet3amallaha encode.",
        ],
        explain: [
            "Why: secrets leak through ordinary code paths: a debug print, an error message, a JSON response. Keeping them in `Secret` from the moment they are read means every one of those paths is closed by the type system (spec §16.1, §18.1).\n\nInstead of:\n    let key = env.get(\"STRIPE_API_KEY\")\n\nwrite:\n    let key = env.secret(\"STRIPE_API_KEY\")",
            "السبب: تتسرب الأسرار عبر مسارات كود عادية: طباعة للتصحيح، أو رسالة خطأ، أو رد JSON. وحفظها في `Secret` من لحظة قراءتها يعني أن نظام الأنواع يغلق كل هذه المسارات (المواصفة §16.1، §18.1).\n\nبدلًا من:\n    ليكن المفتاح = env.get(\"STRIPE_API_KEY\")\n\nاكتب:\n    ليكن المفتاح = env.secret(\"STRIPE_API_KEY\")",
            "Leh: el asrar betetsarrab men masarat code 3adeya: print lel debug, aw resalet error, aw response JSON. W lamma te7fazha f `Secret` men awel ma tet2ery, el type system bye2fel kol el masarat di (el spec §16.1, §18.1).\n\nBadal:\n    khali key = env.get(\"STRIPE_API_KEY\")\n\nEkteb:\n    khali key = env.secret(\"STRIPE_API_KEY\")",
        ],
        proposed: false,
    },
    Entry {
        code: "K1504",
        title: ["LIBRARY PRINTS WITHOUT CONSOLE", "مكتبة تطبع دون console", "LIBRARY BTETBA3 MEN GHER CONSOLE"],
        message: [
            "This library package calls `{core_print}` or `log.*`, but its manifest doesn't declare `console = true`. Apps that use a library need to know it writes to their output.",
            "حزمة المكتبة هذه تستدعي `{core_print}` أو `log.*`، لكن ملف تعريفها لا يعلن `console = true`. والتطبيقات التي تستخدم مكتبة تحتاج أن تعرف أنها تكتب في مخرجاتها.",
            "El library package di btenady `{core_print}` aw `log.*`, bas el manifest bta3ha mesh m3allen `console = true`. W el apps elly betesta5dem library me7taga te3raf enaha btekteb f el output bta3hom.",
        ],
        help: [
            "Add `console = true` to the package manifest, or return the text to the caller instead of printing it.",
            "أضف `console = true` إلى ملف تعريف الحزمة، أو أرجع النص إلى المستدعي بدل طباعته.",
            "Zawed `console = true` lel manifest bta3 el package, aw raga3 el kalam lelly bynady badal ma tetba3o.",
        ],
        explain: [
            "Why: `print` and `log.*` need no capability, which keeps small programs simple. For a library that is a hidden side effect, so the manifest has to declare it, and `kalam audit` shows it to the people who depend on the package (spec §16.4).\n\nIn kalam.toml:\n    [capabilities]\n    console = true",
            "السبب: لا تحتاج `اطبع` و`log.*` إلى صلاحية، وهذا يبقي البرامج الصغيرة بسيطة. لكنها في المكتبة أثر جانبي خفي، فيجب أن يعلنه ملف التعريف، ويعرضه `kalam audit` لمن يعتمدون على الحزمة (المواصفة §16.4).\n\nفي kalam.toml:\n    [capabilities]\n    console = true",
            "Leh: `ektb` w `log.*` mesh me7tageen salaheya, w da bykhalli el programs el soghayara baseeta. Laken f el library da side effect mestakhabby, fa lazem el manifest y3allen 3anno, w `kalam audit` bey3redo lel nas elly mo3tamdeen 3ala el package (el spec §16.4).\n\nF kalam.toml:\n    [capabilities]\n    console = true",
        ],
        proposed: false,
    },
    // ---- SQL, HTML, command, URL and path literals (K16xx, W1606, W1622, spec §17).
    // ar/franco: pending native review.
    Entry {
        code: "K1602",
        title: ["SQL HOLE INSIDE QUOTES", "فجوة SQL داخل علامات اقتباس", "HOLE SQL GOWA QUOTES"],
        message: [
            "This hole is inside SQL quotes. Values in holes are sent to the database as parameters, never pasted into the text, so the quotes would make it a literal string.",
            "هذه الفجوة داخل علامات اقتباس SQL. تُرسل قيم الفجوات إلى قاعدة البيانات كمعاملات ولا تُلصق في النص أبدًا، فكانت علامات الاقتباس ستجعلها نصًا ثابتًا.",
            "El hole di gowa quotes SQL. Qeyam el holes betetba3at lel database ka parameters, 3omraha ma betetlezze2 f el nass, fa el quotes kanet hatkhalleeha string sabet.",
        ],
        help: [
            "Remove the quotes around the hole: write `name = { name }`, not `name = '{ name }'`.",
            "احذف علامتي الاقتباس حول الفجوة: اكتب `name = { name }` لا `name = '{ name }'`.",
            "Shil el quotes elly 7awalen el hole: ekteb `name = { name }`, mesh `name = '{ name }'`.",
        ],
        explain: [
            "Why: a `sql\"…\"` literal turns each hole into a query parameter, which is what makes it immune to injection. Quotes around a hole come from habits with string-built SQL and would turn the parameter into fixed text (spec §17.3).\n\nInstead of:\n    sql\"SELECT * FROM users WHERE name = '{ name }'\"\n\nwrite:\n    sql\"SELECT * FROM users WHERE name = { name }\"",
            "السبب: القيمة الحرفية `sql\"…\"` تحوّل كل فجوة إلى معامل في الاستعلام، وهذا ما يحميها من الحقن. وعلامات الاقتباس حول الفجوة عادة من بناء SQL بالنصوص، وكانت ستحوّل المعامل إلى نص ثابت (المواصفة §17.3).\n\nبدلًا من:\n    sql\"SELECT * FROM users WHERE name = '{ الاسم }'\"\n\nاكتب:\n    sql\"SELECT * FROM users WHERE name = { الاسم }\"",
            "Leh: el literal `sql\"…\"` by7awel kol hole l parameter f el query, w da elly bey7meeh men el injection. W el quotes 7awalen el hole 3ada men bena2 SQL bel strings, w kanet hat7awel el parameter l nass sabet (el spec §17.3).\n\nBadal:\n    sql\"SELECT * FROM users WHERE name = '{ esm }'\"\n\nEkteb:\n    sql\"SELECT * FROM users WHERE name = { esm }\"",
        ],
        proposed: false,
    },
    Entry {
        code: "K1603",
        title: ["SQL HOLE AS A NAME", "فجوة SQL في موضع اسم", "HOLE SQL MAKAN ESM"],
        message: [
            "This hole is where SQL expects a name (a table, a column, `ASC` or `DESC`), not a value. A query parameter can't fill a name, and pasting text there would open the door to injection.",
            "هذه الفجوة في موضع ينتظر فيه SQL اسمًا (جدولًا أو عمودًا أو `ASC` أو `DESC`) لا قيمة. ولا يمكن لمعامل الاستعلام أن يملأ اسمًا، ولصق نص هناك كان سيفتح باب الحقن.",
            "El hole di f makan SQL mestanny feeh esm (table aw column aw `ASC` aw `DESC`), mesh qeema. W parameter el query mayenfa3sh yemla esm, w lez2 nass hena kan hayefta7 bab el injection.",
        ],
        help: [
            "Build the name with `sql.ident(name, allowed: [...])`, which checks it against a fixed list, or use an enum whose cases return fixed names.",
            "ابنِ الاسم بـ `sql.ident(name, allowed: [...])` التي تتحقق منه مقابل قائمة ثابتة، أو استخدم تعدادًا ترجع حالاته أسماء ثابتة.",
            "Ebny el esm b `sql.ident(name, allowed: [...])` elly bet-check-o 3ala list sabta, aw esta5dem enum el 7alat bta3to betraga3 asamy sabta.",
        ],
        explain: [
            "Why: sorting by a column the user picked is a common need, and the usual shortcut pastes the user's text into the query. Here the user picks among names you listed, and the name itself always comes from your code (spec §17.3).\n\nInstead of:\n    sql\"SELECT * FROM notes ORDER BY { column }\"\n\nwrite:\n    let col = sql.ident(column, allowed: [\"title\", \"created_at\"])?\n    sql\"SELECT * FROM notes ORDER BY { col }\"",
            "السبب: الترتيب حسب عمود يختاره المستخدم حاجة شائعة، والاختصار المعتاد يلصق نص المستخدم في الاستعلام. أما هنا فيختار المستخدم من أسماء حددتها أنت، ويأتي الاسم نفسه دائمًا من كودك (المواصفة §17.3).\n\nبدلًا من:\n    sql\"SELECT * FROM notes ORDER BY { العمود }\"\n\nاكتب:\n    ليكن ع = sql.ident(العمود، allowed: [\"title\"، \"created_at\"])؟\n    sql\"SELECT * FROM notes ORDER BY { ع }\"",
            "Leh: el tarteeb b column el user ekhtaro 7aga mentashra, wel ekhtesar el 3adi byelze2 kalam el user f el query. Laken hena el user byekhtar men asamy enta 7addedtaha, wel esm nafso dayman gay men el code bta3ak (el spec §17.3).\n\nBadal:\n    sql\"SELECT * FROM notes ORDER BY { column }\"\n\nEkteb:\n    khali col = sql.ident(column, allowed: [\"title\", \"created_at\"])?\n    sql\"SELECT * FROM notes ORDER BY { col }\"",
        ],
        proposed: false,
    },
    Entry {
        code: "K1604",
        title: ["SEVERAL SQL STATEMENTS", "عدة جمل SQL", "KAZA STATEMENT SQL"],
        message: [
            "This SQL literal holds more than one statement (a `;` outside quotes). Each `Sql` value is exactly one statement.",
            "هذه القيمة الحرفية لـ SQL فيها أكثر من جملة (`;` خارج علامات الاقتباس). وكل قيمة `Sql` جملة واحدة بالضبط.",
            "El SQL literal da feeh aktar men statement (`;` bara el quotes). W kol qeema `Sql` statement wa7ed bel zabt.",
        ],
        help: [
            "Split it into one `sql\"...\"` per statement, and run them inside a transaction if they must succeed together.",
            "قسّمها إلى `sql\"...\"` لكل جملة، ونفّذها داخل معاملة إن كان يجب أن تنجح معًا.",
            "Qassemha l `sql\"...\"` l kol statement, w shaghalhom gowa transaction law lazem yenga7o ma3 ba3d.",
        ],
        explain: [
            "Why: stacked statements are how one injected `;` turns a lookup into a `DROP TABLE`. With one statement per value, that attack shape cannot exist, and `unsafe.raw_sql` rejects a top-level `;` at run time too (spec §17.3).\n\nInstead of:\n    db.exec(sql\"DELETE FROM carts WHERE id = { id }; DELETE FROM items WHERE cart = { id }\")?\n\nwrite:\n    db.exec(sql\"DELETE FROM items WHERE cart = { id }\")?\n    db.exec(sql\"DELETE FROM carts WHERE id = { id }\")?",
            "السبب: الجمل المتتالية هي الطريقة التي يحوّل بها `;` محقون استعلامًا إلى `DROP TABLE`. ومع جملة واحدة لكل قيمة لا يمكن أن يوجد هذا الشكل من الهجوم، و`unsafe.raw_sql` ترفض `;` في المستوى الأعلى وقت التشغيل أيضًا (المواصفة §17.3).\n\nبدلًا من:\n    db.exec(sql\"DELETE FROM carts WHERE id = { الرقم }; DELETE FROM items WHERE cart = { الرقم }\")؟\n\nاكتب:\n    db.exec(sql\"DELETE FROM items WHERE cart = { الرقم }\")؟\n    db.exec(sql\"DELETE FROM carts WHERE id = { الرقم }\")؟",
            "Leh: el statements el wara ba3d heya el taree2a elly `;` ma73oon by7awel beeha query l `DROP TABLE`. Ma3 statement wa7ed l kol qeema, shakl el hagma da mayenfa3sh yeb2a mawgood, w `unsafe.raw_sql` bterfod `;` f el mostawa el a3la w2t el tashgheel kaman (el spec §17.3).\n\nBadal:\n    db.exec(sql\"DELETE FROM carts WHERE id = { id }; DELETE FROM items WHERE cart = { id }\")?\n\nEkteb:\n    db.exec(sql\"DELETE FROM items WHERE cart = { id }\")?\n    db.exec(sql\"DELETE FROM carts WHERE id = { id }\")?",
        ],
        proposed: false,
    },
    Entry {
        code: "K1605",
        title: ["SQL HOLE INSIDE A COMMENT", "فجوة SQL داخل تعليق", "HOLE SQL GOWA COMMENT"],
        message: [
            "This hole is inside an SQL comment, so the database would never see its value. It is almost always a leftover from editing the query.",
            "هذه الفجوة داخل تعليق SQL، فلن ترى قاعدة البيانات قيمتها أبدًا. وغالبًا ما تكون بقية من تعديل الاستعلام.",
            "El hole di gowa comment SQL, fa el database 3omraha ma hatshoof qeemetha. W ghaleban btekoon ba2eya men ta3deel el query.",
        ],
        help: [
            "Move the hole out of the comment, or delete it.",
            "انقل الفجوة خارج التعليق، أو احذفها.",
            "Talla3 el hole bara el comment, aw emsa7ha.",
        ],
        explain: [
            "Why: a hole the database ignores means the query does something other than what the code seems to say. Comments themselves are fine in a `sql\"…\"` literal (spec §17.3).\n\nInstead of:\n    sql\"SELECT * FROM notes -- WHERE owner = { user.id }\"\n\nwrite:\n    sql\"SELECT * FROM notes WHERE owner = { user.id }\"",
            "السبب: الفجوة التي تتجاهلها قاعدة البيانات تعني أن الاستعلام يفعل غير ما يبدو أن الكود يقوله. أما التعليقات نفسها فمسموحة في `sql\"…\"` (المواصفة §17.3).\n\nبدلًا من:\n    sql\"SELECT * FROM notes -- WHERE owner = { المستخدم.الرقم }\"\n\nاكتب:\n    sql\"SELECT * FROM notes WHERE owner = { المستخدم.الرقم }\"",
            "Leh: el hole elly el database betetgahelha ma3naha en el query bey3mel 7aga gher elly el code shaklo bey2oolha. Laken el comments nafsaha masmoo7a f `sql\"…\"` (el spec §17.3).\n\nBadal:\n    sql\"SELECT * FROM notes -- WHERE owner = { user.id }\"\n\nEkteb:\n    sql\"SELECT * FROM notes WHERE owner = { user.id }\"",
        ],
        proposed: false,
    },
    Entry {
        code: "W1606",
        title: ["RAW STRING IN A LIKE PATTERN", "نص خام في نمط LIKE", "STRING KHAM F PATTERN LIKE"],
        message: [
            "A plain `{core_string}` fills this `LIKE` hole, so any `%` or `_` the user types acts as a wildcard and matches more than they asked for.",
            "يملأ هذه الفجوة في `LIKE` نصٌّ `{core_string}` عادي، فيعمل أي `%` أو `_` يكتبه المستخدم كحرف بدل ويطابق أكثر مما طلب.",
            "`{core_string}` 3adi byemla el hole di f `LIKE`, fa ay `%` aw `_` el user yekteboh beyshtaghal ka wildcard w yetabe2 aktar men elly talabo.",
        ],
        help: [
            "Escape the text with `sql.like_contains(s)`, `sql.like_prefix(s)` or `sql.like_suffix(s)`.",
            "هرّب النص بـ `sql.like_contains(s)` أو `sql.like_prefix(s)` أو `sql.like_suffix(s)`.",
            "E3mel escape lel nass b `sql.like_contains(s)` aw `sql.like_prefix(s)` aw `sql.like_suffix(s)`.",
        ],
        explain: [
            "Why: a hole is safe from injection, but `LIKE` gives `%` and `_` their own meaning inside the value. A search for `100%` would also match `1000`, and a lone `%` matches every row (spec §17.3).\n\nInstead of:\n    sql\"SELECT * FROM notes WHERE title LIKE { term }\"\n\nwrite:\n    sql\"SELECT * FROM notes WHERE title LIKE { sql.like_contains(term) }\"",
            "السبب: الفجوة آمنة من الحقن، لكن `LIKE` يعطي `%` و`_` معنى خاصًا داخل القيمة. فالبحث عن `100%` يطابق `1000` أيضًا، و`%` وحدها تطابق كل الصفوف (المواصفة §17.3).\n\nبدلًا من:\n    sql\"SELECT * FROM notes WHERE title LIKE { البحث }\"\n\nاكتب:\n    sql\"SELECT * FROM notes WHERE title LIKE { sql.like_contains(البحث) }\"",
            "Leh: el hole amna men el injection, bas `LIKE` bey-edy `%` w `_` ma3na khas gowa el qeema. Fa el ba7s 3an `100%` hayetabe2 `1000` kaman, w `%` lewa7daha btetabe2 kol el rows (el spec §17.3).\n\nBadal:\n    sql\"SELECT * FROM notes WHERE title LIKE { term }\"\n\nEkteb:\n    sql\"SELECT * FROM notes WHERE title LIKE { sql.like_contains(term) }\"",
        ],
        proposed: false,
    },
    Entry {
        code: "K1610",
        title: ["HTML LITERAL NOT WELL-FORMED", "قيمة HTML غير سليمة البناء", "HTML LITERAL MESH SALEEM"],
        message: [
            "I can't read this `html\"...\"` literal as a well-formed HTML fragment: a tag is unclosed or wrongly nested, or an attribute value is unquoted.",
            "لا أستطيع قراءة القيمة الحرفية `html\"...\"` هذه كجزء HTML سليم البناء: هناك وسم غير مغلق أو متداخل بشكل خاطئ، أو قيمة سمة بلا علامات اقتباس.",
            "Mesh ha2dar a2ra el `html\"...\"` da ka 7etta HTML saleema: fe tag mesh ma2fool aw metdakhel ghalat, aw qeemet attribute men gher quotes.",
        ],
        help: [
            "Close every tag in the order it was opened, and quote every attribute value.",
            "أغلق كل وسم بعكس ترتيب فتحه، وضع كل قيمة سمة بين علامتي اقتباس.",
            "E2fel kol tag bel tarteeb el ma3koos lel fat7, w 7ott kol qeemet attribute ben quotes.",
        ],
        explain: [
            "Why: I escape each hole according to where it sits in the HTML. That only works if I can parse the fragment at compile time; a browser would silently repair broken markup in a way that can move a hole into a different context (spec §17.4).\n\nInstead of:\n    html\"<p><b>{ name }</p></b>\"\n\nwrite:\n    html\"<p><b>{ name }</b></p>\"",
            "السبب: أهرّب كل فجوة حسب موضعها في HTML. وهذا لا يعمل إلا إذا استطعت تحليل الجزء وقت الترجمة، أما المتصفح فيصلح الوسوم المكسورة بصمت بطريقة قد تنقل فجوة إلى سياق مختلف (المواصفة §17.4).\n\nبدلًا من:\n    html\"<p><b>{ الاسم }</p></b>\"\n\nاكتب:\n    html\"<p><b>{ الاسم }</b></p>\"",
            "Leh: ana ba3mel escape l kol hole 7asab makanha f el HTML. W da mayenfa3sh ella law 2dert a7allel el 7etta w2t el compile; el browser bysalla7 el tags el maksoora f el sekket b taree2a momken tenaql hole l context tany (el spec §17.4).\n\nBadal:\n    html\"<p><b>{ esm }</p></b>\"\n\nEkteb:\n    html\"<p><b>{ esm }</b></p>\"",
        ],
        proposed: false,
    },
    Entry {
        code: "K1611",
        title: ["UNQUOTED ATTRIBUTE HOLE", "فجوة سمة بلا اقتباس", "HOLE ATTRIBUTE MEN GHER QUOTES"],
        message: [
            "This attribute's value is a hole without quotes. A value with a space or a `>` would end the attribute early and could add attributes of its own.",
            "قيمة هذه السمة فجوة بلا علامات اقتباس. والقيمة التي فيها مسافة أو `>` كانت ستنهي السمة مبكرًا وقد تضيف سمات من عندها.",
            "Qeemet el attribute da hole men gher quotes. W qeema feeha masafa aw `>` kanet hatkhallas el attribute badry w momken tzawed attributes men 3andaha.",
        ],
        help: [
            "Put quotes around the hole: `class=\"{ c }\"`. `kalam fix` adds them.",
            "ضع علامتي اقتباس حول الفجوة: `class=\"{ c }\"`. والأمر `kalam fix` يضيفهما.",
            "7ott quotes 7awalen el hole: `class=\"{ c }\"`. W `kalam fix` byzawedhom.",
        ],
        explain: [
            "Why: inside quotes I only need to escape the quote character; without them, many more characters change the meaning of the tag. Requiring quotes keeps escaping simple and complete (spec §17.4).\n\nInstead of:\n    html\"<div class={ c }>\"\n\nwrite:\n    html\"<div class=\\\"{ c }\\\">\"",
            "السبب: داخل علامات الاقتباس أحتاج فقط إلى تهريب علامة الاقتباس، وبدونها تغيّر حروف أكثر بكثير معنى الوسم. واشتراط علامات الاقتباس يبقي التهريب بسيطًا وكاملًا (المواصفة §17.4).\n\nبدلًا من:\n    html\"<div class={ ص }>\"\n\nاكتب:\n    html\"<div class=\\\"{ ص }\\\">\"",
            "Leh: gowa el quotes ana me7tag a3mel escape l 3alamet el quote bas; men gherha 7oroof keteer tanya betghayar ma3na el tag. W en el quotes tekoon lazma bykhalli el escape baseet w kamel (el spec §17.4).\n\nBadal:\n    html\"<div class={ c }>\"\n\nEkteb:\n    html\"<div class=\\\"{ c }\\\">\"",
        ],
        proposed: false,
    },
    Entry {
        code: "K1612",
        title: ["HOLE IN SCRIPT OR STYLE", "فجوة في سكربت أو نمط", "HOLE F SCRIPT AW STYLE"],
        message: [
            "This hole is in a place that runs as code: an `on*` attribute, a `style` attribute, or `<script>` or `<style>` content. No escaping makes a value safe there.",
            "هذه الفجوة في مكان يُنفَّذ ككود: سمة `on*`، أو سمة `style`، أو محتوى `<script>` أو `<style>`. ولا يوجد تهريب يجعل القيمة آمنة هناك.",
            "El hole di f makan byeshtaghal ka code: attribute `on*`, aw attribute `style`, aw gowa `<script>` aw `<style>`. W mafeesh escape yekhalli el qeema amna henak.",
        ],
        help: [
            "Pass data to scripts with `{ json(x) }` inside `<script type=\"application/json\">`, and choose styles with a class instead of a style hole.",
            "مرّر البيانات إلى السكربتات بـ `{ json(x) }` داخل `<script type=\"application/json\">`، واختر الأنماط بصنف class بدل فجوة في style.",
            "Ba3at el data lel scripts b `{ json(x) }` gowa `<script type=\"application/json\">`, w ekhtar el styles b class badal hole f style.",
        ],
        explain: [
            "Why: JavaScript and CSS each have their own quoting rules, nested inside HTML's. Values in those places are the classic source of cross-site scripting, so they get no holes at all; the one exception is JSON data, which is escaped for that exact context (spec §17.4).\n\nInstead of:\n    html\"<button onclick=\\\"buy({ id })\\\">Buy</button>\"\n\nwrite (a live component):\n    html\"<button k-click=\\\"{ Msg.Buy(id) }\\\">Buy</button>\"",
            "السبب: لكل من JavaScript وCSS قواعد اقتباس خاصة به، متداخلة داخل قواعد HTML. والقيم في هذه المواضع هي المصدر التقليدي لهجمات XSS، لذلك لا فجوات فيها أصلًا، والاستثناء الوحيد بيانات JSON التي تُهرَّب لهذا السياق تحديدًا (المواصفة §17.4).\n\nبدلًا من:\n    html\"<button onclick=\\\"buy({ الرقم })\\\">اشترِ</button>\"\n\nاكتب (في مكوّن حي):\n    html\"<button k-click=\\\"{ رسالة.اشترِ(الرقم) }\\\">اشترِ</button>\"",
            "Leh: JavaScript w CSS kol wa7ed leeh qawa3ed quotes khassa, metdakhla gowa qawa3ed el HTML. W el qeyam f el amaken di heya el masdar el ma3roof l XSS, 3ashan keda mafeehash holes khales; el estesna2 el wa7eed data JSON elly beyet3amallaha escape lel context da bel zabt (el spec §17.4).\n\nBadal:\n    html\"<button onclick=\\\"buy({ id })\\\">Buy</button>\"\n\nEkteb (f live component):\n    html\"<button k-click=\\\"{ Msg.Buy(id) }\\\">Buy</button>\"",
        ],
        proposed: false,
    },
    Entry {
        code: "K1613",
        title: ["URL ATTRIBUTE NEEDS A URL", "سمة الرابط تحتاج Url", "ATTRIBUTE EL LINK ME7TAG URL"],
        message: [
            "This hole fills a URL attribute (such as `href` or `src`), so its value must be a `Url`, not a `{core_string}`. A string could be a `javascript:` link.",
            "هذه الفجوة تملأ سمة رابط (مثل `href` أو `src`)، فيجب أن تكون قيمتها `Url` لا `{core_string}`. فالنص قد يكون رابط `javascript:`.",
            "El hole di betemla attribute link (zay `href` aw `src`), fa qeemetha lazem teb2a `Url`, mesh `{core_string}`. El string momken yeb2a link `javascript:`.",
        ],
        help: [
            "Build the link with a `url\"...\"` literal, or parse text with `Url.parse(s)`, which refuses `javascript:`, `data:` and `vbscript:`.",
            "ابنِ الرابط بقيمة حرفية `url\"...\"`، أو حلّل النص بـ `Url.parse(s)` التي ترفض `javascript:` و`data:` و`vbscript:`.",
            "Ebny el link b literal `url\"...\"`, aw 7allel el nass b `Url.parse(s)` elly bterfod `javascript:` w `data:` w `vbscript:`.",
        ],
        explain: [
            "Why: HTML-escaping a link does not make it safe: `javascript:alert(1)` contains nothing to escape. The `Url` type can only hold safe schemes, so checking the type is enough (spec §17.4).\n\nInstead of:\n    html\"<a href=\\\"{ link }\\\">profile</a>\"\n\nwrite:\n    let link = Url.parse(text)?\n    html\"<a href=\\\"{ link }\\\">profile</a>\"",
            "السبب: تهريب الرابط في HTML لا يجعله آمنًا، فالرابط `javascript:alert(1)` ليس فيه ما يُهرَّب. والنوع `Url` لا يحمل إلا مخططات آمنة، فيكفي التحقق من النوع (المواصفة §17.4).\n\nبدلًا من:\n    html\"<a href=\\\"{ الرابط }\\\">الملف</a>\"\n\nاكتب:\n    ليكن الرابط = Url.parse(النص)؟\n    html\"<a href=\\\"{ الرابط }\\\">الملف</a>\"",
            "Leh: el escape bta3 el link f HTML mesh bykhalleeh amn: `javascript:alert(1)` mafeehoosh 7aga tet3amallaha escape. W no3 `Url` mayesh-shelsh ella schemes amna, fa el check 3ala el type kefaya (el spec §17.4).\n\nBadal:\n    html\"<a href=\\\"{ link }\\\">profile</a>\"\n\nEkteb:\n    khali link = Url.parse(text)?\n    html\"<a href=\\\"{ link }\\\">profile</a>\"",
        ],
        proposed: false,
    },
    Entry {
        code: "K1614",
        title: ["HOLE IN A TAG OR ATTRIBUTE NAME", "فجوة في اسم وسم أو سمة", "HOLE F ESM TAG AW ATTRIBUTE"],
        message: [
            "This hole is in a tag name or an attribute name. Names decide what the markup does, so they must be written in the literal.",
            "هذه الفجوة في اسم وسم أو اسم سمة. والأسماء تحدد ما تفعله الوسوم، فيجب أن تُكتب في القيمة الحرفية نفسها.",
            "El hole di f esm tag aw esm attribute. W el asamy heya elly bet7added el markup bey3mel eh, fa lazem tetketeb f el literal nafso.",
        ],
        help: [
            "Write the name literally. For a changing set of attributes, use `Attrs`, whose names are a fixed list.",
            "اكتب الاسم حرفيًا. ولمجموعة سمات تتغير، استخدم `Attrs` التي أسماؤها قائمة ثابتة.",
            "Ekteb el esm zay ma howa. W l magmoo3et attributes btetghayar, esta5dem `Attrs` elly asameeha list sabta.",
        ],
        explain: [
            "Why: a hole that names a tag could produce `<script>`, and one that names an attribute could produce `onclick`. Neither can be made safe by escaping the value (spec §17.4).\n\nInstead of:\n    html\"<{ tag }>{ text }</{ tag }>\"\n\nwrite:\n    match level:\n        case .Title: html\"<h1>{ text }</h1>\"\n        case .Body: html\"<p>{ text }</p>\"",
            "السبب: الفجوة التي تسمّي وسمًا قد تنتج `<script>`، والتي تسمّي سمة قد تنتج `onclick`. ولا يمكن جعل أي منهما آمنًا بتهريب القيمة (المواصفة §17.4).\n\nبدلًا من:\n    html\"<{ الوسم }>{ النص }</{ الوسم }>\"\n\nاكتب:\n    حسب المستوى:\n        عندما .عنوان: html\"<h1>{ النص }</h1>\"\n        عندما .فقرة: html\"<p>{ النص }</p>\"",
            "Leh: el hole elly betsammy tag momken tetla3 `<script>`, w elly betsammy attribute momken tetla3 `onclick`. W mafeesh wa7da menhom te2dar tetkhalla amna b escape lel qeema (el spec §17.4).\n\nBadal:\n    html\"<{ tag }>{ text }</{ tag }>\"\n\nEkteb:\n    hasab level:\n        lama .Title: html\"<h1>{ text }</h1>\"\n        lama .Body: html\"<p>{ text }</p>\"",
        ],
        proposed: false,
    },
    Entry {
        code: "K1615",
        title: ["HOLE IN AN HTML COMMENT", "فجوة في تعليق HTML", "HOLE F COMMENT HTML"],
        message: [
            "This hole is inside an HTML comment. Comments are sent to the browser, and a value containing `-->` would end the comment and become markup.",
            "هذه الفجوة داخل تعليق HTML. والتعليقات تُرسل إلى المتصفح، والقيمة التي فيها `-->` كانت ستنهي التعليق وتصبح وسومًا.",
            "El hole di gowa comment HTML. W el comments betetba3at lel browser, w qeema feeha `-->` kanet hatkhallas el comment w teb2a markup.",
        ],
        help: [
            "Move the value out of the comment, or use a Kalam `#` comment, which is never sent.",
            "انقل القيمة خارج التعليق، أو استخدم تعليق كلام `#` الذي لا يُرسل أبدًا.",
            "Talla3 el qeema bara el comment, aw esta5dem comment Kalam `#` elly 3omro ma byetba3at.",
        ],
        explain: [
            "Why: there is no safe way to escape a value inside an HTML comment, and a comment that echoes user data also leaks it into the page source (spec §17.4).\n\nInstead of:\n    html\"<!-- user { user.id } --><p>{ text }</p>\"\n\nwrite:\n    html\"<p>{ text }</p>\"",
            "السبب: لا توجد طريقة آمنة لتهريب قيمة داخل تعليق HTML، والتعليق الذي يكرر بيانات المستخدم يسرّبها أيضًا إلى مصدر الصفحة (المواصفة §17.4).\n\nبدلًا من:\n    html\"<!-- user { المستخدم.الرقم } --><p>{ النص }</p>\"\n\nاكتب:\n    html\"<p>{ النص }</p>\"",
            "Leh: mafeesh taree2a amna te3mel escape l qeema gowa comment HTML, wel comment elly beyraded data el user bysarrabha kaman f source el page (el spec §17.4).\n\nBadal:\n    html\"<!-- user { user.id } --><p>{ text }</p>\"\n\nEkteb:\n    html\"<p>{ text }</p>\"",
        ],
        proposed: false,
    },
    Entry {
        code: "K1616",
        title: ["EVENT HOLE NEEDS A MESSAGE", "فجوة الحدث تحتاج رسالة", "HOLE EL EVENT ME7TAGA MESSAGE"],
        message: [
            "This `k-click`, `k-submit`, `k-input` or `k-change` hole must hold a value whose type derives `Message`. Only those values can be sent to the browser as a signed token.",
            "فجوة `k-click` أو `k-submit` أو `k-input` أو `k-change` هذه يجب أن تحمل قيمة يشتق نوعها `Message`. فهذه القيم وحدها يمكن إرسالها إلى المتصفح كرمز موقَّع.",
            "El hole bta3et `k-click` aw `k-submit` aw `k-input` aw `k-change` di lazem tsheel qeema no3aha bey3mel derive l `Message`. El qeyam di bas elly momken tetba3at lel browser ka token mewa22a3.",
        ],
        help: [
            "Use a case of your message enum, for example `k-click=\"{ Msg.Save }\"`, and add `@derive(Message)` to that enum.",
            "استخدم حالة من تعداد رسائلك، مثل `k-click=\"{ Msg.Save }\"`، وأضف `@derive(Message)` إلى هذا التعداد.",
            "Esta5dem 7ala men el enum bta3 el messages, zay `k-click=\"{ Msg.Save }\"`, w zawed `@derive(Message)` lel enum da.",
        ],
        explain: [
            "Why: an event in a live component comes back from the browser. Encoding it as a signed, opaque message means a user can only send messages your `update` already handles, never forge new ones (spec §20.3).\n\nInstead of:\n    html\"<button k-click=\\\"{ item.id }\\\">Remove</button>\"\n\nwrite:\n    html\"<button k-click=\\\"{ Msg.Remove(item.id) }\\\">Remove</button>\"",
            "السبب: الحدث في المكوّن الحي يعود من المتصفح. وترميزه كرسالة موقّعة ومغلقة يعني أن المستخدم لا يستطيع إلا إرسال رسائل يعالجها `update` لديك، ولا يستطيع أبدًا تزوير رسائل جديدة (المواصفة §20.3).\n\nبدلًا من:\n    html\"<button k-click=\\\"{ البند.الرقم }\\\">احذف</button>\"\n\nاكتب:\n    html\"<button k-click=\\\"{ رسالة.احذف(البند.الرقم) }\\\">احذف</button>\"",
            "Leh: el event f el live component byerga3 men el browser. W lamma yet3amallo encode ka message mewa22a3a w ma2foola, el user mayet2darsh yeba3at ella messages el `update` bta3ak bey3alegha aslan, w 3omro ma ye2dar yezawar messages gedeeda (el spec §20.3).\n\nBadal:\n    html\"<button k-click=\\\"{ item.id }\\\">Remove</button>\"\n\nEkteb:\n    html\"<button k-click=\\\"{ Msg.Remove(item.id) }\\\">Remove</button>\"",
        ],
        proposed: false,
    },
    Entry {
        code: "K1617",
        title: ["UNKNOWN k-* ATTRIBUTE", "سمة k-* غير معروفة", "ATTRIBUTE k-* MESH MA3ROOF"],
        message: [
            "I don't know this `k-` attribute. The `k-` prefix is reserved for Kalam's own directives, so a misspelled one would otherwise do nothing at all.",
            "لا أعرف سمة `k-` هذه. البادئة `k-` محجوزة لتوجيهات كلام نفسها، والتوجيه المكتوب خطأً كان لن يفعل شيئًا أبدًا.",
            "Mesh 3aref el attribute `k-` da. El bedaya `k-` ma7gooza lel directives bta3et Kalam, w el directive el maktooba ghalat kanet mesh hate3mel ay 7aga.",
        ],
        help: [
            "Check the spelling against the known directives: `k-click`, `k-submit`, `k-input`, `k-change`, `k-get`, `k-post`, `k-target` and `k-swap`.",
            "راجع الكتابة مقابل التوجيهات المعروفة: `k-click` و`k-submit` و`k-input` و`k-change` و`k-get` و`k-post` و`k-target` و`k-swap`.",
            "Raga3 el ketaba 3ala el directives el ma3roofa: `k-click`, `k-submit`, `k-input`, `k-change`, `k-get`, `k-post`, `k-target` w `k-swap`.",
        ],
        explain: [
            "Why: the browser ignores attributes it does not know, so `k-clik` would leave a button that silently does nothing. Reserving the prefix lets me catch the typo at compile time (spec §20.3).\n\nInstead of:\n    html\"<button k-clik=\\\"{ Msg.Save }\\\">Save</button>\"\n\nwrite:\n    html\"<button k-click=\\\"{ Msg.Save }\\\">Save</button>\"",
            "السبب: يتجاهل المتصفح السمات التي لا يعرفها، فكانت `k-clik` ستترك زرًا لا يفعل شيئًا دون أي تنبيه. وحجز البادئة يتيح لي اكتشاف الخطأ الإملائي وقت الترجمة (المواصفة §20.3).\n\nبدلًا من:\n    html\"<button k-clik=\\\"{ رسالة.احفظ }\\\">احفظ</button>\"\n\nاكتب:\n    html\"<button k-click=\\\"{ رسالة.احفظ }\\\">احفظ</button>\"",
            "Leh: el browser byetgahel el attributes elly mesh 3arefha, fa `k-clik` kanet hatseeb zorar mesh bey3mel 7aga men gher ay tanbeeh. W 7agz el bedaya byekhallini amsek el ghalta el emla2eya w2t el compile (el spec §20.3).\n\nBadal:\n    html\"<button k-clik=\\\"{ Msg.Save }\\\">Save</button>\"\n\nEkteb:\n    html\"<button k-click=\\\"{ Msg.Save }\\\">Save</button>\"",
        ],
        proposed: false,
    },
    Entry {
        code: "K1618",
        title: ["VIEW MESSAGE TYPE DIFFERS FROM UPDATE", "نوع رسالة العرض يختلف عن update", "NO3 MESSAGE EL VIEW MESH ZAY UPDATE"],
        message: [
            "This live view sends a message type that is not the one its `update` function handles, so these events would arrive at a function that can't read them.",
            "يرسل هذا العرض الحي نوع رسالة غير الذي تعالجه دالة `update` الخاصة به، فكانت هذه الأحداث ستصل إلى دالة لا تستطيع قراءتها.",
            "El live view da bayba3at no3 message mesh howa elly el `update` bta3to bey3alego, fa el events di kanet hatewsal l dala mesh 3arfa te2raha.",
        ],
        help: [
            "Use the cases of the message enum that `update` takes in every `k-*` hole of the view.",
            "استخدم حالات تعداد الرسائل الذي تأخذه `update` في كل فجوة `k-*` في العرض.",
            "Esta5dem 7alat el enum bta3 el messages elly `update` betakhdo f kol hole `k-*` f el view.",
        ],
        explain: [
            "Why: the component is registered with its `state`, `update` and `view` together. Every event the view can send must be a message `update` understands, or a click would reach code that has no case for it (spec §20.3).\n\nInstead of:\n    # update takes CartMsg\n    html\"<button k-click=\\\"{ MenuMsg.Open }\\\">Open</button>\"\n\nwrite:\n    html\"<button k-click=\\\"{ CartMsg.Open }\\\">Open</button>\"",
            "السبب: يُسجَّل المكوّن مع `state` و`update` و`view` معًا. وكل حدث يمكن للعرض إرساله يجب أن يكون رسالة تفهمها `update`، وإلا وصلت نقرة إلى كود ليس فيه حالة لها (المواصفة §20.3).\n\nبدلًا من:\n    # update تأخذ رسالة_السلة\n    html\"<button k-click=\\\"{ رسالة_القائمة.افتح }\\\">افتح</button>\"\n\nاكتب:\n    html\"<button k-click=\\\"{ رسالة_السلة.افتح }\\\">افتح</button>\"",
            "Leh: el component byetsaggel ma3 `state` w `update` w `view` ma3 ba3d. W kol event el view te2dar teba3ato lazem yeb2a message `update` fahmaha, walla click hatewsal l code mafeehoosh 7ala leeha (el spec §20.3).\n\nBadal:\n    # update betakhod CartMsg\n    html\"<button k-click=\\\"{ MenuMsg.Open }\\\">Open</button>\"\n\nEkteb:\n    html\"<button k-click=\\\"{ CartMsg.Open }\\\">Open</button>\"",
        ],
        proposed: false,
    },
    Entry {
        code: "K1620",
        title: ["SHELL CHARACTER IN A COMMAND", "حرف طرفية في أمر", "7ARF SHELL F COMMAND"],
        message: [
            "This `cmd\"...\"` literal contains a shell character such as `|`, `;`, `&`, `>` or `$(` outside quotes. Kalam never runs a shell, so the character would reach the program as plain text.",
            "هذه القيمة الحرفية `cmd\"...\"` فيها حرف طرفية مثل `|` أو `;` أو `&` أو `>` أو `$(` خارج علامات الاقتباس. ولا يشغّل كلام طرفية أبدًا، فكان الحرف سيصل إلى البرنامج كنص عادي.",
            "El literal `cmd\"...\"` da feeh 7arf shell zay `|` aw `;` aw `&` aw `>` aw `$(` bara el quotes. W Kalam 3omro ma beyshaghal shell, fa el 7arf kan hayewsal lel program ka nass 3adi.",
        ],
        help: [
            "Use `proc.pipe([...])` for pipelines, and the `stdin:` and `stdout:` options for files.",
            "استخدم `proc.pipe([...])` لسلاسل الأوامر، والخيارين `stdin:` و`stdout:` للملفات.",
            "Esta5dem `proc.pipe([...])` lel pipelines, wel options `stdin:` w `stdout:` lel files.",
        ],
        explain: [
            "Why: a command runs one program with a list of arguments, so values in holes can never become shell syntax. A pipe or a redirect written in the literal would silently become an argument instead of doing what it looks like (spec §17.5).\n\nInstead of:\n    cmd\"git log | head -n 5\"\n\nwrite:\n    proc.pipe([cmd\"git log\", cmd\"head -n 5\"])",
            "السبب: الأمر يشغّل برنامجًا واحدًا مع قائمة معاملات، فلا يمكن لقيم الفجوات أن تصبح صياغة طرفية أبدًا. والأنبوب أو إعادة التوجيه المكتوب في القيمة الحرفية كان سيصبح معاملًا بصمت بدل أن يفعل ما يبدو أنه يفعله (المواصفة §17.5).\n\nبدلًا من:\n    cmd\"git log | head -n 5\"\n\nاكتب:\n    proc.pipe([cmd\"git log\"، cmd\"head -n 5\"])",
            "Leh: el command beyshaghal program wa7ed ma3 list arguments, fa qeyam el holes 3omraha ma teb2a syntax shell. W el pipe aw el redirect el maktoob f el literal kan hayeb2a argument f el sekket badal ma ye3mel elly shaklo bey3melo (el spec §17.5).\n\nBadal:\n    cmd\"git log | head -n 5\"\n\nEkteb:\n    proc.pipe([cmd\"git log\", cmd\"head -n 5\"])",
        ],
        proposed: false,
    },
    Entry {
        code: "K1621",
        title: ["COMMAND PROGRAM IS NOT LITERAL", "برنامج الأمر ليس حرفيًا", "PROGRAM EL COMMAND MESH LITERAL"],
        message: [
            "The first word of this `cmd\"...\"` literal, the program to run, comes from a hole. The program must be written in the literal, so a value can never choose what runs.",
            "الكلمة الأولى في القيمة الحرفية `cmd\"...\"` هذه، أي البرنامج الذي سيعمل، تأتي من فجوة. ويجب أن يُكتب البرنامج في القيمة الحرفية، حتى لا تختار قيمةٌ ما الذي يعمل أبدًا.",
            "Awel kelma f el literal `cmd\"...\"` da, ya3ny el program elly hayeshtaghal, gaya men hole. W el program lazem yetketeb f el literal, 3ashan 3omr ma qeema tekhtar eh elly yeshtaghal.",
        ],
        help: [
            "Write the program name literally. To choose between a few programs, use `{kw_match}` with one `cmd\"...\"` per case.",
            "اكتب اسم البرنامج حرفيًا. وللاختيار بين بضعة برامج، استخدم `{kw_match}` مع `cmd\"...\"` لكل حالة.",
            "Ekteb esm el program zay ma howa. W 3ashan tekhtar ben kaza program, esta5dem `{kw_match}` ma3 `cmd\"...\"` l kol 7ala.",
        ],
        explain: [
            "Why: if a value can pick the program, an attacker who controls the value can run anything the process may run. With a literal program, a reviewer sees every program the code can start, and `Proc.only` can restrict it further (spec §17.5).\n\nInstead of:\n    cmd\"{ tool } --version\"\n\nwrite:\n    match tool:\n        case .Git: cmd\"git --version\"\n        case .Node: cmd\"node --version\"",
            "السبب: لو استطاعت قيمة أن تختار البرنامج لاستطاع مهاجم يتحكم في القيمة أن يشغّل أي شيء مسموح للعملية. ومع برنامج حرفي يرى المراجع كل برنامج يمكن للكود تشغيله، ويمكن لـ `Proc.only` تقييده أكثر (المواصفة §17.5).\n\nبدلًا من:\n    cmd\"{ الأداة } --version\"\n\nاكتب:\n    حسب الأداة:\n        عندما .جيت: cmd\"git --version\"\n        عندما .نود: cmd\"node --version\"",
            "Leh: law qeema te2dar tekhtar el program, el mohagem elly mesayter 3ala el qeema ye2dar yeshaghal ay 7aga el process masmoo7laha. Ma3 program literal, el reviewer byshoof kol program el code ye2dar yeshaghalo, w `Proc.only` te2dar t2ayyedo aktar (el spec §17.5).\n\nBadal:\n    cmd\"{ tool } --version\"\n\nEkteb:\n    hasab tool:\n        lama .Git: cmd\"git --version\"\n        lama .Node: cmd\"node --version\"",
        ],
        proposed: false,
    },
    Entry {
        code: "W1622",
        title: ["OPTION-LIKE ARGUMENT WITHOUT --", "معامل يشبه خيارًا دون --", "ARGUMENT SHABAH OPTION MEN GHER --"],
        message: [
            "This hole is a whole argument with no `--` before it. If its value starts with `-`, the program would read it as an option, so the run stops with an error instead.",
            "هذه الفجوة معامل كامل ليس قبله `--`. فإن بدأت قيمتها بـ `-` كان البرنامج سيقرؤها كخيار، لذلك يتوقف التشغيل بخطأ بدلًا من ذلك.",
            "El hole di argument kamel mafeesh 2ablo `--`. Law qeemetha bada2et b `-`, el program kan haye2raha ka option, 3ashan keda el tashgheel beyo2af b error badal keda.",
        ],
        help: [
            "Put `--` before the first hole that is a file or a name, for example `cmd\"git log -- { file }\"`.",
            "ضع `--` قبل أول فجوة تمثل ملفًا أو اسمًا، مثل `cmd\"git log -- { file }\"`.",
            "7ott `--` 2abl awel hole betmassel file aw esm, zay `cmd\"git log -- { file }\"`.",
        ],
        explain: [
            "Why: a file named `--upload-pack=evil` passed to `git` is an option, not a file. `--` tells the program that everything after it is an argument. Without it, `proc.run` refuses a value starting with `-`, which is safe but fails at run time; this warning moves the problem to compile time (spec §17.5).\n\nInstead of:\n    cmd\"git log { file }\"\n\nwrite:\n    cmd\"git log -- { file }\"",
            "السبب: ملف اسمه `--upload-pack=evil` يُمرَّر إلى `git` يصبح خيارًا لا ملفًا. و`--` تخبر البرنامج أن كل ما بعدها معاملات. وبدونها ترفض `proc.run` القيمة التي تبدأ بـ `-`، وهذا آمن لكنه يفشل وقت التشغيل، وهذا التحذير ينقل المشكلة إلى وقت الترجمة (المواصفة §17.5).\n\nبدلًا من:\n    cmd\"git log { الملف }\"\n\nاكتب:\n    cmd\"git log -- { الملف }\"",
            "Leh: file esmo `--upload-pack=evil` byetba3at l `git` byeb2a option mesh file. W `--` bet2ool lel program en kol elly ba3daha arguments. Men gherha, `proc.run` bterfod el qeema elly btebda2 b `-`, w da amn bas byo2a3 w2t el tashgheel; wel warning da byenqel el moshkela l w2t el compile (el spec §17.5).\n\nBadal:\n    cmd\"git log { file }\"\n\nEkteb:\n    cmd\"git log -- { file }\"",
        ],
        proposed: false,
    },
    Entry {
        code: "K1631",
        title: ["URL SCHEME OR HOST FROM A HOLE", "مخطط الرابط أو مضيفه من فجوة", "SCHEME AW HOST EL URL MEN HOLE"],
        message: [
            "In this `url\"...\"` literal, the scheme or the host comes from a hole. Both must be written in the literal, so a value can never send a request to another server.",
            "في القيمة الحرفية `url\"...\"` هذه يأتي المخطط أو المضيف من فجوة. ويجب أن يُكتب الاثنان في القيمة الحرفية، حتى لا تستطيع قيمةٌ أبدًا إرسال طلب إلى خادم آخر.",
            "F el literal `url\"...\"` da, el scheme aw el host gay men hole. W el etnein lazem yetketbo f el literal, 3ashan 3omr ma qeema teb3at request l server tany.",
        ],
        help: [
            "Write the scheme and host literally and keep holes for the path and query. For a URL that really comes from input, use `Url.parse(s)`.",
            "اكتب المخطط والمضيف حرفيًا واترك الفجوات للمسار والاستعلام. وللرابط الذي يأتي فعلًا من المدخلات، استخدم `Url.parse(s)`.",
            "Ekteb el scheme wel host zay ma homa w khalli el holes lel path wel query. W lel URL elly fe3lan gay men el input, esta5dem `Url.parse(s)`.",
        ],
        explain: [
            "Why: a host taken from input is how server-side request forgery (SSRF) starts: the server is tricked into calling an internal address. Holes in the path and query are percent-encoded and safe (spec §17.6).\n\nInstead of:\n    url\"https://{ host }/users/{ id }\"\n\nwrite:\n    url\"https://api.example.com/users/{ id }\"",
            "السبب: المضيف المأخوذ من المدخلات هو بداية تزوير الطلبات من جهة الخادم (SSRF)، حيث يُخدع الخادم ليستدعي عنوانًا داخليًا. أما الفجوات في المسار والاستعلام فتُرمَّز بالنسبة المئوية وهي آمنة (المواصفة §17.6).\n\nبدلًا من:\n    url\"https://{ المضيف }/users/{ الرقم }\"\n\nاكتب:\n    url\"https://api.example.com/users/{ الرقم }\"",
            "Leh: el host elly gay men el input howa bedayet el SSRF: el server byetde7ek 3aleih w ynady 3enwan dakhly. Laken el holes f el path wel query btet3amallaha percent-encoding w amna (el spec §17.6).\n\nBadal:\n    url\"https://{ host }/users/{ id }\"\n\nEkteb:\n    url\"https://api.example.com/users/{ id }\"",
        ],
        proposed: false,
    },
    Entry {
        code: "K1641",
        title: ["PATH HOLE NEEDS A PATHSEGMENT", "فجوة المسار تحتاج PathSegment", "HOLE EL PATH ME7TAGA PATHSEGMENT"],
        message: [
            "This `path\"...\"` hole holds a `{core_string}`, but a path hole must be a `PathSegment`. A string could contain `..` or `/` and escape the folder you meant.",
            "هذه الفجوة في `path\"...\"` تحمل `{core_string}`، لكن فجوة المسار يجب أن تكون `PathSegment`. فالنص قد يحتوي `..` أو `/` ويخرج من المجلد الذي قصدته.",
            "El hole di f `path\"...\"` shayla `{core_string}`, bas hole el path lazem teb2a `PathSegment`. El string momken yeb2a feeh `..` aw `/` w yetla3 bara el folder elly 2asdak 3aleih.",
        ],
        help: [
            "Check the text first: `PathSegment.parse(name){punct_try}`. `kalam fix` inserts it.",
            "تحقق من النص أولًا: `PathSegment.parse(name){punct_try}`. والأمر `kalam fix` يضيفها.",
            "Check el nass el awel: `PathSegment.parse(name){punct_try}`. W `kalam fix` byzawedha.",
        ],
        explain: [
            "Why: `uploads/{ name }` with a name of `../../etc/passwd` leaves the uploads folder. `PathSegment.parse` rejects empty names, `.`, `..`, slashes, NUL, drive letters and reserved Windows names, so every hole stays one level inside the folder (spec §17.6).\n\nInstead of:\n    let p = path\"uploads/{ name }\"\n\nwrite:\n    let seg = PathSegment.parse(name)?\n    let p = path\"uploads/{ seg }\"",
            "السبب: المسار `uploads/{ الاسم }` مع اسم مثل `../../etc/passwd` يخرج من مجلد الرفع. و`PathSegment.parse` ترفض الأسماء الفارغة و`.` و`..` والشرطات المائلة وNUL وأحرف الأقراص وأسماء Windows المحجوزة، فتبقى كل فجوة مستوى واحدًا داخل المجلد (المواصفة §17.6).\n\nبدلًا من:\n    ليكن م = path\"uploads/{ الاسم }\"\n\nاكتب:\n    ليكن الجزء = PathSegment.parse(الاسم)؟\n    ليكن م = path\"uploads/{ الجزء }\"",
            "Leh: `uploads/{ name }` b esm zay `../../etc/passwd` byetla3 bara folder el uploads. W `PathSegment.parse` bterfod el asamy el fadya w `.` w `..` wel slashes w NUL w 7oroof el drives w asamy Windows el ma7gooza, fa kol hole btefdal mostawa wa7ed gowa el folder (el spec §17.6).\n\nBadal:\n    khali p = path\"uploads/{ name }\"\n\nEkteb:\n    khali seg = PathSegment.parse(name)?\n    khali p = path\"uploads/{ seg }\"",
        ],
        proposed: false,
    },
    Entry {
        code: "K1650",
        title: ["TWO TAGS WITH ONE NAME", "وسمان باسم واحد", "ETNEIN TAGS B ESM WA7ED"],
        message: [
            "Two literal tags with this name are in scope here, from two imports, so I can't tell which one this literal uses.",
            "يوجد هنا وسمان حرفيان بهذا الاسم من استيرادين مختلفين، فلا أستطيع معرفة أيهما تستخدمه هذه القيمة الحرفية.",
            "Fe hena etnein tags literal b el esm da men etnein imports, fa mesh 3aref el literal da besta5dem anhy wa7ed.",
        ],
        help: [
            "Import only one of the two modules that define the tag in this file, or move the code that needs the other tag to its own module.",
            "استورد في هذا الملف واحدة فقط من الوحدتين اللتين تعرّفان الوسم، أو انقل الكود الذي يحتاج الوسم الآخر إلى وحدة خاصة به.",
            "E3mel import l module wa7da bas men el etnein elly bey3arrafo el tag f el file da, aw na2al el code elly me7tag el tag el tany l module lewa7do.",
        ],
        explain: [
            "Why: a tag such as `sql` decides how every hole in its literal is escaped. If two tags shared a name, the escaping of a query could depend on import order, which nobody reviewing the line would notice (spec §17.1).\n\nInstead of importing two packages that both define `graphql` in one module, keep each one in its own module.",
            "السبب: الوسم مثل `sql` يحدد كيف تُهرَّب كل فجوة في قيمته الحرفية. ولو تشارك وسمان في اسم واحد لأمكن أن يعتمد تهريب استعلام على ترتيب الاستيراد، وهذا لن يلاحظه أي مراجع للسطر (المواصفة §17.1).\n\nبدل استيراد حزمتين تعرّف كلتاهما `graphql` في وحدة واحدة، اجعل كل واحدة في وحدة خاصة بها.",
            "Leh: el tag zay `sql` howa elly bey7added kol hole f el literal bta3o byet3amallaha escape ezay. W law etnein tags leehom nafs el esm, el escape bta3 query momken ye3tamed 3ala tarteeb el imports, w da mafeesh reviewer hayakhod balo meno (el spec §17.1).\n\nBadal ma te3mel import l etnein packages kol wa7da fehom bet3arraf `graphql` f module wa7da, khalli kol wa7da f module lewa7daha.",
        ],
        proposed: false,
    },
    // ---- Interop (K17xx, spec §19). ar/franco: pending native review.
    Entry {
        code: "K1701",
        title: ["NPM MODULE NOT DECLARED", "وحدة npm غير معلنة", "MODULE NPM MESH MET3ALLEN"],
        message: [
            "This `{kw_extern}` imports a JavaScript module that the package manifest doesn't list under `[npm]`.",
            "هذا الـ `{kw_extern}` يستورد وحدة JavaScript لا يذكرها ملف تعريف الحزمة تحت `[npm]`.",
            "El `{kw_extern}` da bey3mel import l module JavaScript el manifest bta3 el package mesh zakerha ta7t `[npm]`.",
        ],
        help: [
            "Add the module with a pinned version and its capabilities to `[npm]` in `kalam.toml` (`kalam add` does this and shows the capability diff).",
            "أضف الوحدة بإصدار محدد وصلاحياتها إلى `[npm]` في `kalam.toml` (والأمر `kalam add` يفعل ذلك ويعرض فرق الصلاحيات).",
            "Zawed el module b version mazboota w el salaheyat bta3etha f `[npm]` f `kalam.toml` (w `kalam add` bey3mel keda w by3red farq el salaheyat).",
        ],
        explain: [
            "Why: every piece of JavaScript a Kalam program loads must be visible in one place, with a fixed version and the capabilities it may use. That is what `kalam audit` and the capability diff read (spec §13.5, §19).\n\nIn kalam.toml:\n    [npm]\n    stripe = { version = \"17.5.0\", caps = [\"net:api.stripe.com\"] }",
            "السبب: كل جزء JavaScript يحمّله برنامج كلام يجب أن يكون ظاهرًا في مكان واحد، بإصدار ثابت والصلاحيات التي قد يستخدمها. وهذا ما يقرؤه `kalam audit` وفرق الصلاحيات (المواصفة §13.5، §19).\n\nفي kalam.toml:\n    [npm]\n    stripe = { version = \"17.5.0\", caps = [\"net:api.stripe.com\"] }",
            "Leh: kol 7etta JavaScript program Kalam bey7ammelha lazem teb2a bayna f makan wa7ed, b version sabta wel salaheyat elly momken testa5demha. W da elly `kalam audit` w farq el salaheyat bey2rooh (el spec §13.5, §19).\n\nF kalam.toml:\n    [npm]\n    stripe = { version = \"17.5.0\", caps = [\"net:api.stripe.com\"] }",
        ],
        proposed: false,
    },
    Entry {
        code: "K1702",
        title: ["CAPABILITY CROSSING INTO JAVASCRIPT", "صلاحية تعبر إلى JavaScript", "SALAHEYA BTE3DY L JAVASCRIPT"],
        message: [
            "A capability (such as `Net` or `Fs`) can't be passed into JavaScript. JavaScript code doesn't follow Kalam's rules, so it could use or store the capability without limits.",
            "لا يمكن تمرير صلاحية (مثل `Net` أو `Fs`) إلى JavaScript. فكود JavaScript لا يتبع قواعد كلام، وقد يستخدم الصلاحية أو يخزنها بلا حدود.",
            "Mayenfa3sh teba3at salaheya (zay `Net` aw `Fs`) l JavaScript. Code el JavaScript mesh mashy 3ala qawa3ed Kalam, w momken yesta5dem el salaheya aw yekhazzenha men gher 7edood.",
        ],
        help: [
            "Keep the capability as a parameter of the `{kw_extern}` function: it then works as a token that Kalam callers must hold, and the glue never passes it to JavaScript.",
            "أبقِ الصلاحية معاملًا في دالة `{kw_extern}`: فتعمل كرمز يجب أن يملكه مستدعو كلام، ولا يمررها كود الربط إلى JavaScript أبدًا.",
            "Khalli el salaheya parameter f dalet el `{kw_extern}`: keda btshtaghal ka token lazem elly bynady men Kalam ykoon me3ah, w el glue 3omro ma beyba3atha l JavaScript.",
        ],
        explain: [
            "Why: capabilities are how Kalam limits what code can reach. Inside JavaScript there is no such limit, so a capability that crossed the boundary would be a hole in the whole model. As a parameter of an `extern` declaration, it stays on the Kalam side and still documents what the call needs (spec §19.2).\n\nDeclare what the JavaScript side may reach in `[npm]` capabilities, and keep the capability as a token parameter:\n    @js(module: \"stripe\", name: \"charge\", async: true, throws: true)\n    extern fn charge(net: Net, req: ChargeRequest) -> Result[Charge, JsError]",
            "السبب: الصلاحيات هي طريقة كلام في تحديد ما يمكن للكود الوصول إليه. وداخل JavaScript لا يوجد هذا الحد، فكانت الصلاحية التي تعبر الحدود ستصبح ثغرة في النموذج كله. أما كمعامل في تعريف `extern` فتبقى في جانب كلام وتوثّق ما يحتاجه الاستدعاء (المواصفة §19.2).\n\nأعلن ما يصل إليه جانب JavaScript في صلاحيات `[npm]`، وأبقِ الصلاحية معاملًا رمزيًا:\n    @js(module: \"stripe\", name: \"charge\", async: true, throws: true)\n    extern دالة ادفع(net: Net، الطلب: ChargeRequest) -> نتيجة[Charge، JsError]",
            "Leh: el salaheyat heya taree2et Kalam f ta7deed el code ye2dar yewsal l eh. W gowa JavaScript mafeesh 7add zay da, fa el salaheya elly te3dy el 7edood kanet hateb2a soghra f el model kollo. Laken ka parameter f ta3reef `extern` btefdal f naheyet Kalam w betwassa2 el call me7tag eh (el spec §19.2).\n\nA3len 3an elly naheyet el JavaScript te2dar tewsalo f salaheyat `[npm]`, w khalli el salaheya parameter token:\n    @js(module: \"stripe\", name: \"charge\", async: true, throws: true)\n    extern dala edfa3(net: Net, req: ChargeRequest) -> natiga[Charge, JsError]",
        ],
        proposed: false,
    },
    // ---- Lints, warnings and hints from the lexer and names (W01xx, L01xx, H02xx, W04xx,
    // spec §5, §6, §11). ar/franco: pending native review.
    Entry {
        code: "W0114",
        title: ["UNUSUAL SPACE IN A STRING", "مسافة غير معتادة في نص", "MASAFA MESH 3ADEYA F STRING"],
        message: [
            "This string contains a space that is not the ordinary ASCII space (for example a no-break space, U+00A0). It looks the same but compares as different text.",
            "هذا النص فيه مسافة غير المسافة العادية في ASCII (مثل المسافة غير القابلة للكسر U+00A0). تبدو مثلها لكنها تُقارن كنص مختلف.",
            "El string da feeh masafa mesh el masafa el 3adeya bta3et ASCII (zay el no-break space U+00A0). Shaklaha zayaha bas fel moqarna btetle3 nass mokhtalef.",
        ],
        help: [
            "Replace it with an ordinary space, or write it as an escape such as `\\u{00A0}` if you really need it.",
            "استبدلها بمسافة عادية، أو اكتبها كتسلسل هروب مثل `\\u{00A0}` إن كنت تحتاجها فعلًا.",
            "Baddelha b masafa 3adeya, aw ektebha ka escape zay `\\u{00A0}` law fe3lan me7tagha.",
        ],
        explain: [
            "Why: text copied from documents and web pages often carries no-break or thin spaces. Inside a string they change comparisons, splits and lookups without anything visible. An escape makes the intent explicit (spec §5.1).\n\nInstead of a string containing an invisible U+00A0 between two words, write:\n    \"10\\u{00A0}km\"",
            "السبب: النصوص المنسوخة من المستندات وصفحات الويب كثيرًا ما تحمل مسافات غير قابلة للكسر أو رفيعة. وداخل النص تغيّر المقارنات والتقسيم والبحث دون أي شيء ظاهر. وتسلسل الهروب يجعل القصد صريحًا (المواصفة §5.1).\n\nبدل نص فيه U+00A0 خفية بين كلمتين، اكتب:\n    \"10\\u{00A0}km\"",
            "Leh: el kalam el mansookh men documents w safa7at web keteer beyeb2a feeh no-break aw thin spaces. W gowa el string betghayar el moqarnat wel ta2seem wel ba7s men gher ay 7aga bayna. W el escape bykhalli el 2asd wade7 (el spec §5.1).\n\nBadal string feeh U+00A0 mestakhabbeya ben kelmetein, ekteb:\n    \"10\\u{00A0}km\"",
        ],
        proposed: false,
    },
    Entry {
        code: "W0133",
        title: ["UNUSUAL CHARACTER IN A STRING", "حرف غير معتاد في نص", "7ARF MESH 3ADI F STRING"],
        message: [
            "This string contains an unusual code point, such as an invisible or look-alike character. Readers can't see it, but it changes what the text is.",
            "هذا النص فيه نقطة ترميز غير معتادة، مثل حرف خفي أو حرف شبيه بآخر. لا يراه القراء لكنه يغيّر حقيقة النص.",
            "El string da feeh code point mesh 3adi, zay 7arf mestakhabby aw 7arf shabah 7arf tany. El nas mesh shayfeeno bas howa byghayar el nass fe3lan.",
        ],
        help: [
            "Delete the character, or write it as an escape such as `\\u{200B}` so readers can see it is there.",
            "احذف الحرف، أو اكتبه كتسلسل هروب مثل `\\u{200B}` حتى يرى القراء أنه موجود.",
            "Emsa7 el 7arf, aw ekteboh ka escape zay `\\u{200B}` 3ashan el nas yshoofo enno mawgood.",
        ],
        explain: [
            "Why: a string is data your program compares and stores, and the reviewer reads it in the source. An invisible character in it means the reviewed text and the real text differ, which is how look-alike keys, names and URLs slip through (spec §5.7).\n\nInstead of a string with a hidden U+200B, write:\n    \"admin\"\n\nor, if it is intended:\n    \"admin\\u{200B}\"",
            "السبب: النص بيانات يقارنها برنامجك ويخزنها، ويقرؤها المراجع في الكود. والحرف الخفي فيه يعني أن النص المراجَع غير النص الحقيقي، وهكذا تمر المفاتيح والأسماء والروابط الشبيهة (المواصفة §5.7).\n\nبدل نص فيه U+200B خفية، اكتب:\n    \"admin\"\n\nأو إن كانت مقصودة:\n    \"admin\\u{200B}\"",
            "Leh: el string data el program bta3ak bey2aren-ha w yekhazzenha, wel reviewer bye2raha f el code. W el 7arf el mestakhabby feeha ma3nah en el nass elly etraga3 gher el nass el 7a2ee2y, w keda el keys wel asamy wel links el shabah ba3d bt3addy (el spec §5.7).\n\nBadal string feeh U+200B mestakhabbeya, ekteb:\n    \"admin\"\n\naw, law ma2soda:\n    \"admin\\u{200B}\"",
        ],
        proposed: false,
    },
    Entry {
        code: "L0114",
        title: ["MIXED AND/OR WITHOUT PARENTHESES", "خلط و/أو دون أقواس", "KHALT WE/AW MEN GHER AQWAS"],
        message: [
            "This condition mixes `{kw_and}` and `{kw_or}` without parentheses. In Arabic, `و` and `أو` differ by one letter, so the grouping is easy to misread.",
            "هذا الشرط يخلط `{kw_and}` و`{kw_or}` دون أقواس. وفي العربية يختلف `و` عن `أو` بحرف واحد، فيسهل أن يُقرأ التجميع خطأً.",
            "El shart da bykhallat `{kw_and}` w `{kw_or}` men gher aqwas. W bel 3araby `و` w `أو` el far2 benhom 7arf wa7ed, fa sahl el tagmee3 yet2ery ghalat.",
        ],
        help: [
            "Add parentheses to show the grouping you mean, for example `(a {kw_and} b) {kw_or} c`. `kalam fmt` keeps them.",
            "أضف أقواسًا لتبيّن التجميع الذي تقصده، مثل `(a {kw_and} b) {kw_or} c`. والأمر `kalam fmt` يُبقي عليها.",
            "Zawed aqwas t-warry el tagmee3 elly 2asdak 3aleih, zay `(a {kw_and} b) {kw_or} c`. W `kalam fmt` byseebhom.",
        ],
        explain: [
            "Why: `and` binds tighter than `or`, as in most languages, but the Arabic keywords `و` and `أو` are one letter apart, so a reader can easily take one for the other. Parentheses make the meaning visible in every skin (spec §7.4, exception E1).\n\nInstead of:\n    if admin or owner and active:\n\nwrite:\n    if admin or (owner and active):",
            "السبب: يرتبط `و` أقوى من `أو` كما في معظم اللغات، لكن الكلمتين `و` و`أو` يفصلهما حرف واحد، فيسهل أن يحسب القارئ إحداهما الأخرى. والأقواس تجعل المعنى ظاهرًا في كل واجهة (المواصفة §7.4، الاستثناء E1).\n\nبدلًا من:\n    إذا مدير أو مالك و نشط:\n\nاكتب:\n    إذا مدير أو (مالك و نشط):",
            "Leh: `we` btertebet aqwa men `aw` zay mo3zam el loghat, bas el kelmetein el 3arabi `و` w `أو` el far2 benhom 7arf wa7ed, fa sahl el 2ari ye7seb wa7da el tanya. W el aqwas betkhalli el ma3na bayen f kol skin (el spec §7.4, el estesna2 E1).\n\nBadal:\n    law admin aw owner we active:\n\nEkteb:\n    law admin aw (owner we active):",
        ],
        proposed: false,
    },
    Entry {
        code: "H0202",
        title: ["NAME IS A KEYWORD IN ANOTHER SKIN", "الاسم كلمة محجوزة في واجهة أخرى", "EL ESM KEYWORD F SKIN TANY"],
        message: [
            "This name is a reserved word in another skin. It works here, but when the file is shown in that skin, the name will appear in backticks so it can't be read as the keyword.",
            "هذا الاسم كلمة محجوزة في واجهة أخرى. وهو يعمل هنا، لكن حين يُعرض الملف بتلك الواجهة سيظهر الاسم بين علامتي `` ` `` حتى لا يُقرأ ككلمة محجوزة.",
            "El esm da keyword ma7gooza f skin tany. Howa shaghal hena, bas lamma el file yetshaf bel skin da, el esm hayezhar ben backticks 3ashan mayet2resh ka keyword.",
        ],
        help: [
            "Nothing needs to change. To avoid the backticks, pick a name that is not a keyword in any skin.",
            "لا يلزم أي تغيير. ولتجنّب علامتي الاقتباس الخلفيتين، اختر اسمًا ليس كلمة محجوزة في أي واجهة.",
            "Mafeesh 7aga lazem tetghayar. W 3ashan teb3ed 3an el backticks, ekhtar esm mesh keyword f ay skin.",
        ],
        explain: [
            "Why: every Kalam file can be shown in all three skins. A name such as `law` is fine in an English file, but `law` means `if` in Franco, so the Franco view writes it as `` `law` ``. This hint tells you that ahead of time; it is never an error (spec §5.4).\n\nFor example, in an English file:\n    let law = \"contract\"\n\nis shown in Franco as:\n    khali `law` = \"contract\"",
            "السبب: كل ملف كلام يمكن عرضه بالواجهات الثلاث. والاسم مثل `law` لا بأس به في ملف إنجليزي، لكن `law` تعني `إذا` في الفرانكو، فيكتبه عرض الفرانكو بين علامتين `` ` ``. وهذا التلميح يخبرك بذلك مسبقًا، وليس خطأً أبدًا (المواصفة §5.4).\n\nمثلًا في ملف إنجليزي:\n    let law = \"contract\"\n\nيُعرض بالفرانكو هكذا:\n    khali `law` = \"contract\"",
            "Leh: kol file Kalam momken yetshaf bel talat skins. W esm zay `law` mafeesh moshkela feeh f file English, bas `law` ma3naha `if` bel Franco, fa el Franco view btektebo `` `law` ``. W el hint da by2ollak keda men badry; w 3omro ma byeb2a error (el spec §5.4).\n\nMasalan f file English:\n    let law = \"contract\"\n\nbyetshaf bel Franco keda:\n    khali `law` = \"contract\"",
        ],
        proposed: false,
    },
    Entry {
        code: "W0403",
        title: ["CATCH-ALL OVER ANOTHER PACKAGE'S ENUM", "فرع عام على تعداد من حزمة أخرى", "FAR3 3AM 3ALA ENUM MEN PACKAGE TANYA"],
        message: [
            "This `_` arm matches an enum from another package. When that package adds a variant, it will silently go to this arm instead of being reported.",
            "هذا الفرع `_` يطابق تعدادًا من حزمة أخرى. فحين تضيف تلك الحزمة عضوًا جديدًا سيذهب بصمت إلى هذا الفرع بدل أن يُبلَّغ عنه.",
            "El far3 `_` da byetabe2 enum men package tanya. Lamma el package di tzawed variant, hayerou7 lel far3 da f el sekket badal ma yetballagh 3anno.",
        ],
        help: [
            "List the variants you handle instead of `_`, or add `@allow(W0403)` to the item if the catch-all is deliberate (`kalam audit` counts it).",
            "اذكر الأعضاء التي تعالجها بدل `_`، أو أضف `@allow(W0403)` إلى العنصر إن كان الفرع العام مقصودًا (والأمر `kalam audit` يعدّه).",
            "Ekteb el variants elly bet3alegha badal `_`, aw zawed `@allow(W0403)` lel 7aga law el far3 el 3am ma2sood (w `kalam audit` bey3edo).",
        ],
        explain: [
            "Why: for your own enums, the compiler shows every `match` to update when you add a variant. For another package's enum, a `_` arm hides the new variant after an upgrade, which matters most for roles and states. An enum marked `@exhaustive_strict` makes this an error (spec §11).\n\nInstead of:\n    match role:\n        case .Admin: allow()\n        case _: deny()\n\nwrite:\n    match role:\n        case .Admin: allow()\n        case .Member: deny()\n        case .Guest: deny()",
            "السبب: في تعداداتك أنت يعرض المترجم كل `حسب` يجب تحديثها حين تضيف عضوًا. أما في تعداد حزمة أخرى فالفرع `_` يخفي العضو الجديد بعد التحديث، وهذا أهم ما يكون في الأدوار والحالات. والتعداد المعلَّم بـ `@exhaustive_strict` يجعل هذا خطأً (المواصفة §11).\n\nبدلًا من:\n    حسب الدور:\n        عندما .مدير: اسمح()\n        عندما _: ارفض()\n\nاكتب:\n    حسب الدور:\n        عندما .مدير: اسمح()\n        عندما .عضو: ارفض()\n        عندما .زائر: ارفض()",
            "Leh: f el enums bta3tak el compiler bywarrik kol `hasab` lazem tet7addes lamma tzawed variant. Laken f enum men package tanya, el far3 `_` bykhabby el variant el gedeed ba3d el update, w da ahamm 7aga f el roles wel states. W el enum elly 3aleih `@exhaustive_strict` bykhalli da error (el spec §11).\n\nBadal:\n    hasab role:\n        lama .Admin: esma7()\n        lama _: erfod()\n\nEkteb:\n    hasab role:\n        lama .Admin: esma7()\n        lama .Member: erfod()\n        lama .Guest: erfod()",
        ],
        proposed: false,
    },
    Entry {
        code: "W0710",
        title: ["REQUEST DECODED INTO A ROW TYPE", "طلب يُفك ترميزه إلى نوع صف", "REQUEST BYET3AMALLO DECODE L ROW TYPE"],
        message: [
            "Request data is decoded straight into a type that also derives `Row`, a database row. A client could then set any field of the row, such as `role` or `owner_id`.",
            "تُفك بيانات الطلب مباشرة إلى نوع يشتق `Row` أيضًا، أي صف في قاعدة البيانات. فيستطيع العميل حينها ضبط أي حقل في الصف، مثل `role` أو `owner_id`.",
            "Data el request btet3amallaha decode 3ala tool l type bey3mel derive l `Row` kaman, ya3ny row f el database. Fa el client ye2dar yezbot ay field f el row, zay `role` aw `owner_id`.",
        ],
        help: [
            "Decode into a separate input type with only the fields a client may set, then build the row from it.",
            "فك الترميز إلى نوع مدخلات منفصل فيه فقط الحقول التي يحق للعميل ضبطها، ثم ابنِ الصف منه.",
            "E3mel decode l type input monfasel feeh bas el fields elly el client mesmoo7lo yezbotha, w ba3dein ebny el row menno.",
        ],
        explain: [
            "Why: this is the mass-assignment bug: a form that should change a title can also change `is_admin` if the request is bound to the whole row. A separate input type lists exactly what a request may touch (spec §18.3).\n\nInstead of:\n    let note: Note = req.json()?\n\nwrite:\n    @derive(Decode)\n    type NewNote:\n        title: String\n    let input: NewNote = req.json()?\n    let note = Note(id: next_id, title: input.title, owner: user.id)",
            "السبب: هذا خطأ الإسناد الجماعي: النموذج الذي يجب أن يغيّر عنوانًا يستطيع أيضًا تغيير `is_admin` إذا رُبط الطلب بالصف كله. ونوع المدخلات المنفصل يحدد بالضبط ما يحق للطلب أن يمسّه (المواصفة §18.3).\n\nبدلًا من:\n    ليكن الملاحظة: ملاحظة = الطلب.json()؟\n\nاكتب:\n    @derive(Decode)\n    هيكل ملاحظة_جديدة:\n        العنوان: نص\n    ليكن المدخل: ملاحظة_جديدة = الطلب.json()؟\n    ليكن الملاحظة = ملاحظة(الرقم: الرقم_التالي، العنوان: المدخل.العنوان، المالك: المستخدم.الرقم)",
            "Leh: di ghaltet el mass assignment: el form elly el mafrood tghayar title te2dar kaman tghayar `is_admin` law el request etrabat bel row kollo. W el input type el monfasel bey7added bel zabt el request mesmoo7lo yelmes eh (el spec §18.3).\n\nBadal:\n    khali note: Note = req.json()?\n\nEkteb:\n    @derive(Decode)\n    haykal NewNote:\n        title: nass\n    khali input: NewNote = req.json()?\n    khali note = Note(id: next_id, title: input.title, owner: user.id)",
        ],
        proposed: false,
    },
    Entry {
        code: "W0720",
        title: ["LEGACY CRYPTO IMPORTED", "استيراد تشفير قديم", "IMPORT CRYPTO ADEEM"],
        message: [
            "This module imports `std/crypto/legacy` (MD5, SHA-1, raw RSA). These are only for talking to old systems: they are not safe for new passwords, signatures or secrets.",
            "هذه الوحدة تستورد `std/crypto/legacy` (MD5 وSHA-1 وRSA الخام). وهذه للتعامل مع الأنظمة القديمة فقط، وليست آمنة لكلمات مرور أو توقيعات أو أسرار جديدة.",
            "El module di bte3mel import l `std/crypto/legacy` (MD5 w SHA-1 w RSA kham). Dol bas lel ta3amol ma3 systems adeema: mesh amneen l passwords aw signatures aw asrar gedeeda.",
        ],
        help: [
            "Use the high-level APIs in `std/crypto`: `password.hash` and `verify`, `aead.seal` and `open`, or `sign` and `verify`. Keep the legacy import only where an old format requires it.",
            "استخدم الواجهات عالية المستوى في `std/crypto`: `password.hash` و`verify`، أو `aead.seal` و`open`، أو `sign` و`verify`. وأبقِ الاستيراد القديم فقط حيث يفرضه تنسيق قديم.",
            "Esta5dem el APIs el 3alya f `std/crypto`: `password.hash` w `verify`, aw `aead.seal` w `open`, aw `sign` w `verify`. W khalli el import el adeem bas f el amaken elly format adeem byfrodo.",
        ],
        explain: [
            "Why: MD5 and SHA-1 are broken for security, and raw RSA is easy to misuse. They live under `legacy` so every use is visible, and `kalam audit` counts them (spec §18.4).\n\nTo store a password, use `password.hash` from `std/crypto` (Argon2id) instead of an MD5 or SHA-1 hash from `std/crypto/legacy`. Keep the legacy import for reading an old format only.",
            "السبب: MD5 وSHA-1 مكسوران أمنيًا، وRSA الخام سهل الاستخدام الخاطئ. لذلك تقع تحت `legacy` ليظهر كل استخدام لها، ويعدّها `kalam audit` (المواصفة §18.4).\n\nلتخزين كلمة مرور استخدم `password.hash` من `std/crypto` (Argon2id) بدل تجزئة MD5 أو SHA-1 من `std/crypto/legacy`. وأبقِ الاستيراد القديم لقراءة تنسيق قديم فقط.",
            "Leh: MD5 w SHA-1 maksoreen amneyan, w RSA el kham sahl yetsta5dem ghalat. 3ashan keda homa ta7t `legacy` 3ashan kol esta5dam yeb2a bayen, w `kalam audit` bey3edhom (el spec §18.4).\n\n3ashan te5zen password esta5dem `password.hash` men `std/crypto` (Argon2id) badal hash MD5 aw SHA-1 men `std/crypto/legacy`. W khalli el import el adeem bas 3ashan te2ra format adeem.",
        ],
        proposed: false,
    },
];

#[cfg(test)]
mod tests {
    use super::*;
    use kalam_base::Skin;

    #[test]
    fn template_fills_known_names_and_keeps_other_braces() {
        let args = vec![("cp", "U+202E".to_string())];
        assert_eq!(
            render_template("found {cp}, write `\\u{...}`", &args),
            "found U+202E, write `\\u{...}`"
        );
        assert_eq!(render_template("{unknown}", &args), "{unknown}");
    }

    #[test]
    fn keyword_placeholders_are_spelled_in_the_given_skin() {
        let t = "write `{kw_else} {kw_if}` or `{core_none}{punct_try}`, not {found}";
        assert_eq!(
            render_in_skin(t, &[], Skin::En),
            "write `else if` or `None?`, not {found}"
        );
        assert_eq!(
            render_in_skin(t, &[], Skin::Ar),
            "write `وإلا إذا` or `لا_شيء؟`, not {found}"
        );
        assert_eq!(
            render_in_skin(t, &[], Skin::Franco),
            "write `walla law` or `mafeesh?`, not {found}"
        );
        // An emitter-supplied value wins over the table.
        let args = vec![("kw_if", "IF".to_string())];
        assert!(render_in_skin(t, &args, Skin::En).contains("`else IF`"));
        // Unknown concepts and ordinary names are not keyword placeholders.
        assert_eq!(keyword_concept("kw_nope"), None);
        assert_eq!(keyword_concept("found"), None);
        assert_eq!(keyword_concept("kw_let"), Some(Concept::KwLet));
    }

    #[test]
    fn placeholders_skip_literal_braces() {
        assert_eq!(
            placeholders("`\\u{...}` {cp} `{ x }` {order.id} {} {cp} {kw_let}"),
            vec!["cp", "kw_let"]
        );
    }

    #[test]
    fn k0126_states_the_true_64_bit_int_range() {
        // DECISIONS.md D18: `Int` is a checked 64-bit integer on every backend.
        let e = lookup("K0126").expect("K0126 is in the catalog");
        for msg in e.message {
            assert!(msg.contains("9223372036854775807"), "{msg}");
            assert!(
                !msg.contains("2⁵³") && !msg.contains("9007199254740991"),
                "{msg}"
            );
        }
    }

    #[test]
    fn codes_are_unique() {
        for (i, a) in CATALOG.iter().enumerate() {
            for b in &CATALOG[i + 1..] {
                assert_ne!(a.code, b.code);
            }
        }
    }
}
