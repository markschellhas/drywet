#!/usr/bin/env python3
"""Generate docs/html/*.html from the Markdown pages in docs/."""

from __future__ import annotations

import html
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parent
DOCS = ROOT.parent

PAGES = [
    ("index.md", "index.html", "Overview", "Start"),
    ("getting-started.md", "getting-started.html", "Getting started", "Start"),
    ("context-transport.md", "context-transport.html", "Context & Transport", "Runtime"),
    ("time.md", "time.html", "Musical time", "Runtime"),
    ("instruments.md", "instruments.html", "Instruments", "Runtime"),
    ("scheduling.md", "scheduling.html", "Sequence, Part, Loop", "Runtime"),
    ("output.md", "output.html", "Output sinks", "Runtime"),
    ("engine.md", "engine.html", "Stdio engine", "Hosts"),
    ("examples.md", "examples.html", "Examples", "Hosts"),
    ("api.md", "api.html", "API cheat sheet", "Hosts"),
]

NAV_GROUPS = [
    ("Start", ["index.html", "getting-started.html"]),
    (
        "Runtime",
        [
            "context-transport.html",
            "time.html",
            "instruments.html",
            "scheduling.html",
            "output.html",
        ],
    ),
    ("Hosts", ["engine.html", "examples.html", "api.html"]),
]

LABEL = {html_name: label for _, html_name, label, _ in PAGES}

TITLES = {
    "index.html": "drywet — musician runtime for Rust audio apps",
    "getting-started.html": "Getting started — drywet",
    "context-transport.html": "Context & Transport — drywet",
    "time.html": "Musical time — drywet",
    "instruments.html": "Instruments — drywet",
    "scheduling.html": "Sequence, Part, Loop — drywet",
    "output.html": "Output sinks — drywet",
    "engine.html": "Stdio engine — drywet",
    "examples.html": "Examples — drywet",
    "api.html": "API cheat sheet — drywet",
}

FOOTERS = {
    "index.html": (
        "<span>drywet v1 draft API, from the product spec</span>"
        '<a href="getting-started.html">Getting started →</a>'
    ),
    "getting-started.html": (
        '<a href="index.html">← Overview</a>'
        '<a href="context-transport.html">Context &amp; Transport →</a>'
    ),
    "context-transport.html": (
        '<a href="getting-started.html">← Getting started</a>'
        '<a href="time.html">Musical time →</a>'
    ),
    "time.html": (
        '<a href="context-transport.html">← Context &amp; Transport</a>'
        '<a href="instruments.html">Instruments →</a>'
    ),
    "instruments.html": (
        '<a href="time.html">← Musical time</a>'
        '<a href="scheduling.html">Sequence, Part, Loop →</a>'
    ),
    "scheduling.html": (
        '<a href="instruments.html">← Instruments</a>'
        '<a href="output.html">Output sinks →</a>'
    ),
    "output.html": (
        '<a href="scheduling.html">← Sequence, Part, Loop</a>'
        '<a href="engine.html">Stdio engine →</a>'
    ),
    "engine.html": (
        '<a href="output.html">← Output sinks</a>'
        '<a href="examples.html">Examples →</a>'
    ),
    "examples.html": (
        '<a href="engine.html">← Stdio engine</a>'
        '<a href="api.html">API cheat sheet →</a>'
    ),
    "api.html": (
        '<a href="examples.html">← Examples</a>'
        '<a href="index.html">Overview</a>'
    ),
}

RUST_KW = {
    "use",
    "let",
    "fn",
    "mut",
    "if",
    "else",
    "match",
    "Some",
    "None",
    "true",
    "false",
    "impl",
    "struct",
    "pub",
    "mod",
    "for",
    "in",
    "return",
    "Ok",
    "Err",
    "as",
    "const",
    "static",
    "trait",
    "where",
    "self",
    "Self",
    "crate",
    "super",
    "type",
    "enum",
    "loop",
    "while",
    "break",
    "continue",
    "move",
    "ref",
    "Box",
    "Vec",
    "Default",
}


def slug(text: str) -> str:
    text = re.sub(r"<[^>]+>", "", text)
    text = html.unescape(text).lower()
    text = re.sub(r"[^a-z0-9]+", "-", text).strip("-")
    return text


