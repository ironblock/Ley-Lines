#!/usr/bin/env python3
"""How large rows for a working tree would be, from a tree and an earlier copy of it. No git, no model.

    python3 design/sizes.py BEFORE NOW [--tests DIR ...] [--show names|shape|region|whole|none]

BEFORE and NOW are two directories, such as two commits exported with `git archive`. It prints the size of what
a row would carry at each level, in bytes and in tokens estimated as bytes / 3.5:

  names    each changed file: new, changed or gone, its lines now, lines added and taken out
  shape    + each declaration added, changed or gone, with its first line and its lines now
  region   + the text of each added or changed declaration as it is now
  diff     names + the changed lines with three lines round each, for comparison
  whole    names + each changed file whole
  tests    with --tests: for each added or changed declaration, the test files that name it

A prototype, written to measure, not to ship. Its limits: it lists files with `rg --files`, so it leaves out hidden
files and what a .gitignore leaves out; a file that is not UTF-8 text is named and not measured; declarations are
what `ast-grep outline --items structure` reports, and a file it reports none for is counted as whole.
"""
import argparse, difflib, hashlib, json, os, re, subprocess, sys

def files_of(root):
    out = subprocess.run(["rg", "--files", "--no-config", "."], cwd=root, capture_output=True, text=True).stdout
    return sorted(p[2:] if p.startswith("./") else p for p in out.split("\n") if p)

def read(root, rel):
    try:
        return open(os.path.join(root, rel), encoding="utf-8").read()
    except (OSError, UnicodeDecodeError):
        return None

def outline(root, rel):
    r = subprocess.run(["ast-grep", "outline", rel, "--items", "structure", "--json=compact"], cwd=root, capture_output=True, text=True)
    try:
        got = json.loads(r.stdout)
    except ValueError:
        return None
    return got[0]["items"] if got else []

def decls(items, text):
    """key -> (first line of source, start, end, hash of its text), top level and members."""
    lines = text.split("\n")
    out = {}
    def add(key, it):
        a, b = it["range"]["start"]["line"], it["range"]["end"]["line"]
        body = "\n".join(lines[a:b + 1])
        n = 2
        k = key
        while k in out:
            k = f"{key}#{n}"; n += 1
        out[k] = (lines[a].strip(), a + 1, b + 1, hashlib.sha256(body.encode()).hexdigest(), body)
    for it in items:
        name = it.get("name") or "?"
        members = it.get("members") or []
        if members:
            for m in members:
                add(f"{name}.{m.get('name') or '?'}", m)
            # the owner itself: its first line only, so a change to a member does not also mark the owner
            a = it["range"]["start"]["line"]
            out[name] = (lines[a].strip(), a + 1, it["range"]["end"]["line"] + 1, hashlib.sha256(lines[a].encode()).hexdigest(), lines[a])
        else:
            add(name, it)
    return out

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("before"); ap.add_argument("now"); ap.add_argument("--tests", nargs="*", default=[])
    ap.add_argument("--show", choices=["names", "shape", "region", "whole", "none"], default="shape")
    a = ap.parse_args()
    fb, fn = set(files_of(a.before)), set(files_of(a.now))
    rows = {"names": [], "shape": [], "region": [], "whole": [], "tests": [], "diff": []}
    nfiles = 0
    for rel in sorted(fb | fn):
        tb = read(a.before, rel) if rel in fb else None
        tn = read(a.now, rel) if rel in fn else None
        if rel in fb and rel in fn and tb == tn:
            continue
        if (rel in fb and tb is None) or (rel in fn and tn is None):
            rows["names"].append(f"{rel}: not text, {'new' if rel not in fb else 'gone' if rel not in fn else 'changed'}")
            continue
        nfiles += 1
        if rel not in fn:
            rows["names"].append(f"{rel}: gone ({tb.count(chr(10))} lines)")
            continue
        status = "new" if rel not in fb else "changed"
        sm = difflib.SequenceMatcher(None, (tb or "").split("\n"), tn.split("\n"), autojunk=False)
        added = sum(j2 - j1 for tag, i1, i2, j1, j2 in sm.get_opcodes() if tag in ("replace", "insert"))
        gone = sum(i2 - i1 for tag, i1, i2, j1, j2 in sm.get_opcodes() if tag in ("replace", "delete"))
        rows["names"].append(f"{rel}: {status}, {tn.count(chr(10))} lines" + (f", +{added} -{gone}" if status == "changed" else ""))
        rows["diff"].append("".join(difflib.unified_diff((tb or "").splitlines(True), tn.splitlines(True), "before/" + rel, "now/" + rel, n=3)))
        on = outline(a.now, rel)
        if on is None or (not on and not rel.endswith((".py", ".rs", ".ts", ".tsx", ".js", ".go", ".java", ".c", ".h", ".cpp"))):
            rows["whole"].append(f"FILE {rel}\n{tn}")
            continue
        dn = decls(on, tn)
        db = decls(outline(a.before, rel) or [], tb) if tb is not None else {}
        for key in sorted(set(dn) | set(db), key=lambda k: (dn.get(k, db.get(k))[1], k)):
            if key in dn and key in db and dn[key][3] == db[key][3]:
                continue
            if key not in dn:
                rows["shape"].append(f"  - {key}: gone (was: {db[key][0][:100]})")
                continue
            first, s, e, _, body = dn[key]
            word = "added" if key not in db else "changed"
            rows["shape"].append(f"  {'+' if word == 'added' else '~'} {rel}:{s}-{e} {first[:140]}")
            if "\n" in body or word == "added":
                rows["region"].append(f"{rel}:{s}-{e}\n{body}")
            if a.tests:
                name = key.split(".")[-1].split("#")[0]
                if re.fullmatch(r"[A-Za-z_]\w{3,}", name):
                    r = subprocess.run(["rg", "-l", "-w", "-F", "--no-config", name] + a.tests, cwd=a.now, capture_output=True, text=True)
                    found = [f for f in r.stdout.split("\n") if f and f != rel]
                    rows["tests"].append(f"  {name}: " + (", ".join(found[:6]) + (f" and {len(found) - 6} more" if len(found) > 6 else "") if found else "named in no test file"))
        rows["whole"].append(f"FILE {rel}\n{tn}")
    size = lambda parts: sum(len(p.encode()) + 1 for p in parts)
    names, shape, region, whole, tests = (size(rows[k]) for k in ("names", "shape", "region", "whole", "tests"))
    print(f"changed files: {nfiles}; declarations added, changed or gone: {len(rows['shape'])}")
    print(f"  names   {names:>8} bytes  ~{round(names / 3.5):>6} tokens")
    print(f"  shape   {names + shape:>8} bytes  ~{round((names + shape) / 3.5):>6} tokens   (names + first lines of what changed)")
    print(f"  region  {names + shape + region:>8} bytes  ~{round((names + shape + region) / 3.5):>6} tokens   (+ text of each added or changed declaration)")
    diff = size(rows["diff"])
    print(f"  diff    {names + diff:>8} bytes  ~{round((names + diff) / 3.5):>6} tokens   (names + the changed lines with three lines round each)")
    print(f"  whole   {names + whole:>8} bytes  ~{round((names + whole) / 3.5):>6} tokens   (names + every changed file whole)")
    if a.tests:
        print(f"  tests   {tests:>8} bytes  ~{round(tests / 3.5):>6} tokens   (for each declaration, the test files that name it)")
    if a.show != "none":
        print("\n".join(rows["names"]))
        if a.show in ("shape", "region", "whole"):
            print("\n".join(rows["shape"]))
        if a.tests:
            print("\n".join(rows["tests"][:40]))
main()
