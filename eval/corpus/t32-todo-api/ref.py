# Python reference for task 32: the same CRUD API on the standard library, port 8080.
# Strict request decoding (unknown fields, wrong types -> 400), like Kalam's schema-first decode.
import json
import re
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

INT_MAX = 2**53 - 1
store = {"next_id": 1, "todos": []}


def decode(raw, spec):
    try:
        obj = json.loads(raw)
    except ValueError:
        return None
    if type(obj) is not dict or set(obj) != set(spec):
        return None
    return obj if all(type(obj[k]) is t for k, t in spec.items()) else None


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
        ok = re.fullmatch(r"-?[0-9]+", m.group(1)) and abs(int(m.group(1))) <= INT_MAX
        return False, int(m.group(1)) if ok else "bad"

    def do(self, method):
        collection, tid = self.target()
        if not collection and tid is None:
            return self.send(404)
        if tid == "bad":
            return self.send(400)
        todos = store["todos"]
        if collection and method == "GET":
            return self.send(200, todos)
        if collection and method == "POST":
            new = decode(self.body(), {"title": str})
            if new is None or new["title"] == "":
                return self.send(400)
            todo = {"id": store["next_id"], "title": new["title"], "done": False}
            store["next_id"] += 1
            store["todos"] = todos + [todo]
            return self.send(201, todo)
        found = [t for t in todos if t["id"] == tid]
        if method == "GET":
            return self.send(200, found[0]) if found else self.send(404)
        if method == "PUT":
            upd = decode(self.body(), {"title": str, "done": bool})
            if upd is None:
                return self.send(400)
            if not found:
                return self.send(404)
            todo = {"id": tid, "title": upd["title"], "done": upd["done"]}
            store["todos"] = [todo if t["id"] == tid else t for t in todos]
            return self.send(200, todo)
        if method == "DELETE":
            if not found:
                return self.send(404)
            store["todos"] = [t for t in todos if t["id"] != tid]
            return self.send(204)
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


ThreadingHTTPServer(("127.0.0.1", 8080), Handler).serve_forever()
