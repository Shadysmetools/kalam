# Python reference for task 9. The secret comes from random.Random(seed); only facts that hold for
# any deterministic PRNG are printed, so the output matches Kalam's Rand.seeded whatever its algorithm.
import random
import sys

INT_MIN, INT_MAX = -(2**63), 2**63 - 1


def to_int(text):
    digits = text.translate(str.maketrans("٠١٢٣٤٥٦٧٨٩", "0123456789"))
    body = digits[1:] if digits[:1] in "+-" and len(digits) > 1 else digits
    if not body or not body.isascii() or not body.isdigit():
        return None
    value = int(digits)
    return value if INT_MIN <= value <= INT_MAX else None


def read_seed(lines):
    for attempt in range(1, 6):
        if not lines:
            return None
        text = lines.pop(0)
        seed = to_int(text.strip(" "))
        if seed is not None and seed > 0:
            print(f"attempt {attempt}: seed accepted")
            return seed
        if seed is not None:
            print(f"attempt {attempt}: the seed must be positive")
        else:
            print(f'attempt {attempt}: "{text}" is not a number')
    return None


def guesses_needed(secret):
    lowest, highest, tries = 1, 100, 0
    while True:
        guess = (lowest + highest) // 2
        tries += 1
        if guess < secret:
            lowest = guess + 1
        elif guess > secret:
            highest = guess - 1
        else:
            return tries


seed = read_seed(sys.stdin.read().split("\n")[:-1])
if seed is None:
    print("no seed given")
else:
    secret = random.Random(seed).randint(1, 100)
    replay = random.Random(seed).randint(1, 100)
    print("same seed, same secret" if secret == replay else "not deterministic")
    print("secret is in 1..=100" if 1 <= secret <= 100 else "out of range")
    tries = guesses_needed(secret)
    print("found within 7 guesses" if tries <= 7 else "needed more than 7 guesses")
print(f"worst case over 1..=100: {max(guesses_needed(s) for s in range(1, 101))} guesses")
