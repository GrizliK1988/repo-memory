#!/usr/bin/env python3
"""Small, dependency-free mobile review server for docs/ and spec/."""

from __future__ import annotations

import argparse
import html
import json
from datetime import datetime, timezone
from http import HTTPStatus
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from urllib.parse import parse_qs, quote, unquote, urlparse


ROOT = Path(__file__).resolve().parents[1]
DOCS = ROOT / "docs"
REVIEW_ROOTS = (ROOT / "docs", ROOT / "spec")
COMMENTS = DOCS / ".review-comments.json"


def safe_document(value: str) -> Path | None:
    candidate = (ROOT / value).resolve()
    try:
        relative = candidate.relative_to(ROOT)
    except ValueError:
        return None
    if candidate.is_file() and relative.parts and relative.parts[0] in {"docs", "spec"}:
        return candidate
    # Compatibility with links and comments created before docs/ was prefixed.
    legacy_candidate = (DOCS / value).resolve()
    try:
        legacy_candidate.relative_to(DOCS.resolve())
    except ValueError:
        return None
    return legacy_candidate if legacy_candidate.is_file() else None


def documents() -> list[str]:
    return sorted(
        str(path.relative_to(ROOT))
        for review_root in REVIEW_ROOTS
        if review_root.is_dir()
        for path in review_root.rglob("*")
        if path.is_file() and not path.name.startswith(".")
    )


def read_comments() -> list[dict[str, str]]:
    try:
        contents = json.loads(COMMENTS.read_text(encoding="utf-8"))
        return contents if isinstance(contents, list) else []
    except (FileNotFoundError, json.JSONDecodeError):
        return []


def write_comments(comments: list[dict[str, str]]) -> None:
    COMMENTS.write_text(json.dumps(comments, indent=2) + "\n", encoding="utf-8")


def page(title: str, body: str) -> bytes:
    return f"""<!doctype html>
<html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">
<title>{html.escape(title)}</title><style>
:root {{ color-scheme: light dark; font-family: system-ui, sans-serif; }}
body {{ margin: auto; max-width: 880px; padding: 20px; line-height: 1.5; }}
a {{ color: #1769e0; }} nav {{ display:flex; gap:12px; flex-wrap:wrap; margin-bottom:20px; }}
pre {{ white-space: pre-wrap; overflow-wrap: anywhere; padding:16px; border-radius:8px; background:#f2f4f7; color:#171a1f; }}
textarea,input {{ box-sizing:border-box; width:100%; padding:12px; font:inherit; margin:5px 0 12px; }}
button {{ padding:11px 16px; font:inherit; border-radius:6px; border:0; background:#1769e0; color:white; }}
.comment {{ border-left:4px solid #1769e0; padding:8px 12px; margin:12px 0; background:#f2f4f7; color:#171a1f; }}
small {{ color:#59636e; }}
</style></head><body>{body}</body></html>""".encode()


class ReviewHandler(BaseHTTPRequestHandler):
    def send_html(self, status: HTTPStatus, title: str, body: str) -> None:
        data = page(title, body)
        self.send_response(status)
        self.send_header("Content-Type", "text/html; charset=utf-8")
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def do_GET(self) -> None:  # noqa: N802
        request = urlparse(self.path)
        if request.path == "/":
            links = "".join(
                f'<li><a href="/document?path={quote(name)}">{html.escape(name)}</a></li>'
                for name in documents()
            ) or "<li>No reviewable files found in docs/ or spec/.</li>"
            self.send_html(HTTPStatus.OK, "Markdown review", f"<h1>Markdown file review</h1><p>Files from <code>docs/</code> and <code>spec/</code>. Open a document, then leave feedback for the correction pass.</p><ul>{links}</ul>")
            return
        if request.path == "/document":
            name = parse_qs(request.query).get("path", [""])[0]
            source = safe_document(unquote(name))
            if not source:
                self.send_html(HTTPStatus.NOT_FOUND, "Not found", "<h1>Document not found</h1><p><a href='/'>Back to documents</a></p>")
                return
            try:
                text = source.read_text(encoding="utf-8")
            except UnicodeDecodeError:
                self.send_html(HTTPStatus.UNSUPPORTED_MEDIA_TYPE, "Preview unavailable", "<h1>Preview unavailable</h1><p>This file is not UTF-8 text.</p>")
                return
            relevant = [comment for comment in read_comments() if comment.get("document") == name]
            comments_html = "".join(
                f"<div class='comment'><strong>{html.escape(comment.get('author', 'Anonymous'))}</strong> "
                f"<small>{html.escape(comment.get('created_at', ''))}</small><br>{html.escape(comment.get('comment', ''))}</div>"
                for comment in relevant
            ) or "<p>No comments yet.</p>"
            safe_name = html.escape(name)
            body = f"""<nav><a href='/'>← All files</a></nav><h1>{safe_name}</h1>
<pre>{html.escape(text)}</pre><hr><h2>Leave a correction note</h2>
<form id='comment-form'><label>Name (optional)<input name='author' maxlength='100' autocomplete='name'></label>
<label>Comment<textarea name='comment' required maxlength='4000' rows='5' placeholder='What should be changed?'></textarea></label>
<button>Save comment</button></form><p id='status'></p><h2>Saved comments</h2>{comments_html}
<script>document.querySelector('#comment-form').addEventListener('submit', async e => {{e.preventDefault();let f=new FormData(e.target);let r=await fetch('/comments',{{method:'POST',headers:{{'Content-Type':'application/json'}},body:JSON.stringify({{document:{json.dumps(name)},author:f.get('author'),comment:f.get('comment')}})}});document.querySelector('#status').textContent=r.ok?'Saved. Reloading…':'Could not save comment.';if(r.ok)setTimeout(()=>location.reload(),400)}});</script>"""
            self.send_html(HTTPStatus.OK, name, body)
            return
        self.send_html(HTTPStatus.NOT_FOUND, "Not found", "<h1>Not found</h1>")

    def do_POST(self) -> None:  # noqa: N802
        if urlparse(self.path).path != "/comments":
            self.send_error(HTTPStatus.NOT_FOUND)
            return
        try:
            length = int(self.headers.get("Content-Length", "0"))
            payload = json.loads(self.rfile.read(min(length, 5000)))
            document = str(payload["document"])
            comment = str(payload["comment"]).strip()
            author = str(payload.get("author", "")).strip() or "Anonymous"
            if not safe_document(document) or not comment or len(comment) > 4000 or len(author) > 100:
                raise ValueError
        except (ValueError, KeyError, json.JSONDecodeError):
            self.send_error(HTTPStatus.BAD_REQUEST, "Provide a valid document and comment")
            return
        comments = read_comments()
        comments.append({"document": document, "author": author, "comment": comment,
                         "created_at": datetime.now(timezone.utc).strftime("%Y-%m-%d %H:%M UTC")})
        write_comments(comments)
        self.send_response(HTTPStatus.CREATED)
        self.send_header("Content-Length", "0")
        self.end_headers()

    def log_message(self, fmt: str, *args: object) -> None:
        print(f"{self.address_string()} - {fmt % args}")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="Serve docs/ and spec/ for mobile review and comments")
    parser.add_argument("--port", type=int, default=8010)
    args = parser.parse_args()
    print(f"Markdown review server: http://0.0.0.0:{args.port}")
    ThreadingHTTPServer(("0.0.0.0", args.port), ReviewHandler).serve_forever()
