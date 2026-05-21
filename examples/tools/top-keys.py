#!/usr/bin/env python3
"""
Top-level Keys — API Almanac sample analyzer plugin.

Reads an API Almanac plugin bundle from stdin and outputs an HTML artifact
listing the top-level keys of the JSON response body.

To install in a project:
  cp examples/tools/top-keys.py  your-project/tools/top-keys.py
  cp examples/tools/top-keys.yaml your-project/tools/top-keys.yaml
"""
import html
import json
import sys


def render(body):
    if isinstance(body, dict):
        if not body:
            return "<p>Response object has no keys.</p>"
        items = "".join(
            f"<li><code>{html.escape(str(k))}</code>"
            f"<span class='type'> {type(body[k]).__name__}</span></li>"
            for k in body
        )
        return f"<ul>{items}</ul>"
    if isinstance(body, list):
        return f"<p>Response is an array with <strong>{len(body)}</strong> item(s).</p>"
    return "<p>Response body is not a JSON object or array.</p>"


bundle = json.load(sys.stdin)
body = bundle.get("response", {}).get("body")

content = f"""<!doctype html>
<html>
<head>
<meta charset="utf-8">
<style>
  body {{ font-family: monospace; font-size: 13px; margin: 0; padding: 10px;
         color: #ccc; background: #1e1e1e; }}
  ul {{ margin: 0; padding: 0 0 0 16px; }}
  li {{ margin: 3px 0; }}
  code {{ color: #79b8ff; }}
  .type {{ color: #6a9955; font-size: 11px; }}
</style>
</head>
<body>{render(body)}</body>
</html>"""

json.dump({
    "title": "Top-level Keys",
    "artifacts": [
        {
            "kind": "html",
            "title": "Response keys",
            "content": content,
        }
    ],
    "diagnostics": [],
}, sys.stdout)
