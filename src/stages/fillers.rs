use regex::Regex;
use lazy_static::lazy_static;

lazy_static! {
    static ref PL_FILLERS: Regex = Regex::new(
        r"(?i)\b(?:no więc|no właśnie|no wiesz|no nie|no tak|tak jakby|w zasadzie|yyy+|eee+|hmm+|hm+|no|znaczy|jakby|wiesz|powiedzmy|generalnie|kurczę)\b"
    ).unwrap();

    static ref EN_FILLERS: Regex = Regex::new(
        r"(?i)\b(?:um+|uh+|hmm+|hm+|like|you know|basically|actually|literally|i mean|sort of|kind of|right)\b"
    ).unwrap();
    
    static ref MULTI_SPACE: Regex = Regex::new(r"  +").unwrap();
}

pub fn remove_fillers(text: &str, lang: &str) -> String {
    let pattern = if lang == "pl" { &*PL_FILLERS } else { &*EN_FILLERS };
    let result = pattern.replace_all(text, "");
    MULTI_SPACE.replace_all(&result, " ").trim().to_string()
}
