"""Regenerate the two Wave 13 candidate starter trees and reference overlays.

This is fixture construction, not a model runner. Private overlays stay outside
the starter trees. Run from any directory with Python 3.
"""

from pathlib import Path


ROOT = Path(__file__).resolve().parent


def put(relative, content):
    path = ROOT / relative
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(content, encoding="utf-8")


def make_small():
    task = "tasks/quote-credit-v3"
    cg = f"{task}/cgx2"
    rs = f"{task}/rust"
    put(f"{cg}/cgx2.toml", 'schema_version = 1\nname = "quote_credit"\nentry = "src/main.cg"\nsources = ["src/main.cg", "src/pricing.cg", "src/adjust.cg"]\n')
    put(f"{cg}/src/main.cg", "module quote.main\nimport quote.pricing\nfn main() -> i64:\n    quote.pricing.quote(7, 120, 18, 34)\n")
    put(f"{cg}/src/pricing.cg", "module quote.pricing\nimport quote.adjust\npub fn quote(units: i64, unit_price: i64, shipping: i64, credit: i64) -> i64:\n    quote.adjust.finalize(units * unit_price, shipping, credit)\n")
    put(f"{cg}/src/adjust.cg", "module quote.adjust\npub fn finalize(subtotal: i64, shipping: i64, credit: i64) -> i64:\n    subtotal + shipping + credit\n")
    put(f"{rs}/main.rs", "mod pricing;\nmod adjust;\nfn main() { println!(\"{}\", pricing::quote(7, 120, 18, 34)); }\n")
    put(f"{rs}/pricing.rs", "pub fn quote(units: i64, unit_price: i64, shipping: i64, credit: i64) -> i64 {\n    adjust::finalize(units.checked_mul(unit_price).expect(\"i64 arithmetic overflow\"), shipping, credit)\n}\nuse crate::adjust;\n")
    put(f"{rs}/adjust.rs", "pub fn finalize(subtotal: i64, shipping: i64, credit: i64) -> i64 {\n    subtotal.checked_add(shipping).and_then(|n| n.checked_add(credit)).expect(\"i64 arithmetic overflow\")\n}\n")
    put(f"{rs}/README.md", "# Quote credit repair\nRun `rustc --edition=2024 main.rs -o quote` from this directory. Public functions mirror the CGx2 starter.\n")
    put("private/quote-credit-v3/cgx2/adjust.cg", "module quote.adjust\npub fn finalize(subtotal: i64, shipping: i64, credit: i64) -> i64:\n    subtotal + shipping - credit\n")
    put("private/quote-credit-v3/rust/adjust.rs", "pub fn finalize(subtotal: i64, shipping: i64, credit: i64) -> i64 {\n    subtotal.checked_add(shipping).and_then(|n| n.checked_sub(credit)).expect(\"i64 arithmetic overflow\")\n}\n")


LEAVES = {
    "generation": [("solar", 2), ("wind", 3), ("hydro", 5), ("gas", 7), ("coal", 11)],
    "transmission": [("line", 13), ("transformer", 17), ("switch", 19)],
    "distribution": [("local", 23), ("meter", 29), ("connection", 31)],
    "market": [("reserve", 37), ("balancing", 41), ("settlement", 43)],
}


def group_cg(name, leaves, repaired):
    imports = "".join(f"import grid.{leaf}\n" for leaf, _ in leaves)
    # The old adapter incorrectly rescales the normalized usage in one call per group.
    terms = [f"grid.{leaf}.charge({'units' if repaired or i else 'units * 2'})" for i, (leaf, _) in enumerate(leaves)]
    return f"module grid.{name}\n{imports}pub fn charge(units: i64) -> i64:\n    " + " + ".join(terms) + "\n"


def group_rs(name, leaves, repaired):
    terms = [f"{leaf}::charge({'units' if repaired or i else 'units.checked_mul(2).expect(\"i64 arithmetic overflow\")'})" for i, (leaf, _) in enumerate(leaves)]
    chain = terms[0] + "\n" + "".join(f"        .checked_add({term}).expect(\"i64 arithmetic overflow\")\n" for term in terms[1:])
    imports = "".join(f"use crate::{leaf};\n" for leaf, _ in leaves)
    return f"{imports}pub fn charge(units: i64) -> i64 {{\n    {chain}\n}}\n"


def make_large():
    task = "tasks/grid-quote-v3"
    cg = f"{task}/cgx2"
    rs = f"{task}/rust"
    groups = list(LEAVES)
    leaves = [leaf for items in LEAVES.values() for leaf, _ in items]
    modules = ["main", "quote", *groups, *leaves]
    sources = ", ".join(f'"src/{name}.cg"' for name in modules)
    put(f"{cg}/cgx2.toml", f'schema_version = 1\nname = "grid_quote"\nentry = "src/main.cg"\nsources = [{sources}]\n')
    put(f"{cg}/src/main.cg", "module grid.main\nimport grid.quote\nfn main() -> i64:\n    grid.quote.total(10)\n")
    put(f"{cg}/src/quote.cg", "module grid.quote\n" + "".join(f"import grid.{g}\n" for g in groups) + "pub fn total(units: i64) -> i64:\n    " + " + ".join(f"grid.{g}.charge(units)" for g in groups) + "\n")
    put(f"{rs}/main.rs", "mod quote;\n" + "".join(f"mod {g};\n" for g in groups) + "".join(f"mod {leaf};\n" for leaf in leaves) + 'fn main() { println!("{}", quote::total(10)); }\n')
    rs_terms = [f"{g}::charge(units)" for g in groups]
    put(f"{rs}/quote.rs", "".join(f"use crate::{g};\n" for g in groups) + "pub fn total(units: i64) -> i64 {\n    " + rs_terms[0] + "\n" + "".join(f"        .checked_add({term}).expect(\"i64 arithmetic overflow\")\n" for term in rs_terms[1:]) + "}\n")
    for group, items in LEAVES.items():
        put(f"{cg}/src/{group}.cg", group_cg(group, items, False))
        put(f"{rs}/{group}.rs", group_rs(group, items, False))
        put(f"private/grid-quote-v3/cgx2/{group}.cg", group_cg(group, items, True))
        put(f"private/grid-quote-v3/rust/{group}.rs", group_rs(group, items, True))
        for leaf, rate in items:
            put(f"{cg}/src/{leaf}.cg", f"module grid.{leaf}\npub fn charge(units: i64) -> i64:\n    units * {rate}\n")
            put(f"{rs}/{leaf}.rs", f"pub fn charge(units: i64) -> i64 {{ units.checked_mul({rate}).expect(\"i64 arithmetic overflow\") }}\n")


if __name__ == "__main__":
    make_small()
    make_large()
