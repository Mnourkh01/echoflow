//! Offline restoration of Latin diacritics on English output.
//!
//! Whisper transcribing English routinely strips the accent off common loanwords
//! ("cafe", "resume-as-CV", "naive"), which reads as sloppy in European / formal
//! writing. We put the marks back with a small, deterministic dictionary — no AI
//! call, no network — so it works in raw mode and stays offline.
//!
//! The list is deliberately conservative: it excludes words whose unaccented form
//! is itself a valid, common English word (resume/expose/rose/pate/sake), because
//! "let's resume the call" must never become "let's résumé the call".

/// Restore accents on known loanwords in `text`, preserving each word's casing
/// and all surrounding punctuation / spacing.
pub fn restore_diacritics(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 8);
    let mut word = String::new();
    for ch in text.chars() {
        if ch.is_ascii_alphabetic() {
            word.push(ch);
        } else {
            if !word.is_empty() {
                out.push_str(&restore_word(&word));
                word.clear();
            }
            out.push(ch);
        }
    }
    if !word.is_empty() {
        out.push_str(&restore_word(&word));
    }
    out
}

fn restore_word(word: &str) -> String {
    match lookup(&word.to_ascii_lowercase()) {
        Some(accented) => match_case(word, accented),
        None => word.to_string(),
    }
}

/// Plain ASCII (lowercased) -> properly accented form. Only unambiguous loanwords.
fn lookup(lower: &str) -> Option<&'static str> {
    Some(match lower {
        "cafe" => "café",
        "cafes" => "cafés",
        "naive" => "naïve",
        "naivete" => "naïveté",
        "naively" => "naïvely",
        "cliche" => "cliché",
        "cliches" => "clichés",
        "fiance" => "fiancé",
        "fiancee" => "fiancée",
        "entree" => "entrée",
        "entrees" => "entrées",
        "facade" => "façade",
        "facades" => "façades",
        "jalapeno" => "jalapeño",
        "jalapenos" => "jalapeños",
        "pinata" => "piñata",
        "pinatas" => "piñatas",
        "souffle" => "soufflé",
        "souffles" => "soufflés",
        "saute" => "sauté",
        "sauteed" => "sautéed",
        "sauteing" => "sautéing",
        "seance" => "séance",
        "seances" => "séances",
        "touche" => "touché",
        "voila" => "voilà",
        "deja" => "déjà", // "déjà vu"
        "creme" => "crème",
        "brulee" => "brûlée",
        "protege" => "protégé",
        "proteges" => "protégés",
        "protegee" => "protégée",
        "soiree" => "soirée",
        "soirees" => "soirées",
        "matinee" => "matinée",
        "matinees" => "matinées",
        "canape" => "canapé",
        "canapes" => "canapés",
        "frappe" => "frappé",
        "frappes" => "frappés",
        "attache" => "attaché",
        "communique" => "communiqué",
        "communiques" => "communiqués",
        "consomme" => "consommé",
        "risque" => "risqué",
        "decor" => "décor",
        "vis-a-vis" => "vis-à-vis", // never reached (hyphen splits words), kept for intent
        _ => return None,
    })
}

/// Apply `original`'s casing to `accented`: ALLCAPS, Titlecase, or lowercase.
fn match_case(original: &str, accented: &str) -> String {
    if original.len() > 1 && original.chars().all(|c| c.is_ascii_uppercase()) {
        return accented.to_uppercase();
    }
    let first_upper = original
        .chars()
        .next()
        .map(|c| c.is_ascii_uppercase())
        .unwrap_or(false);
    if first_upper {
        let mut chars = accented.chars();
        match chars.next() {
            Some(f) => f.to_uppercase().collect::<String>() + chars.as_str(),
            None => accented.to_string(),
        }
    } else {
        accented.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restores_common_loanwords() {
        assert_eq!(restore_diacritics("a nice cafe"), "a nice café");
        assert_eq!(restore_diacritics("Cafe Latte"), "Café Latte");
        assert_eq!(restore_diacritics("that is so naive."), "that is so naïve.");
        assert_eq!(restore_diacritics("deja vu"), "déjà vu");
    }

    #[test]
    fn leaves_ambiguous_english_words_alone() {
        assert_eq!(restore_diacritics("let's resume the call"), "let's resume the call");
        assert_eq!(restore_diacritics("expose the bug"), "expose the bug");
    }

    #[test]
    fn preserves_caps_and_punctuation() {
        assert_eq!(restore_diacritics("CAFE!"), "CAFÉ!");
        assert_eq!(restore_diacritics("(facade)"), "(façade)");
    }
}