def md_href(href: str) -> str:
    if href.startswith("http") or href.startswith("#") or href.startswith("../"):
        if href.startswith("../examples"):
            return "https://github.com/markschellhas/drywet/tree/master/examples"
        if href.startswith("../"):
            return href.replace("../", "../../")
        return href
    if href.endswith(".md"):
        path, frag = href, ""
        if "#" in href:
            path, frag = href.split("#", 1)
            frag = "#" + frag
        name = path.rsplit("/", 1)[-1].replace(".md", ".html")
        return name + frag
    if href.startswith("html/"):
        return href[len("html/") :]
    return href


def inline(text: str) -> str:
    parts: list[str] = []
    i = 0
    pattern = re.compile(
        r"(`[^`]+`|\*\*[^*]+\*\*|\[[^\]]+\]\([^)]+\))"
    )
    for match in pattern.finditer(text):
        parts.append(html.escape(text[i : match.start()]))
        token = match.group(0)
        if token.startswith("`"):
            parts.append(f"<code>{html.escape(token[1:-1])}</code>")
        elif token.startswith("**"):
            parts.append(f"<strong>{inline(token[2:-2])}</strong>")
        else:
            label, href = re.match(r"\[([^\]]+)\]\(([^)]+)\)", token).groups()
            parts.append(f'<a href="{html.escape(md_href(href))}">{inline(label)}</a>')
        i = match.end()
    parts.append(html.escape(text[i:]))
    return "".join(parts)


def highlight(code: str, lang: str) -> str:
    if lang not in {"rust", "toml"}:
        return html.escape(code)

    out: list[str] = []
    i = 0
    while i < len(code):
        if code.startswith("//", i):
            end = code.find("\n", i)
            if end < 0:
                end = len(code)
            out.append(f'<span class="tok-cm">{html.escape(code[i:end])}</span>')
            i = end
            continue
        if code[i] in "\"'":
            q = code[i]
            j = i + 1
            while j < len(code):
                if code[j] == "\\":
                    j += 2
                    continue
                if code[j] == q:
                    j += 1
                    break
                j += 1
            out.append(f'<span class="tok-str">{html.escape(code[i:j])}</span>')
            i = j
            continue
        if code[i].isdigit() or (
            code[i] == "." and i + 1 < len(code) and code[i + 1].isdigit()
        ):
            j = i
            while j < len(code) and (code[j].isdigit() or code[j] in "._"):
                j += 1
            out.append(f'<span class="tok-num">{html.escape(code[i:j])}</span>')
            i = j
            continue
        if code[i].isalpha() or code[i] == "_":
            j = i
            while j < len(code) and (code[j].isalnum() or code[j] == "_"):
                j += 1
            word = code[i:j]
            cls = "tok-kw" if word in RUST_KW else None
            if cls:
                out.append(f'<span class="{cls}">{html.escape(word)}</span>')
            else:
                out.append(html.escape(word))
            i = j
            continue
        out.append(html.escape(code[i]))
        i += 1
    return "".join(out)


def parse_table(lines: list[str], start: int) -> tuple[str, int]:
    rows = []
    i = start
    while i < len(lines) and "|" in lines[i]:
        cells = [c.strip() for c in lines[i].strip().strip("|").split("|")]
        rows.append(cells)
        i += 1
    if len(rows) < 2:
        return "", start
    header, body = rows[0], rows[2:]
    thead = "".join(f"<th>{inline(c)}</th>" for c in header)
    tbody = []
    for row in body:
        tbody.append("<tr>" + "".join(f"<td>{inline(c)}</td>" for c in row) + "</tr>")
    html_table = (
        "<table><thead><tr>"
        + thead
        + "</tr></thead><tbody>"
        + "".join(tbody)
        + "</tbody></table>"
    )
    return html_table, i


def fence_label(info: str) -> str:
    info = info.strip()
    return {
        "rust": "Rust",
        "toml": "Cargo.toml",
        "text": "shell",
        "": "text",
    }.get(info, info)


