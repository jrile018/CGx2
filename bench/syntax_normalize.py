"""Public, narrow normalization shared by both syntax-pilot conditions."""

import re

FUNCTION = re.compile(r"fn [A-Za-z_][A-Za-z_0-9]*\([^{};]*\) -> i64")

def normalize_brace(source):
    """Accept only the packet's one-level function braces and terminators."""
    if not source.endswith("\n") or "\t" in source:
        raise ValueError("brace source needs final newline and no tabs")
    lines = []
    in_function = False
    for raw in source.splitlines():
        if not raw:
            if in_function:
                raise ValueError("blank lines inside brace functions are unsupported")
            lines.append("")
        elif raw == "}":
            if not in_function:
                raise ValueError("unmatched closing brace")
            in_function = False
        elif raw.endswith(" {"):
            if in_function or not FUNCTION.fullmatch(raw[:-2]):
                raise ValueError("unsupported brace block")
            lines.append(raw[:-2] + ":")
            in_function = True
        else:
            prefix = "    " if in_function else ""
            if not raw.startswith(prefix) or raw.startswith("     "):
                raise ValueError("invalid brace indentation")
            statement = raw[len(prefix):]
            if not statement.endswith(";") or any(c in statement for c in "{}"):
                raise ValueError("unsupported brace statement")
            lines.append(prefix + statement[:-1])
    if in_function:
        raise ValueError("unclosed function")
    while lines and lines[-1] == "":
        lines.pop()
    return "\n".join(lines) + "\n"
