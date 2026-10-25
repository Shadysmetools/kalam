# Python reference for task 8. Same IEEE-754 binary64 operations in the same order as the Kalam
# program; no value is a rounding tie, so round-half-even vs half-away-from-zero cannot differ.
def show_tenths(t):
    return f"{'-' if t < 0 else ''}{abs(t) // 10}.{abs(t) % 10}"


def label(f):
    if f <= 32.0:
        return "freezing"
    if f < 68.0:
        return "cool"
    if f < 86.0:
        return "warm"
    return "hot"


print("Celsius to Fahrenheit, exact (Int)")
for step in range(8):
    celsius = -40 + step * 20
    print(f"{celsius} C = {celsius * 9 // 5 + 32} F")
print("Celsius to Fahrenheit, one decimal (Float)")
for c_tenths in [-400, -175, -33, 0, 150, 217, 366, 375, 1000]:
    fahrenheit = float(c_tenths) / 10.0 * 1.8 + 32.0
    print(f"{show_tenths(c_tenths)} C = {show_tenths(round(fahrenheit * 10.0))} F ({label(fahrenheit)})")
