# Python reference for task 11: the same validation rules, applied in the same order.
def make_user(name, email, age):
    trimmed = name.strip(" ")
    if not trimmed:
        return None, "name is empty"
    if age < 13 or age > 120:
        return None, f"age {age} is outside 13..=120"
    parts = email.split("@")
    if len(parts) != 2 or not parts[0] or "." not in parts[1]:
        return None, f"bad email: {email}"
    return {"name": trimmed, "email": email.lower(), "age": age, "role": "member"}, None


inputs = [("  Sara  ", "Sara@Example.com", 30), ("", "x@y.io", 20), ("Omar", "omar.example.com", 25),
          ("Laila", "laila@site", 40), ("Huda", "huda@mail@eg", 22), ("Karim", "karim@mail.eg", 9),
          ("نور", "noor@mail.eg", 22)]
for name, email, age in inputs:
    user, error = make_user(name, email, age)
    if user:
        print(f"ok: {user['name']} <{user['email']}>, age {user['age']}, {user['role']}")
    else:
        print(f"rejected: {error}")
user, _ = make_user("Amal", "AMAL@corp.eg", 41)
admin = dict(user, role="admin")
print(f"{admin['name']} is {admin['role']}; the original is {user['role']}")
