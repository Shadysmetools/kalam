# Python reference for task 6. dict keeps insertion order, like Kalam's Map.
def normalize(word):
    cleaned = word.lower()
    for mark in [",", ".", "!", "?", "،", "؟"]:
        cleaned = cleaned.replace(mark, "")
    return cleaned


text = "The cat saw the dog. The dog ran! كتاب جديد، كتاب قديم؟ A cat, a dog. !"
counts = {}
for raw in text.split(" "):
    word = normalize(raw)
    if word:
        counts[word] = counts.get(word, 0) + 1
best, best_count = "", 0
for word, n in counts.items():
    print(f"{word}: {n}")
    if n > best_count:
        best, best_count = word, n
print(f"distinct words: {len(counts)}")
print(f"most frequent: {best} ({best_count})")
