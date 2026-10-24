# Python reference for task 49 (positive twin). Text-context escaping of & < >, query-value
# percent-encoding, a PathSegment validator, an RE2-compatible pattern and a redacting secret.
import re
from urllib.parse import quote


def esc(s):
    return s.replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;")


payload = "<script>alert(1)</script>"
items = "".join(f"<li>{esc(n)}</li>" for n in ["Tom & Jerry", "<b>bold</b>", "سارة"])
print(f"<p>{esc(payload)}</p><ul>{items}</ul>")
print(f'<a href="https://example.com/search?q={quote("tea&cakes=1#top", safe="")}">search</a>')


def segment_ok(s):
    return s not in ("", ".", "..") and not any(c in s for c in "/\\\0")


for candidate in ["report.pdf", "../etc/passwd", "a/b", ""]:
    print(f'segment "{candidate}": {"accepted" if segment_ok(candidate) else "rejected"}')
username = re.compile(r"^[a-z0-9_]{3,20}$")
for candidate in ["sara_99", "x", "robert'); DROP TABLE students;--"]:
    print(f'username "{candidate}": {"valid" if username.fullmatch(candidate) else "invalid"}')
print("password: [REDACTED]")
