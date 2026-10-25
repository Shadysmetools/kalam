# Python reference for task 31. Text holes escape & < > (INDEX.md G6: no quotes in hole values);
# List[Html] holes concatenate with no separator (G16); no dir/bdi is added (G20).
def esc(s):
    return s.replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;")


def layout(page, nav):
    file, title, body = page
    return (f"<html><head><title>{esc(title)} | Kalam Cafe</title></head><body>"
            f"<header><nav>{nav}</nav></header><main><h1>{esc(title)}</h1>{body}</main>"
            f"<footer>Built with Kalam</footer></body></html>")


visitor = "<script>alert(1)</script>"
motto = "كلام بسيط & آمن"
items = "".join(f"<li>{esc(e)}</li>" for e in ["sara@example.com", "a<b>@x.io"])
pages = [
    ("index.html", "Home", f"<p>Welcome, {esc(visitor)}!</p>"),
    ("about.html", "About Tom & Jerry", f"<p>{esc(motto)}</p>"),
    ("contact.html", "Contact", f"<p>Write to us:</p><ul>{items}</ul>"),
]
nav = "".join(f'<a href="/{file}">{esc(title)}</a>' for file, title, _ in pages)
for page in pages:
    print(f"== {page[0]} ==")
    print(layout(page, nav))
