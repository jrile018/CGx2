pub(crate) fn python_name(name: &str) -> String {
    let ascii_identifier = name
        .bytes()
        .enumerate()
        .all(|(i, b)| b.is_ascii_alphanumeric() && (i != 0 || !b.is_ascii_digit()) || b == b'_');
    if ascii_identifier
        && !name.starts_with("_cgx2_")
        && !name.starts_with("__")
        && !is_keyword(name)
    {
        return name.to_owned();
    }
    let mut encoded = String::from("_cgx2_user_");
    for byte in name.bytes() {
        encoded.push_str(&format!("{byte:02x}"));
    }
    encoded
}

fn is_keyword(name: &str) -> bool {
    matches!(
        name,
        "False"
            | "None"
            | "True"
            | "and"
            | "as"
            | "assert"
            | "async"
            | "await"
            | "break"
            | "case"
            | "class"
            | "continue"
            | "def"
            | "del"
            | "elif"
            | "else"
            | "except"
            | "finally"
            | "for"
            | "from"
            | "global"
            | "if"
            | "import"
            | "in"
            | "is"
            | "lambda"
            | "match"
            | "nonlocal"
            | "not"
            | "or"
            | "pass"
            | "raise"
            | "return"
            | "try"
            | "while"
            | "with"
            | "yield"
    )
}
