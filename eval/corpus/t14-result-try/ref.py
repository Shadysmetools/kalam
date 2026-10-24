# Python reference for task 14. Mirrors Kalam semantics: Int is +-(2**53-1); str->int accepts
# ASCII and Arabic-Indic digits (spec 6.2); out-of-range text is a parse error, never a float.
INT_MAX = 2**53 - 1


class AgeError(Exception):
    pass


def to_int(text):
    digits = text.translate(str.maketrans("٠١٢٣٤٥٦٧٨٩", "0123456789"))
    body = digits[1:] if digits[:1] in "+-" and len(digits) > 1 else digits
    if not body or not body.isascii() or not body.isdigit():
        return None
    value = int(digits)
    return value if -INT_MAX <= value <= INT_MAX else None


def parse_age(text):
    if text == "":
        raise AgeError("empty input")
    value = to_int(text)
    if value is None:
        raise AgeError(f"not a number: {text}")
    if value < 0 or value > 150:
        raise AgeError(f"out of range: {value}")
    return value


def report(texts):
    try:
        print(f"total = {sum(parse_age(t) for t in texts)}")
    except AgeError as error:
        print(f"stopped: {error}")


for text in ["42", "٤٢", "-7", "abc", "", "9007199254740992", "200"]:
    try:
        print(f'"{text}" -> ok {parse_age(text)}')
    except AgeError as error:
        print(f'"{text}" -> {error}')
report(["30", "12", "٧"])
report(["30", "x1", "12"])
