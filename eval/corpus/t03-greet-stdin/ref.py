# Python reference for task 3: greet every non-blank line of standard input.
# Kalam's trim is assumed to strip ASCII spaces; the fixture uses no other whitespace.
import sys

greeted = skipped = 0
for line in sys.stdin.read().split("\n")[:-1]:
    name = line.strip(" ")
    if not name:
        skipped += 1
        continue
    print(f"Hello, {name}! مرحبا يا {name} ({len(name)} characters)")
    greeted += 1
print(f"greeted {greeted}, skipped {skipped}")
