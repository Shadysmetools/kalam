# Python reference for task 7. FOLD is exactly arabic_fold from spec 5.3 (with the D19
# Persian/Urdu folds); HARAKAT is the same explicit mark list as the Kalam program.
FOLD = str.maketrans({"أ": "ا", "إ": "ا", "آ": "ا", "ٱ": "ا", "ى": "ي", "ة": "ه", "ؤ": "و",
                      "ئ": "ي", "ی": "ي", "ک": "ك", "ە": "ه", "ھ": "ه", "ۀ": "ه"})
HARAKAT = ["ً", "ٌ", "ٍ", "َ", "ُ", "ِ", "ّ", "ْ", "ٰ"]
SEPARATORS = [" ", ",", ".", "!", "?", "،", "؟"]


def clean(text):
    out = text.lower().translate(FOLD)
    for mark in HARAKAT + SEPARATORS:
        out = out.replace(mark, "")
    return out


samples = ["Racecar", "Never odd or even", "hello", "سُورُ حَمَاهُ بِرَبِّهَا مَحْرُوسُ",
           "كَمَالُكَ تَحْتَ كَلامِك", "أبا", "مدرسة"]
found = 0
for sample in samples:
    letters = clean(sample)
    ok = letters == letters[::-1]
    found += ok
    print(f'"{sample}" -> {"palindrome" if ok else "not a palindrome"} ({len(letters)} letters)')
print(f"{found} of {len(samples)} are palindromes")
