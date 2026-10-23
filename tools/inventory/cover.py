"""Summary of `cargo llvm-cov --json`: the lines the tests run, per crate,
and the largest functions no test runs."""

import json
import os
import re
import sys

ESCAPES = {
    "$SP$": "@", "$BP$": "*", "$RF$": "&", "$LT$": "<", "$GT$": ">",
    "$LP$": "(", "$RP$": ")", "$C$": ",", "$u7e$": "~", "$u20$": " ",
    "$u27$": "'", "$u5b$": "[", "$u5d$": "]", "$u7b$": "{", "$u7d$": "}",
    "$u3b$": ";", "$u2b$": "+", "$u22$": '"', "..": "::",
}


def demangle(name):
    match = re.match(r"_ZN(.*)E$", name)
    if not match:
        return name
    rest, parts = match.group(1), []
    while rest and rest[0].isdigit():
        length = re.match(r"\d+", rest).group(0)
        start = len(length)
        parts.append(rest[start:start + int(length)])
        rest = rest[start + int(length):]
    if parts and re.fullmatch(r"h[0-9a-f]{16}", parts[-1]):
        parts.pop()
    text = "::".join(parts)
    for code, char in ESCAPES.items():
        text = text.replace(code, char)
    return text.replace("_$", "$").lstrip("_")


def crate(path):
    parts = os.path.relpath(path).split(os.sep)
    if parts[0] in ("crates", "experiments") and len(parts) > 1:
        return parts[1]
    return "zyterm"


def main():
    data = json.load(open(sys.argv[1]))["data"][0]
    top = int(sys.argv[2])
    root = os.getcwd() + os.sep
    totals = {}
    for file in data["files"]:
        if not file["filename"].startswith(root):
            continue
        lines = file["summary"]["lines"]
        total = totals.setdefault(crate(file["filename"]), [0, 0])
        total[0] += lines["covered"]
        total[1] += lines["count"]
    print(f"{'crate':24} {'run':>7} {'lines':>7}     %")
    for name, (covered, count) in sorted(totals.items(), key=lambda item: item[1][0] - item[1][1]):
        print(f"{name:24} {covered:7} {count:7} {100 * covered / max(count, 1):5.1f}")
    functions = {}
    for function in data["functions"]:
        name = function["filenames"][0]
        if not name.startswith(root) or not function["regions"]:
            continue
        start = function["regions"][0][0]
        end = max(region[2] for region in function["regions"])
        key = (os.path.relpath(name), start)
        runs, _, _ = functions.get(key, (0, 0, ""))
        functions[key] = (runs + function["count"], end - start + 1, demangle(function["name"]))
    cold = [(lines, name, start, text)
            for (name, start), (runs, lines, text) in functions.items() if runs == 0]
    cold.sort(reverse=True)
    print(f"\n{len(cold)} functions no test runs, {sum(c[0] for c in cold)} lines; the largest:")
    for lines, name, start, function in cold[:top]:
        print(f"{lines:5} {name}:{start} {function}")


main()
