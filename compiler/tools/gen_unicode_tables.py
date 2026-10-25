"""Generates the vendored Unicode tables of kalam-lexer (FOUNDER-QUEUE Q18).

Writes, deterministically (a rerun with the same inputs gives no diff):

- crates/kalam-lexer/src/unicode_tables.rs: General_Category Cn (SEC0112), the NFC data
  (canonical combining class, canonical decompositions, primary composites, NFC_QC No and
  Maybe) for SEC0101, and Extended_Pictographic for the spec 5.1 emoji exceptions;
- crates/kalam-lexer/src/nfc_vectors.rs: NFC test vectors computed by Python's own
  `unicodedata.normalize`, which the Rust implementation must reproduce exactly.

Sources:

- Python `unicodedata`. It must report Unicode 16.0.0, the version spec 5.1 pins for
  v0.1 (CPython 3.14 ships it). Run with `-I` so nothing outside the standard library loads.
- Extended_Pictographic is not in `unicodedata`; it is read from node's ICU
  (`/\\p{Extended_Pictographic}/u`), whose Unicode version is recorded separately. Any skew
  is harmless: a code point unassigned in 16.0 is SEC0112 wherever it appears.
- The Script property is not in `unicodedata` either, so SEC0105 keeps its hand-written
  block table in lib.rs (`script_of`).

Usage, from compiler/:
    <python 3.14> -I tools/gen_unicode_tables.py           # write the files
    <python 3.14> -I tools/gen_unicode_tables.py --check   # exit 1 if they are stale
"""

import json
import pathlib
import random
import subprocess
import sys
import unicodedata

REQUIRED_VERSION = "16.0.0"
MAX = 0x110000
SURROGATES = range(0xD800, 0xE000)

ROOT = pathlib.Path(__file__).resolve().parent.parent
TABLES = ROOT / "crates" / "kalam-lexer" / "src" / "unicode_tables.rs"
VECTORS = ROOT / "crates" / "kalam-lexer" / "src" / "nfc_vectors.rs"

# Hangul (Unicode 3.12): composed algorithmically, not from tables.
L_BASE, V_BASE, T_BASE, S_BASE = 0x1100, 0x1161, 0x11A7, 0xAC00
L_COUNT, V_COUNT, T_COUNT = 19, 21, 28
S_COUNT = L_COUNT * V_COUNT * T_COUNT


def code_points():
    for cp in range(MAX):
        if cp not in SURROGATES:
            yield cp


def ranges(cps):
    """Sorted code points -> [(lo, hi)] inclusive runs."""
    out = []
    for cp in cps:
        if out and out[-1][1] == cp - 1:
            out[-1][1] = cp
        else:
            out.append([cp, cp])
    return [tuple(r) for r in out]


def nfc(s):
    return unicodedata.normalize("NFC", s)


def nfd(s):
    return unicodedata.normalize("NFD", s)


def canonical_decomposition(cp):
    """One level of the canonical decomposition, or None (Hangul is algorithmic)."""
    if S_BASE <= cp < S_BASE + S_COUNT:
        return None
    d = unicodedata.decomposition(chr(cp))
    if not d or d.startswith("<"):
        return None
    parts = [int(p, 16) for p in d.split()]
    assert 1 <= len(parts) <= 2, (hex(cp), d)
    return parts


def extended_pictographic():
    js = (
        "const r=/^\\p{Extended_Pictographic}$/u;const o=[];"
        "for(let c=0;c<0x110000;c++){if(c>=0xD800&&c<=0xDFFF)continue;"
        "if(r.test(String.fromCodePoint(c)))o.push(c)}"
        "console.log(JSON.stringify({unicode:process.versions.unicode,"
        "icu:process.versions.icu,node:process.versions.node,cps:o}))"
    )
    out = subprocess.run(
        ["node", "-e", js], check=True, capture_output=True, text=True
    ).stdout
    data = json.loads(out)
    return data, ranges(data["cps"])


def esc(s):
    return "".join(
        ch if " " <= ch <= "~" and ch not in '"\\{}' else "\\u{%X}" % ord(ch) for ch in s
    )


def fmt_pairs(name, doc, rows):
    lines = [f"/// {doc}", f"pub(crate) static {name}: &[(u32, u32)] = &["]
    for lo, hi in rows:
        lines.append(f"    (0x{lo:04X}, 0x{hi:04X}),")
    lines.append("];")
    return "\n".join(lines)


def fmt_triples(name, doc, rows, kind="u32"):
    lines = [f"/// {doc}", f"pub(crate) static {name}: &[(u32, u32, {kind})] = &["]
    for a, b, c in rows:
        third = f"{c}" if kind == "u8" else f"0x{c:04X}"
        lines.append(f"    (0x{a:04X}, 0x{b:04X}, {third}),")
    lines.append("];")
    return "\n".join(lines)


