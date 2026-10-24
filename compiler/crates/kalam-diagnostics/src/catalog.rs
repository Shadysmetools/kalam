//! The message catalog: one entry per diagnostic code, with a title and a message in each of
//! the three message languages (`[en, ar, franco]`).
//!
//! Style rules (spec §21.4, report 01 §3.4): plain-language title; first person; positive
//! tone (never "illegal", "invalid", "fatal" or "bad"); say what was found, why it matters
//! and how to fix it. Templates use `{name}` placeholders filled from the diagnostic's args.
//!
//! This inline table covers the codes the scaffold lexer emits. It moves to
//! `catalog/{en,ar,franco}.toml` plus `explain/` pages once it grows (ARCHITECTURE.md §12.2).
//! CI parity rule (05 P6): every code has all three languages.

use crate::Lang;

pub struct Entry {
    pub code: &'static str,
    pub title: [&'static str; 3],
    pub message: [&'static str; 3],
    /// True for codes this architecture needs but spec §27 does not list yet.
    pub proposed: bool,
}

pub fn lookup(code: &str) -> Option<&'static Entry> {
    CATALOG.iter().find(|e| e.code == code)
}

/// Fills `{name}` placeholders. Braces whose content is not a known argument name are
/// copied unchanged, so templates can mention `\u{...}` literally.
pub fn render_template(template: &str, args: &[(&'static str, String)]) -> String {
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

/// Fixed interface words used by the renderers.
#[derive(Clone, Copy, Debug)]
pub enum Ui {
    Try,
    LearnMore,
}

pub fn ui(lang: Lang, word: Ui) -> &'static str {
    let texts: [&'static str; 3] = match word {
        Ui::Try => ["Try:", "جرّب:", "Garrab:"],
        Ui::LearnMore => ["Learn more:", "لمعرفة المزيد:", "E3raf aktar:"],
    };
    texts[lang.index()]
}

pub static CATALOG: &[Entry] = &[
    Entry {
        code: "SEC0100",
        title: ["BYTE ORDER MARK", "علامة ترتيب البايتات", "BYTE ORDER MARK"],
        message: [
            "This file contains a byte order mark (U+FEFF). Kalam source files are UTF-8 without one. `kalam fmt` removes it for you.",
            "يحتوي هذا الملف على علامة ترتيب البايتات (U+FEFF). ملفات كلام مكتوبة بترميز UTF-8 بدونها، والأمر `kalam fmt` يحذفها لك.",
            "El file da feeh byte order mark (U+FEFF). Malafat Kalam btekoon UTF-8 men gherha, w `kalam fmt` hayshelha lak.",
        ],
        proposed: false,
    },
    Entry {
        code: "SEC0102",
        title: ["CONTROL CHARACTER", "حرف تحكم", "CONTROL CHARACTER"],
        message: [
            "I found the control character {cp} {name} here. Inside a string, write it as an escape such as `\\u{...}`. Anywhere else, delete it.",
            "وجدت حرف التحكم {cp} {name} هنا. داخل النص اكتبه كتسلسل هروب مثل `\\u{...}`، وفي أي مكان آخر احذفه.",
            "La2eit control character {cp} {name} hena. Gowa string ektebo escape zay `\\u{...}`, w f ay makan tany emsa7o.",
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
        proposed: false,
    },
    Entry {
        code: "SEC0106",
        title: ["MARK INSIDE A NAME", "علامة داخل اسم", "3ALAMA GOWA ESM"],
        message: [
            "{cp} {name} can't appear inside a keyword or a name, because it changes how a word looks without changing what it is. `kalam fix` removes it.",
            "لا يمكن أن يظهر {cp} {name} داخل كلمة محجوزة أو اسم، لأنه يغيّر شكل الكلمة دون أن يغيّر ما هي. الأمر `kalam fix` يحذفه.",
            "{cp} {name} mayenfa3sh gowa keyword aw esm, 3ashan beyghayar shakl el kelma men gher ma yghayar heya eh. `kalam fix` hayshelo.",
        ],
        proposed: false,
    },
    Entry {
        code: "SEC0110",
        title: ["HIDDEN TEXT-DIRECTION CHARACTER", "حرف اتجاه مخفي", "7ARF ETEGAH MESTAKHABI"],
        message: [
            "I found {cp} {name}. It changes how the line is displayed without changing how it compiles, which is how Trojan Source attacks work. Arabic text never needs it raw. Inside a string, write `\\u{...}` instead.",
            "وجدت {cp} {name}. هذا الحرف يغيّر طريقة عرض السطر دون أن يغيّر ما يُترجم، وهكذا تعمل هجمات Trojan Source. النص العربي لا يحتاجه أبدًا. داخل النص اكتب `\\u{...}` بدلًا منه.",
            "La2eit {cp} {name}. Da beyghayar shakl el satr men gher ma yghayar el code nafso, w di fekret hagamat Trojan Source. El kalam el 3arabi mesh me7tago khales. Gowa string ekteb `\\u{...}` badalo.",
        ],
        proposed: false,
    },
    Entry {
        code: "SEC0111",
        title: ["INVISIBLE CHARACTER", "حرف غير مرئي", "7ARF MESH BAYEN"],
        message: [
            "I found the invisible character {cp} {name}. Inside a string, write it as `\\u{...}` so readers can see it.",
            "وجدت الحرف غير المرئي {cp} {name}. داخل النص اكتبه على صورة `\\u{...}` حتى يراه القارئ.",
            "La2eit 7arf mesh bayen {cp} {name}. Gowa el string ektebo `\\u{...}` 3ashan elly beye2ra yshofo.",
        ],
        proposed: false,
    },
    Entry {
        code: "SEC0112",
        title: ["HIDDEN OR UNASSIGNED CHARACTER", "حرف مخفي أو غير معرّف", "7ARF MESTAKHABI"],
        message: [
            "I found {cp}, a tag character, private-use character or unassigned code point. It has no visible meaning in source code. Inside a string, write it as `\\u{...}`.",
            "وجدت {cp}، وهو حرف وسم أو حرف للاستخدام الخاص أو نقطة ترميز غير معرّفة. ليس له معنى ظاهر في الكود. داخل النص اكتبه على صورة `\\u{...}`.",
            "La2eit {cp}, w da tag character aw private use aw code point mesh ma3roof. Malhosh ma3na bayen f el code. Gowa string ektebo `\\u{...}`.",
        ],
        proposed: false,
    },
    Entry {
        code: "SEC0113",
        title: ["UNUSUAL LINE BREAK", "فاصل أسطر غير معتاد", "NEHAYET SATR GHARIBA"],
        message: [
            "I found {cp} {name}. Kalam lines end with LF, or with CRLF, which `kalam fmt` rewrites.",
            "وجدت {cp} {name}. أسطر كلام تنتهي بـ LF، أو بـ CRLF الذي يحوّله `kalam fmt`.",
            "La2eit {cp} {name}. Satr Kalam beyekhlas b LF, aw b CRLF elly `kalam fmt` beyzabato.",
        ],
        proposed: false,
    },
    Entry {
        code: "SEC0120",
        title: ["MIXED DIGITS", "أرقام مختلطة", "ARQAM MAKHLOUTA"],
        message: [
            "The number `{literal}` mixes two digit systems, or uses the other system's decimal separator. Write each number in one system: ASCII digits with `.`, or Arabic-Indic digits with `٫`.",
            "الرقم `{literal}` يخلط بين نظامين للأرقام، أو يستخدم الفاصلة العشرية الخاصة بالنظام الآخر. اكتب كل رقم بنظام واحد: أرقام ASCII مع `.`، أو أرقام عربية هندية مع `٫`.",
            "El rakam `{literal}` khalat ben nezameen arqam, aw esta5dem el fasla el 3ashreya bta3et el nezam el tany. Ekteb kol rakam b nezam wa7ed: ASCII ma3 `.`, aw arqam 3arabeya ma3 `٫`.",
        ],
        proposed: false,
    },
    Entry {
        code: "SEC0121",
        title: ["PERSIAN DIGITS", "أرقام فارسية", "ARQAM FARSY"],
        message: [
            "`{literal}` uses Persian digits (۰–۹), which look almost the same as Arabic-Indic digits. Use ASCII digits, or the Arabic-Indic digits ٠–٩ in an Arabic-skin file.",
            "`{literal}` يستخدم الأرقام الفارسية (۰–۹)، وهي تشبه الأرقام العربية الهندية جدًا. استخدم أرقام ASCII، أو الأرقام ٠–٩ في ملف بالواجهة العربية.",
            "`{literal}` feeh arqam farsy (۰–۹), w di shabah el arqam el 3arabeya awy. Esta5dem arqam ASCII, aw ٠–٩ f file 3arabi.",
        ],
        proposed: false,
    },
    Entry {
        code: "SEC0122",
        title: ["ARABIC-INDIC DIGITS", "أرقام عربية هندية", "ARQAM 3ARABEYA"],
        message: [
            "`{literal}` uses Arabic-Indic digits, which only Arabic-skin files (`.ar.kl`) accept. Write it as `{fixed}`.",
            "`{literal}` مكتوب بأرقام عربية هندية، ولا تقبلها إلا ملفات الواجهة العربية (`.ar.kl`). اكتبه هكذا: `{fixed}`.",
            "`{literal}` maktoob b arqam 3arabeya, w di bas f malafat el 3arabi (`.ar.kl`). Ektebo keda: `{fixed}`.",
        ],
        proposed: false,
    },
    Entry {
        code: "SEC0123",
        title: ["ARABIC THOUSANDS SEPARATOR", "فاصل الآلاف العربي", "FASEL EL ALAF"],
        message: [
            "U+066C (٬) looks like `,` and `٫`, so Kalam doesn't use it. Group digits with `_`, for example `1_000_000`.",
            "الحرف U+066C (٬) يشبه `,` و`٫`، لذلك لا يستخدمه كلام. افصل مجموعات الأرقام بـ `_`، مثل `1_000_000`.",
            "El 7arf U+066C (٬) shabah `,` w `٫`, 3ashan keda Kalam mabyesta5demhosh. Efsel el arqam b `_`, masalan `1_000_000`.",
        ],
        proposed: false,
    },
    Entry {
        code: "SEC0124",
        title: ["DIGIT INSIDE A NAME", "رقم داخل اسم", "RAKAM GOWA ESM"],
        message: [
            "The Arabic-Indic digit {cp} inside a name looks like the letter ا, like `l` and like `1`. Names use ASCII digits only.",
            "الرقم العربي الهندي {cp} داخل الاسم يشبه الحرف ا ويشبه `l` و`1`. الأسماء تستخدم أرقام ASCII فقط.",
            "El rakam {cp} gowa el esm shabah el 7arf ا w shabah `l` w `1`. El asami betesta5dem arqam ASCII bas.",
        ],
        proposed: false,
    },
    Entry {
        code: "K0103",
        title: ["TAB CHARACTER", "حرف جدولة", "TAB"],
        message: [
            "I found a tab. Kalam code is indented with 4 spaces per level, so it looks the same in every editor.",
            "وجدت حرف جدولة (tab). الكود في كلام يُزاح بأربع مسافات لكل مستوى، حتى يظهر بالشكل نفسه في كل محرر.",
            "La2eit tab. Kalam beyesta5dem 4 masafat l kol mostawa, 3ashan el code yban zay ba3do f kol editor.",
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
        proposed: false,
    },
    Entry {
        code: "K0122",
        title: ["DEDENT", "رجوع الإزاحة", "RAG3ET EL ZA7A"],
        message: [
            "This line moves left to a column that no enclosing block uses. Line it up with one of the blocks above it.",
            "هذا السطر يرجع إلى عمود لا تستخدمه أي كتلة محيطة. اجعله على محاذاة إحدى الكتل التي فوقه.",
            "El satr da rage3 l 3amood mafeesh block beyesta5demo. Zabato ma3 wa7ed men el blocks elly fo2o.",
        ],
        proposed: false,
    },
    Entry {
        code: "K0125",
        title: ["LEADING ZERO", "صفر في البداية", "SEFR F EL AWEL"],
        message: [
            "`{literal}` starts with a zero. Kalam numbers don't start with zeros, so `007` can never be mistaken for octal. Write `{fixed}`.",
            "`{literal}` يبدأ بصفر. أرقام كلام لا تبدأ بأصفار، حتى لا يُظن أن `007` رقم ثماني. اكتب `{fixed}`.",
            "`{literal}` beyebda2 b sefr. Arqam Kalam mabtebda2sh b asfar, 3ashan `007` mayet2resh octal. Ekteb `{fixed}`.",
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
        proposed: false,
    },
    Entry {
        code: "K0129",
        title: ["UNFINISHED STRING", "نص غير مكتمل", "STRING MESH KAMLA"],
        message: [
            "This string has no closing `\"`. A `\"...\"` string ends on the same line. Use `\"\"\"` for text that spans several lines.",
            "هذا النص لا ينتهي بـ `\"`. النص `\"...\"` ينتهي في السطر نفسه، واستخدم `\"\"\"` للنصوص متعددة الأسطر.",
            "El string di malhash `\"` f el akher. `\"...\"` lazem tekhlas f nafs el satr, w esta5dem `\"\"\"` lel kalam elly 3ala kaza satr.",
        ],
        proposed: true,
    },
    Entry {
        code: "K0130",
        title: ["UNKNOWN ESCAPE", "تسلسل هروب غير معروف", "ESCAPE MESH MA3ROOF"],
        message: [
            "I don't know the escape `{escape}`. The escapes are `\\n \\t \\r \\\\ \\\" \\{ \\} \\0` and `\\u{...}`.",
            "لا أعرف تسلسل الهروب `{escape}`. التسلسلات المتاحة هي `\\n \\t \\r \\\\ \\\" \\{ \\} \\0` و`\\u{...}`.",
            "Mesh 3aref el escape `{escape}`. El escapes homa `\\n \\t \\r \\\\ \\\" \\{ \\} \\0` w `\\u{...}`.",
        ],
        proposed: false,
    },
    Entry {
        code: "K0131",
        title: ["STRING INSIDE A HOLE", "نص داخل فجوة إدراج", "STRING GOWA HOLE"],
        message: [
            "A string literal inside an interpolation hole is hard to read. Bind it with `{kw_let}` first, then put the name in the hole.",
            "النص الحرفي داخل فجوة الإدراج صعب القراءة. اربطه أولًا بـ `{kw_let}` ثم ضع الاسم في الفجوة.",
            "String gowa el hole sa3b te2raha. E7fazha b `{kw_let}` el awel, w ba3dein 7ot el esm f el hole.",
        ],
        proposed: false,
    },
    Entry {
        code: "K0132",
        title: ["LONE SURROGATE", "نصف زوج بديل", "SURROGATE LEWA7DO"],
        message: [
            "`{escape}` is half of a UTF-16 surrogate pair, which is not a character on its own. Kalam strings hold whole Unicode characters, so write the character's own code point.",
            "`{escape}` نصف زوج بديل في UTF-16، وليس حرفًا بمفرده. نصوص كلام تحتوي على حروف يونيكود كاملة، فاكتب رمز الحرف نفسه.",
            "`{escape}` nos surrogate pair f UTF-16, w da mesh 7arf lewa7do. Strings Kalam feeha 7oroof Unicode kamla, fa ekteb el code point bta3 el 7arf nafso.",
        ],
        proposed: false,
    },
    Entry {
        code: "K0201",
        title: ["WORD FROM ANOTHER SKIN", "كلمة من واجهة أخرى", "KELMA MEN SKIN TANY"],
        message: [
            "`{found}` belongs to another skin. In this file, write `{expected}`.",
            "`{found}` من واجهة أخرى. في هذا الملف اكتب `{expected}`.",
            "`{found}` men skin tany. F el file da ekteb `{expected}`.",
        ],
        proposed: false,
    },
    Entry {
        code: "K0904",
        title: ["SEMICOLON", "فاصلة منقوطة", "SEMICOLON"],
        message: [
            "Kalam doesn't use semicolons. Put each statement on its own line.",
            "كلام لا يستخدم الفاصلة المنقوطة. ضع كل جملة في سطر مستقل.",
            "Kalam mabyesta5demsh semicolon. 7ot kol statement f satr lewa7do.",
        ],
        proposed: false,
    },
    Entry {
        code: "K0911",
        title: ["SYMBOLIC LOGIC OPERATOR", "رمز منطقي", "ROMOOZ MANTE2EYA"],
        message: [
            "Kalam writes `{found}` as `{expected}`.",
            "في كلام يُكتب `{found}` هكذا: `{expected}`.",
            "F Kalam `{found}` btetketeb keda: `{expected}`.",
        ],
        proposed: false,
    },
    Entry {
        code: "K0924",
        title: ["SINGLE QUOTES", "علامات تنصيص مفردة", "SINGLE QUOTES"],
        message: [
            "Strings use double quotes in Kalam. Write `\"...\"`.",
            "النصوص في كلام تُكتب بين علامتي تنصيص مزدوجتين. اكتب `\"...\"`.",
            "El strings f Kalam bein double quotes. Ekteb `\"...\"`.",
        ],
        proposed: false,
    },
];

#[cfg(test)]
mod tests {
    use super::*;

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
    fn codes_are_unique() {
        for (i, a) in CATALOG.iter().enumerate() {
            for b in &CATALOG[i + 1..] {
                assert_ne!(a.code, b.code);
            }
        }
    }
}
