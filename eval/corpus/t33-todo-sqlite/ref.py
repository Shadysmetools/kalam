# Python reference for task 33: the T32 CRUD API on sqlite3 with parameterized queries, port 8080.
# Run in a fresh, empty working directory (fixture: cwd = empty temp dir).
# Strict request decoding (unknown fields, wrong types -> 400), like Kalam's schema-first decode.
import json
import re
import sqlite3
from http.server import BaseHTTPRequestHandler, HTTPServer

INT_MIN, INT_MAX = -(2**63), 2**63 - 1  # Int is 64-bit (DECISIONS D18)
db = sqlite3.connect("todos.db", check_same_thread=False)
db.execute("CREATE TABLE todos (id INTEGER PRIMARY KEY AUTOINCREMENT, title TEXT NOT NULL, "
           "done INTEGER NOT NULL)")
db.commit()


def decode(raw, spec):
    try:
        obj = json.loads(raw)
    except ValueError:
        return None
    if type(obj) is not dict or set(obj) != set(spec):
        return None
    return obj if all(type(obj[k]) is t for k, t in spec.items()) else None


def todo(row):
    return {"id": row[0], "title": row[1], "done": row[2] == 1}


class Handler(BaseHTTPRequestHandler):
    def send(self, status, body=None):
        data = b"" if body is None else json.dumps(body).encode()
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("X-Content-Type-Options", "nosniff")
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def body(self):
        return self.rfile.read(int(self.headers.get("Content-Length", 0)))

    def target(self):
        """Return (collection?, id or 'bad' or None) for /todos and /todos/<id>."""
        if self.path == "/todos":
            return True, None
        m = re.fullmatch(r"/todos/([^/]+)", self.path)
        if not m:
            return False, None
        ok = re.fullmatch(r"-?[0-9]+", m.group(1)) and INT_MIN <= int(m.group(1)) <= INT_MAX
        return False, int(m.group(1)) if ok else "bad"

    def do(self, method):
        collection, tid = self.target()
        if not collection and tid is None:
            return self.send(404)
        if tid == "bad":
            return self.send(400)
        if collection and method == "GET":
            rows = db.execute("SELECT id, title, done FROM todos ORDER BY id").fetchall()
            return self.send(200, [todo(r) for r in rows])
        if collection and method == "POST":
            new = decode(self.body(), {"title": str})
            if new is None or new["title"] == "":
                return self.send(400)
            row = db.execute("INSERT INTO todos (title, done) VALUES (?, 0) RETURNING id, title, done",
                             (new["title"],)).fetchone()
            db.commit()
            return self.send(201, todo(row))
        if method == "GET":
            row = db.execute("SELECT id, title, done FROM todos WHERE id = ?", (tid,)).fetchone()
            return self.send(200, todo(row)) if row else self.send(404)
        if method == "PUT":
            upd = decode(self.body(), {"title": str, "done": bool})
            if upd is None:
                return self.send(400)
            changed = db.execute("UPDATE todos SET title = ?, done = ? WHERE id = ?",
                                 (upd["title"], 1 if upd["done"] else 0, tid)).rowcount
            db.commit()
            if changed == 0:
                return self.send(404)
            return self.send(200, {"id": tid, "title": upd["title"], "done": upd["done"]})
        if method == "DELETE":
            removed = db.execute("DELETE FROM todos WHERE id = ?", (tid,)).rowcount
            db.commit()
            return self.send(204) if removed else self.send(404)
        return self.send(405)

    def do_GET(self):
        self.do("GET")

    def do_POST(self):
        self.do("POST")

    def do_PUT(self):
        self.do("PUT")

    def do_DELETE(self):
        self.do("DELETE")

    def log_message(self, *args):
        pass


HTTPServer(("127.0.0.1", 8080), Handler).serve_forever()
