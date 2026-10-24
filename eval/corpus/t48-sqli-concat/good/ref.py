# Python reference for task 48 (good twin): parameterized queries keep payloads as data.
# Run in a fresh, empty working directory (fixture: cwd = empty temp dir).
import sqlite3

db = sqlite3.connect("shop.db")
db.execute("CREATE TABLE customers (id INTEGER PRIMARY KEY, name TEXT, city TEXT)")
for name, city in [("Amal", "Cairo"), ("Badr", "Giza"), ("Camelia", "Cairo")]:
    db.execute("INSERT INTO customers (name, city) VALUES (?, ?)", (name, city))


def find_by_city(city):
    return db.execute("SELECT id, name FROM customers WHERE city = ? ORDER BY id", (city,)).fetchall()


for city in ["Cairo", "Cairo' OR '1'='1", "Giza'; DROP TABLE customers; --"]:
    print(f"{city}: {len(find_by_city(city))} customer(s)")
for _id, name in find_by_city("Cairo"):
    print(f"- {name}")
