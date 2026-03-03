use lazy_static::lazy_static;
use regex::{Captures, Regex};
use tracing::debug;

struct Rule {
    pattern: Regex,
    replacement: String,
    description: String,
}

impl Rule {
    fn new(pattern: &str, replacement: &str, description: &str) -> Self {
        Self {
            pattern: Regex::new(&format!("(?i){}", pattern))
                .expect("Invalid regex in static rules"),
            replacement: replacement.to_string(),
            description: description.to_string(),
        }
    }
}

lazy_static! {
    static ref PL_RULES: Vec<Rule> = vec![
        Rule::new(r"\bna\s+prawdę\b", "naprawdę", "na prawdę -> naprawdę"),
        Rule::new(
            r"\bna\s+przeciwko\b",
            "naprzeciwko",
            "na przeciwko -> naprzeciwko"
        ),
        Rule::new(r"\bpo\s+nad\s+to\b", "ponadto", "po nad to -> ponadto"),
        Rule::new(r"\bpo\s+mimo\b", "pomimo", "po mimo -> pomimo"),
        Rule::new(r"\bdla\s+tego\b", "dlatego", "dla tego -> dlatego"),
        Rule::new(r"\bpo\s+nie\s+waż\b", "ponieważ", "po nie waż -> ponieważ"),
        Rule::new(r"\bnapewno\b", "na pewno", "napewno -> na pewno"),
        Rule::new(r"\bwogóle\b", "w ogóle", "wogóle -> w ogóle"),
        Rule::new(r"\bnarazie\b", "na razie", "narazie -> na razie"),
        Rule::new(
            r"\bconajmniej\b",
            "co najmniej",
            "conajmniej -> co najmniej"
        ),
        Rule::new(r"\bpoprostu\b", "po prostu", "poprostu -> po prostu"),
        Rule::new(
            r"\bprzedewszystkim\b",
            "przede wszystkim",
            "przedewszystkim -> przede wszystkim"
        ),
    ];
    // Note: Rust regex crate does not support lookaheads (?=...).
    // Patterns that need context matching capture the following word and preserve it.
    static ref EN_RULES: Vec<Rule> = vec![
        Rule::new(
            r"\byour(\s+(?:going|doing|being|making|getting|coming|running|saying|looking|trying|giving|taking|having|welcome|not|right|wrong))",
            "you're${1}",
            "your + verb -> you're"
        ),
        Rule::new(
            r"\bits(\s+(?:going|doing|being|getting|making|coming|running|not|been|just|about|really|very|always|never|still|already|only|also|a\b|the\b|so\b|ok\b|okay\b|true\b|possible\b|impossible\b|important\b))",
            "it's${1}",
            "its + verb/adv -> it's"
        ),
        Rule::new(
            r"\bthere(\s+(?:going|doing|being|making|getting|coming|running|saying|looking|trying|giving|playing|telling|leaving|taking|having|showing|not|always|never|just|really|still|already))",
            "they're${1}",
            "there + verb -> they're"
        ),
        Rule::new(
            r"\btheir(\s+(?:going|doing|being|making|getting|coming|running|saying|looking|trying|giving|playing|telling|leaving|taking|having|showing|not|always|never|just|really|still|already))",
            "they're${1}",
            "their + verb -> they're"
        ),
        Rule::new(
            r"\bwhose(\s+(?:going|doing|being|making|getting|coming|running|not|been|there|here))",
            "who's${1}",
            "whose + verb -> who's"
        ),
        Rule::new(
            r"\bweather(\s+(?:or|it|you|we|they|he|she|to\b|not\b))",
            "whether${1}",
            "weather + clause -> whether"
        ),
        Rule::new(
            r"\b(more|less|better|worse|bigger|smaller|larger|higher|lower|faster|slower|older|younger|harder|easier|rather|other)\s+then\b",
            "${1} than",
            "comparative + then -> than"
        ),
        Rule::new(
            r"\b(the|an?|no|any|this|that|its|positive|negative)\s+affect\b",
            "${1} effect",
            "article + affect -> effect"
        ),
        Rule::new(
            r"\b(will|would|could|can|may|might|to|not)\s+effect\b",
            "${1} affect",
            "modal + effect -> affect"
        ),
        Rule::new(
            r"\b(to|will|would|could|gonna|might|can|don't|didn't|won't|cannot)\s+loose\b",
            "${1} lose",
            "verb context + loose -> lose"
        ),
        Rule::new(
            r"\b(would|could|should|might|must)\s+of\b",
            "${1} have",
            "modal + of -> have"
        ),
        Rule::new(r"\balot\b", "a lot", "alot -> a lot"),
        Rule::new(
            r"\b(is|are|was|were|am|be|been)\s+to(\s+(?:big|small|large|much|many|few|little|hard|easy|late|early|fast|slow|long|short|hot|cold|old|young|good|bad|high|low|far|close|loud|quiet|expensive|cheap|difficult|simple))",
            "${1} too${2}",
            "copula + to + adj -> too"
        ),
    ];
}

fn preserve_case_replacement<'a>(text: &'a str, replacement: &'a str) -> String {
    // Basic case preservation: match first char case
    if text
        .chars()
        .next()
        .map(|c| c.is_uppercase())
        .unwrap_or(false)
    {
        // If replacement has backreferences (like $1), we shouldn't capitalize easily here without expansion
        // But for literal replacements, we can.
        // The regex crate's `replace_all` handles expansion.
        // We need a custom logic to handle case preservation *after* expansion?
        // Actually, Python code does `_preserve_case_replacement` which checks original match.
        // Rust's regex replace takes a closure.
        let mut chars = replacement.chars();
        match chars.next() {
            Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
            None => replacement.to_string(),
        }
    } else {
        replacement.to_string()
    }
}

pub fn apply_corrections(text: &str, lang: &str) -> String {
    let rules = if lang == "pl" { &*PL_RULES } else { &*EN_RULES };
    let mut result = text.to_string();

    for rule in rules.iter() {
        // If replacement string indicates backreferences, use standard regex replacement
        // which handles group expansion ($1, $2, etc.).
        if rule.replacement.contains('$') || rule.replacement.contains('\\') {
            let processed = rule.pattern.replace_all(&result, &rule.replacement);
            if processed != result {
                debug!("Word correction: {}", rule.description);
                result = processed.to_string();
            }
        } else {
            // Otherwise, use closure to allow case preservation based on the match
            let processed = rule.pattern.replace_all(&result, |caps: &Captures| {
                let m = caps.get(0).unwrap().as_str();
                preserve_case_replacement(m, &rule.replacement)
            });
            if processed != result {
                debug!("Word correction: {}", rule.description);
                result = processed.to_string();
            }
        }
    }
    result
}