def generate():
    version = unicodedata.unidata_version
    if version != REQUIRED_VERSION:
        sys.exit(
            f"unicodedata is Unicode {version}; spec 5.1 pins {REQUIRED_VERSION} "
            "(run this with CPython 3.14)"
        )

    unassigned = ranges(
        cp for cp in code_points() if unicodedata.category(chr(cp)) == "Cn"
    )

    ccc_rows = []
    for cp in code_points():
        c = unicodedata.combining(chr(cp))
        if c == 0:
            continue
        if ccc_rows and ccc_rows[-1][1] == cp - 1 and ccc_rows[-1][2] == c:
            ccc_rows[-1][1] = cp
        else:
            ccc_rows.append([cp, cp, c])

    decomp_rows = []
    compose_rows = []
    for cp in code_points():
        parts = canonical_decomposition(cp)
        if parts is None:
            continue
        decomp_rows.append((cp, parts[0], parts[1] if len(parts) == 2 else 0))
        # A primary composite: NFC recomposes exactly this pair into it. This drops
        # singletons, composition exclusions and non-starter decompositions.
        if len(parts) == 2 and nfc(chr(parts[0]) + chr(parts[1])) == chr(cp):
            compose_rows.append((parts[0], parts[1], cp))
    compose_rows.sort()

    qc_no = [cp for cp in code_points() if nfc(chr(cp)) != chr(cp)]
    maybe = {b for _, b, _ in compose_rows}
    maybe.update(range(V_BASE, V_BASE + V_COUNT))
    maybe.update(range(T_BASE + 1, T_BASE + T_COUNT))
    assert not maybe & set(qc_no), "a code point is both NFC_QC=No and Maybe"
    qc_maybe = sorted(maybe)

    pict_info, pict = extended_pictographic()

    header = f"""//! GENERATED by compiler/tools/gen_unicode_tables.py. Do not edit; rerun the generator.
//!
//! Unicode {version} (Python {sys.version.split()[0]} `unicodedata.unidata_version` =
//! "{version}"), the version spec §5.1 pins for v0.1 (FOUNDER-QUEUE Q18):
//! General_Category Cn (SEC0112) and the NFC data (SEC0101). Extended_Pictographic comes
//! from node {pict_info['node']} (ICU {pict_info['icu']}, Unicode {pict_info['unicode']}),
//! because `unicodedata` has no emoji properties. Every table is sorted by code point.

/// The Unicode version of the General_Category and normalization data.
pub const UNICODE_VERSION: &str = "{version}";"""
    parts = [
        header,
        fmt_pairs(
            "UNASSIGNED",
            "General_Category = Cn (unassigned, noncharacters included): SEC0112.",
            unassigned,
        ),
        fmt_triples(
            "COMBINING_CLASS",
            "`(first, last, ccc)` for every code point whose Canonical_Combining_Class is not 0.",
            ccc_rows,
            kind="u8",
        ),
        fmt_triples(
            "DECOMPOSITION",
            "`(cp, first, second)`: one level of the canonical decomposition (`second` is 0 \n"
            "/// for a singleton). Hangul syllables are decomposed algorithmically.",
            decomp_rows,
        ),
        fmt_triples(
            "COMPOSITION",
            "`(first, second, composite)`: the primary composites, sorted by pair.",
            compose_rows,
        ),
        fmt_pairs("NFC_QC_NO", "NFC_Quick_Check = No.", ranges(qc_no)),
        fmt_pairs("NFC_QC_MAYBE", "NFC_Quick_Check = Maybe.", ranges(qc_maybe)),
        fmt_pairs("EXTENDED_PICTOGRAPHIC", "Extended_Pictographic.", pict),
    ]
    tables = "\n\n".join(parts) + "\n"

    # ---- Test vectors (Python's normalize is the oracle) --------------------------------
    vectors = []
    for cp, _, _ in decomp_rows:
        s = chr(cp)
        vectors.append(s)
        vectors.append(nfd(s))
    for s in [
        "á̖",
        "á̖",
        "é́",
        "أ",
        "ؤ",
        "ئ",
        "بَّ",
        "بَّ",
        "가",
        "각",
        "각",
        "각ᆨ",
        "ᄀ̀ᅡ",
        "ୋ",
        "େ̀ା",
        "̀a",
        "Å",
        "Ω",
        "क़",
        "̈́",
        "ཱི",
    ]:
        vectors.append(s)
    pool = sorted(
        {chr(a) for a, _, _ in compose_rows}
        | {chr(b) for _, b, _ in compose_rows}
        | {chr(cp) for cp in qc_no}
        | {chr(lo) for lo, _, _ in ccc_rows}
        | {chr(c) for c in range(L_BASE, L_BASE + L_COUNT)}
        | {chr(S_BASE), chr(S_BASE + 1), chr(S_BASE + T_COUNT)}
        | set("aeAE اب")
    )
    rng = random.Random(16_0_0)
    for _ in range(1500):
        vectors.append("".join(rng.choice(pool) for _ in range(rng.randint(1, 6))))
    seen = set()
    rows = []
    for s in vectors:
        if s in seen:
            continue
        seen.add(s)
        rows.append(f'    ("{esc(s)}", "{esc(nfc(s))}"),')
    vector_src = (
        "//! GENERATED by compiler/tools/gen_unicode_tables.py. Do not edit; rerun the generator.\n"
        f"//! `(input, NFC(input))` from Python {sys.version.split()[0]} `unicodedata` "
        f"(Unicode {version}).\n\n"
        "pub(crate) static NFC_VECTORS: &[(&str, &str)] = &[\n" + "\n".join(rows) + "\n];\n"
    )
    return {TABLES: tables, VECTORS: vector_src}


def main():
    outputs = generate()
    check = "--check" in sys.argv[1:]
    stale = []
    for path, text in outputs.items():
        old = path.read_text(encoding="utf-8") if path.exists() else None
        if old == text:
            continue
        if check:
            stale.append(str(path))
        else:
            path.write_text(text, encoding="utf-8", newline="\n")
            print(f"wrote {path}")
    if stale:
        sys.exit("stale generated files: " + ", ".join(stale))


if __name__ == "__main__":
    main()