def convert(md: str, html_name: str) -> tuple[str, str, str, list[tuple[str, str]]]:
    lines = md.splitlines()
    title = ""
    kicker = ""
    body: list[str] = []
    headings: list[tuple[str, str]] = []
    i = 0
    para: list[str] = []
    list_items: list[str] = []
    list_tag = ""

    def flush_para() -> None:
        nonlocal para
        if para:
            text = " ".join(para)
            body.append(f"<p>{inline(text)}</p>")
            para = []

    def flush_list() -> None:
        nonlocal list_items, list_tag
        if list_items:
            body.append(f"<{list_tag}>" + "".join(list_items) + f"</{list_tag}>")
            list_items = []
            list_tag = ""

    while i < len(lines):
        line = lines[i]
        if line.startswith("<!-- kicker:"):
            kicker = line.split(":", 1)[1].rstrip(" -->").strip()
            i += 1
            continue
        if line.startswith("# "):
            title = line[2:].strip()
            i += 1
            continue
        if line.startswith("```"):
            flush_para()
            flush_list()
            info = line[3:]
            i += 1
            code_lines = []
            while i < len(lines) and not lines[i].startswith("```"):
                code_lines.append(lines[i])
                i += 1
            i += 1
            lang = info.strip().split()[0] if info.strip() else ""
            body.append(
                '<div class="code">'
                f'<div class="meta"><span>{html.escape(fence_label(lang))}</span>'
                '<button class="copy" type="button">Copy</button></div>'
                f"<pre>{highlight(chr(10).join(code_lines), lang)}</pre></div>"
            )
            continue
        if line.startswith("|") and i + 1 < len(lines) and set(lines[i + 1].replace("|", "").replace("-", "").replace(" ", "")) == set():
            flush_para()
            flush_list()
            table, i = parse_table(lines, i)
            body.append(table)
            continue
        if line.startswith("> [!"):
            flush_para()
            flush_list()
            kind = re.match(r"> \[!(\w+)\]", line).group(1).lower()
            css = {"note": "note", "warning": "warn", "caution": "danger"}.get(kind, "note")
            i += 1
            quote = []
            while i < len(lines) and lines[i].startswith(">"):
                quote.append(lines[i][1:].strip())
                i += 1
            if quote:
                first = quote[0]
                if first.startswith("**") and first.endswith("**") is False and first.count("**") >= 2:
                    strong, rest = re.match(r"\*\*([^*]+)\*\*\s*(.*)", first).groups()
                    text = " ".join([rest] + quote[1:]).strip()
                    body.append(
                        f'<div class="callout {css}"><strong>{html.escape(strong)}</strong>'
                        f"<p>{inline(text)}</p></div>"
                    )
                else:
                    body.append(
                        f'<div class="callout {css}"><p>{inline(" ".join(quote))}</p></div>'
                    )
            continue
        if re.match(r"^[-*] ", line):
            flush_para()
            if list_tag != "ul":
                flush_list()
                list_tag = "ul"
            list_items.append(f"<li>{inline(line[2:])}</li>")
            i += 1
            continue
        if heading := re.match(r"^(#{2,3})\s+(.*)", line):
            flush_para()
            flush_list()
            level = len(heading.group(1))
            text = heading.group(2)
            hid = slug(text)
            if level == 2:
                headings.append((hid, text))
            body.append(f"<h{level} id=\"{hid}\">{inline(text)}</h{level}>")
            i += 1
            continue
        if not line.strip():
            flush_para()
            flush_list()
            i += 1
            continue
        para.append(line.strip())
        i += 1
    flush_para()
    flush_list()

    lede = ""
    if html_name == "index.html" and body and body[0].startswith("<p>"):
        lede = body.pop(0)
        lede = lede.replace("<p>", '<p class="lede">')
    elif body and body[0].startswith("<p>"):
        lede = body.pop(0)
        lede = lede.replace("<p>", '<p class="lede">')

    return title, kicker, lede + "\n".join(body), headings


def nav_html(current: str) -> str:
    chunks = [
        '<a class="brand" href="index.html">',
        '<img class="logo" src="drywet.jpg" alt="" width="36" height="36" />',
        '<span class="mark"><span class="dry">dry</span><span class="wet">wet</span></span>',
        '<span class="ver">v1</span></a>',
        '<p class="tagline">Rust library for scheduling and playing musical events on desktop.</p>',
    ]
    for group, pages in NAV_GROUPS:
        chunks.append(f'<div class="nav-label">{group}</div>')
        for page in pages:
            cls = "item current" if page == current else "item"
            chunks.append(f'<a class="{cls}" href="{page}">{html.escape(LABEL[page])}</a>')
    return "\n      ".join(chunks)


