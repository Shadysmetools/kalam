# Python reference for task 12.
from dataclasses import dataclass, replace


@dataclass(frozen=True)
class Rect:
    width: int
    height: int


@dataclass(frozen=True)
class Square:
    side: int


@dataclass(frozen=True)
class Triangle:
    base: int
    height: int


@dataclass(frozen=True)
class Empty:
    pass


@dataclass(frozen=True)
class Item:
    label: str
    shape: object
    count: int = 1


def area(shape):
    match shape:
        case Rect(w, h):
            return w * h
        case Square(s):
            return s * s
        case Triangle(b, h):
            return b * h // 2
        case Empty():
            return 0


def describe(shape):
    match shape:
        case Rect(w, h) if w == h:
            return "square-ish rect"
        case Rect() | Square():
            return "four sides"
        case Triangle():
            return "three sides"
        case Empty():
            return "nothing"


items = [
    Item("door", Rect(90, 210)),
    Item("tile", Square(30), count=12),
    Item("sail", Triangle(5, 7)),
    Item("box", Rect(4, 4)),
    Item("gap", Empty()),
]
total = 0
for item in items:
    one = area(item.shape)
    total += one * item.count
    print(f"{item.label}: {describe(item.shape)}, area {one} x{item.count}")
print(f"total area: {total}")
bigger = replace(items[1], count=20)
print(f"{bigger.label} now x{bigger.count}, original x{items[1].count}")
same = items[0] == Item("door", Rect(90, 210))
print("equal records" if same else "different records")
