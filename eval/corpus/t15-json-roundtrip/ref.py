# Python reference for task 15. Strict schema-first reading of JSON: unknown, duplicate, missing,
# mistyped and out-of-range (Int is +-(2**53-1)) fields are all rejected.
import json
from dataclasses import dataclass, replace, asdict

INT_MAX = 2**53 - 1


def no_dupes(pairs):
    keys = [k for k, _ in pairs]
    if len(keys) != len(set(keys)):
        raise ValueError("duplicate key")
    return dict(pairs)


def strict_int(v):
    if type(v) is not int or not -INT_MAX <= v <= INT_MAX:
        raise ValueError("not an Int")
    return v


def strict_str(v):
    if type(v) is not str:
        raise ValueError("not a String")
    return v


def fields(obj, names):
    if type(obj) is not dict or set(obj) != set(names):
        raise ValueError("shape")
    return obj


@dataclass(frozen=True)
class Address:
    city: str
    zip: str


@dataclass(frozen=True)
class User:
    id: int
    name: str
    tags: tuple
    active: bool
    address: Address


def decode_user(text):
    o = fields(json.loads(text, object_pairs_hook=no_dupes), ["id", "name", "tags", "active", "address"])
    a = fields(o["address"], ["city", "zip"])
    if type(o["active"]) is not bool or type(o["tags"]) is not list:
        raise ValueError("shape")
    return User(strict_int(o["id"]), strict_str(o["name"]), tuple(strict_str(t) for t in o["tags"]),
                o["active"], Address(strict_str(a["city"]), strict_str(a["zip"])))


def encode_user(u):
    d = asdict(u)
    d["tags"] = list(d["tags"])
    return json.dumps(d)


def check(label, text):
    try:
        o = fields(json.loads(text, object_pairs_hook=no_dupes), ["x", "y"])
        strict_int(o["x"]), strict_int(o["y"])
        verdict = "accepted"
    except ValueError:
        verdict = "rejected"
    print(f"{label}: {verdict}")


text = '{"id": 7, "name": "Sara", "tags": ["admin", "ops"], "active": true,\n  "address": {"city": "Cairo", "zip": "11511"}}'
user = decode_user(text)
print(f"{user.name} (#{user.id}) lives in {user.address.city}")
print(f"{len(user.tags)} tags, first tag {user.tags[0]}")
back = decode_user(encode_user(user))
print("round-trip: equal" if back == user else "round-trip: different")
moved = replace(user, address=replace(user.address, city="Alexandria"))
reread = decode_user(encode_user(moved))
print(f"moved to {reread.address.city}, zip still {reread.address.zip}")
check("valid", '{"x": 1, "y": 2}')
check("unknown field", '{"x": 1, "y": 2, "z": 3}')
check("duplicate key", '{"x": 1, "x": 5, "y": 2}')
check("missing field", '{"x": 1}')
check("string for Int", '{"x": "1", "y": 2}')
check("fraction for Int", '{"x": 1.5, "y": 2}')
check("Int out of range", '{"x": 9007199254740992, "y": 0}')
check("truncated", '{"x": 1,')