def toc_html(headings: list[tuple[str, str]]) -> str:
    if len(headings) < 2:
        return ""
    items = "".join(f'<li><a href="#{hid}">{html.escape(title)}</a></li>' for hid, title in headings)
    return f'<nav class="toc"><strong>On this page</strong><ol>{items}</ol></nav>'


CARD_HEADINGS = {"who-it-is-for", "install-paths"}


def wrap_cards(inner: str) -> str:
    parts = re.split(r"(<h2 id=\"[^\"]+\">.*?</h2>)", inner)
    out: list[str] = []
    i = 0
    while i < len(parts):
        part = parts[i]
        out.append(part)
        hid = re.search(r'<h2 id="([^"]+)"', part)
        if hid and hid.group(1) in CARD_HEADINGS and i + 1 < len(parts):
            following = parts[i + 1]
            cards = re.findall(r"<p><strong>(.*?)</strong>\s*(.*?)</p>", following, flags=re.S)
            if len(cards) >= 2:
                rest = re.sub(r"(<p><strong>.*?</p>\s*)+", "", following, count=1, flags=re.S)
                grid = ['<div class="grid-2">']
                for title, text in cards:
                    grid.append(
                        '<article class="card">'
                        f"<h3>{title}</h3>"
                        f"<p>{text}</p>"
                        "</article>"
                    )
                grid.append("</div>")
                parts[i + 1] = "".join(grid) + rest
        i += 1
    return "".join(out)


def wrap(html_name: str, title: str, kicker: str, inner: str, headings: list[tuple[str, str]]) -> str:
    inner = wrap_cards(inner)
    hero = ""
    if html_name == "index.html":
        # First paragraph already pulled as lede; rebuild overview hero.
        inner_parts = inner.split("</p>", 1)
        if inner_parts[0].startswith('<p class="lede">'):
            lede = inner_parts[0] + "</p>"
            rest = inner_parts[1] if len(inner_parts) > 1 else ""
        else:
            lede = ""
            rest = inner
        hero = f"""      <div class="overview-hero">
        <img class="overview-art" src="drywet.jpg" alt="DryWet mix knob" width="96" height="96" />
        <div>
          <p class="kicker">{html.escape(kicker or "Overview")}</p>
          <h1>{html.escape(title)}</h1>
          {lede}
        </div>
      </div>
      <div class="pills">
        <span class="pill">Rust crate</span>
        <span class="pill">PipeWire callback</span>
        <span class="pill">no Qt / QML</span>
        <span class="pill">vendored engine</span>
        <span class="pill">NDJSON host adapter</span>
      </div>
"""
        inner = rest.replace('<div class="code">', '<div class="code hero-example">', 1)
        toc = ""
    else:
        hero = f"""      <p class="kicker">{html.escape(kicker)}</p>
      <h1>{html.escape(title)}</h1>
"""
        # lede is already at start of inner
        toc = toc_html(headings)

    return f"""<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1" />
  <title>{html.escape(TITLES[html_name])}</title>
  <link rel="stylesheet" href="styles.css" />
  <link rel="preconnect" href="https://fonts.googleapis.com" />
  <link rel="preconnect" href="https://fonts.gstatic.com" crossorigin />
  <link href="https://fonts.googleapis.com/css2?family=Figtree:wght@400;500;600;700&family=Fraunces:opsz,wght@9..144,500;9..144,600&family=IBM+Plex+Mono:ital,wght@0,400;0,500;1,400&display=swap" rel="stylesheet" />
</head>
<body>
  <a class="skip" href="#content">Skip to content</a>
  <button class="menu-btn" type="button" aria-expanded="false">Menu</button>
  <div class="shell">
    <nav class="side" aria-label="Documentation">
      {nav_html(html_name)}
    </nav>
    <main id="content">
{hero}{toc}
{inner}
      <footer class="page">
        {FOOTERS[html_name]}
      </footer>
    </main>
  </div>
  <script src="app.js"></script>
</body>
</html>
"""


def main() -> None:
    for md_name, html_name, _label, _group in PAGES:
        md = (DOCS / md_name).read_text()
        title, kicker, inner, headings = convert(md, html_name)
        (ROOT / html_name).write_text(wrap(html_name, title, kicker, inner, headings))
        print(f"wrote {html_name}")


if __name__ == "__main__":
    main()
