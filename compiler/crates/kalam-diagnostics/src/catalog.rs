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
//! This inline table covers the whole P6 code set (FOUNDER-QUEUE Q11, strict reading): every
//! spec §27 K01xx, K02xx, K09xx and SEC01xx code except retired ones, plus every code the
//! compiler emits. It moves to `catalog/{en,ar,franco}.toml` plus `explain/` pages once it
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
            "This file has a byte order mark (U+FEFF), or bytes that are not UTF-8. Kalam reads source files only as plain UTF-8, so every editor, reviewer and compiler sees exactly the same characters.",
            "يحتوي هذا الملف على علامة ترتيب البايتات (U+FEFF) أو على بايتات ليست بترميز UTF-8. يقرأ كلام ملفات الكود بترميز UTF-8 العادي فقط، حتى يرى كل محرر ومراجع ومترجم الحروف نفسها تمامًا.",
            "El file da feeh byte order mark (U+FEFF), aw bytes mesh UTF-8. Kalam beye2ra malafat el code UTF-8 3adi bas, 3ashan kol editor w reviewer w compiler yshoofo nafs el 7oroof bel zabt.",
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
            "This file is not in Unicode normalization form NFC. The same letter can be stored as one code point or as a base letter plus combining marks; the two look identical but compare as different text.",
            "هذا الملف ليس بصيغة التطبيع NFC في يونيكود. يمكن تخزين الحرف نفسه كنقطة ترميز واحدة، أو كحرف أساسي تتبعه علامات مركّبة، فيبدوان متطابقين لكنهما نصان مختلفان عند المقارنة.",
            "El file da mesh b sheklo el NFC f Unicode. Nafs el 7arf momken yetkhezen code point wa7ed, aw 7arf asasi w ba3do 3alamat murakkaba; el etnein shabah ba3d bel zabt, bas lamma tqarenhom yetle3o mokhtalfeen.",
        ],
        help: [
            "Run `kalam fmt`, which rewrites the file in NFC and changes nothing else.",
            "شغّل `kalam fmt`، فهو يعيد كتابة الملف بصيغة NFC ولا يغيّر شيئًا آخر.",
            "Shaghal `kalam fmt`, hayekteb el file b NFC w mesh hayghayar ay 7aga tanya.",
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
            "Two different names in this scope look the same once lookalike letters are folded together, for example `ي` and `ى`, `ك` and `ک`, or a Latin letter and its Cyrillic twin. A reader can't tell which of the two a line uses.",
            "في هذا النطاق اسمان مختلفان يتطابقان في الشكل بعد توحيد الحروف المتشابهة، مثل `ي` و`ى`، أو `ك` و`ک`، أو حرف لاتيني وشبيهه السيريلي. لا يستطيع القارئ أن يعرف أي الاسمين يستخدمه السطر.",
            "F el scope da fe esmeen mokhtalfeen bas shabah ba3d ba3d ma el 7oroof el shabah ba3d tetwa7ad, zay `ي` w `ى`, aw `ك` w `ک`, aw 7arf latini w el 7arf el cyrillic elly shabaho. Elly beye2ra mesh hay3raf el satr beyesta5dem anhy esm.",
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
            "The number `{literal}` mixes two digit systems, or uses the other system's decimal separator. A reader could misread its value.",
            "الرقم `{literal}` يخلط بين نظامين للأرقام، أو يستخدم الفاصلة العشرية الخاصة بالنظام الآخر، فقد يُخطئ القارئ في قيمته.",
            "El rakam `{literal}` khalat ben nezameen arqam, aw esta5dem el fasla el 3ashreya bta3et el nezam el tany, fa momken elly beye2ra yefham qeemto ghalat.",
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
            "`{literal}` uses Persian digits (۰–۹), which look almost the same as the Arabic-Indic digits ٠–٩ but are different characters: ۴ and ٤, ۶ and ٦ are easy to confuse.",
            "`{literal}` يستخدم الأرقام الفارسية (۰–۹)، وهي تشبه الأرقام العربية الهندية ٠–٩ جدًا لكنها حروف مختلفة، فمن السهل الخلط بين ۴ و٤، وبين ۶ و٦.",
            "`{literal}` feeh arqam farsy (۰–۹), w di shabah el arqam el 3arabeya ٠–٩ awy bas 7oroof tanya, w sahl telkhbat ben ۴ w ٤, w ben ۶ w ٦.",
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
            "`{literal}` uses Arabic-Indic digits, which only Arabic-skin files (`.ar.kl`) accept. In this file, numbers use ASCII digits.",
            "`{literal}` مكتوب بأرقام عربية هندية، ولا تقبلها إلا ملفات الواجهة العربية (`.ar.kl`). أما في هذا الملف فالأرقام تُكتب بأرقام ASCII.",
            "`{literal}` maktoob b arqam 3arabeya, w di bas f malafat el 3arabi (`.ar.kl`). F el file da el arqam btetketeb ASCII.",
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
            "`{literal}` doesn't fit in `{type}`. For `{core_int}`, the range is the whole numbers from -9223372036854775808 to 9223372036854775807 (64 bits) on every platform.",
            "`{literal}` لا يتسع له النوع `{type}`. مدى النوع `{core_int}` هو الأعداد الصحيحة من -9223372036854775808 إلى 9223372036854775807 (64 بت) على كل المنصات.",
            "`{literal}` mesh hayeksa3 f `{type}`. Lel `{core_int}`, el mada howa el arqam el sa7ee7a men -9223372036854775808 l 9223372036854775807 (64 bit) 3ala kol platform.",
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
