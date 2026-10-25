# Python reference for task 4. to_int mirrors Kalam's String.to_int: optional sign, then ASCII or
# Arabic-Indic digits only (spec 6.2, INDEX.md G9); the Int range is 64-bit (DECISIONS D18).
INT_MIN, INT_MAX = -(2**63), 2**63 - 1


def to_int(text):
    digits = text.translate(str.maketrans("٠١٢٣٤٥٦٧٨٩", "0123456789"))
    body = digits[1:] if digits[:1] in "+-" and len(digits) > 1 else digits
    if not body or not body.isascii() or not body.isdigit():
        return None
    value = int(digits)
    return value if INT_MIN <= value <= INT_MAX else None


numbers = []
for word in "12 ٧ 30 ١٠٠ -5 x7 8 ٤٢ 3.5".split(" "):
    n = to_int(word)
    if n is None:
        print(f'skipped "{word}"')
    else:
        numbers.append(n)
tenths = sum(numbers) * 10 // len(numbers)
print(f"count = {len(numbers)}")
print(f"sum = {sum(numbers)}")
print(f"average = {tenths // 10}.{tenths % 10} (truncated to one decimal)")
print(f"min = {min(numbers)}")
print(f"max = {max(numbers)}")
