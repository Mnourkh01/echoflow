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

/// Inline voice commands. In raw dictation the user can SAY punctuation and line
/// breaks ("new line", "period", "comma", "question mark"...) and we convert them
/// to the real characters — English + Arabic. Deliberately conservative: only a
/// fixed set of unambiguous command phrases is recognized, everything else is
/// left exactly as spoken. `lang` gates sentence capitalization (Latin only;
/// Arabic has no letter case).
pub fn apply_voice_commands(text: &str, lang: &str) -> String {
    let tokens: Vec<&str> = text.split_whitespace().collect();
    if tokens.is_empty() {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < tokens.len() {
        // Prefer a two-word command ("new line", "question mark") over one word.
        if i + 1 < tokens.len() {
            let phrase = format!("{} {}", norm(tokens[i]), norm(tokens[i + 1]));
            if let Some(cmd) = command(&phrase) {
                apply_cmd(&mut out, cmd);
                i += 2;
                continue;
            }
        }
        if let Some(cmd) = command(&norm(tokens[i])) {
            apply_cmd(&mut out, cmd);
            i += 1;
            continue;
        }
        push_word(&mut out, tokens[i]);
        i += 1;
    }
    let out = out.trim().to_string();
    if lang.starts_with("ar") {
        out
    } else {
        capitalize_sentences(&out)
    }
}

/// Lowercase + strip surrounding non-alphanumerics for command matching.
fn norm(token: &str) -> String {
    token
        .trim_matches(|c: char| !c.is_alphanumeric())
        .to_lowercase()
}

enum Cmd {
    Punct(&'static str),
    Break(&'static str),
}

fn command(phrase: &str) -> Option<Cmd> {
    Some(match phrase {
        "new paragraph" | "فقرة جديدة" => Cmd::Break("\n\n"),
        "new line" | "newline" | "سطر جديد" => Cmd::Break("\n"),
        "full stop" | "period" | "نقطة" => Cmd::Punct("."),
        "comma" => Cmd::Punct(","),
        "فاصلة" => Cmd::Punct("،"), // Arabic comma
        "question mark" => Cmd::Punct("?"),
        "علامة استفهام" => Cmd::Punct("؟"), // Arabic question mark
        "exclamation mark" | "exclamation point" | "علامة تعجب" => Cmd::Punct("!"),
        "colon" | "نقطتان" => Cmd::Punct(":"),
        "semicolon" => Cmd::Punct(";"),
        _ => return None,
    })
}

fn apply_cmd(out: &mut String, cmd: Cmd) {
    // Both punctuation and breaks hug the previous word: drop a trailing space.
    while out.ends_with(' ') {
        out.pop();
    }
    match cmd {
        Cmd::Punct(p) => out.push_str(p),
        Cmd::Break(b) => out.push_str(b),
    }
}

fn push_word(out: &mut String, word: &str) {
    // Space before a word unless we're at the start or just emitted whitespace
    // (e.g. a line break). After punctuation the last char isn't whitespace, so a
    // space is added: "word. word".
    if !out.is_empty() && !out.ends_with(|c: char| c.is_whitespace()) {
        out.push(' ');
    }
    out.push_str(word);
}

/// Capitalize the first letter and the first letter after each sentence ender
/// (. ? !) or newline. ASCII-focused; a no-op on scripts without case.
fn capitalize_sentences(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut cap_next = true;
    for ch in text.chars() {
        if cap_next && ch.is_alphabetic() {
            out.extend(ch.to_uppercase());
            cap_next = false;
        } else {
            out.push(ch);
            if ch == '.' || ch == '?' || ch == '!' || ch == '\n' {
                cap_next = true;
            } else if !ch.is_whitespace() {
                cap_next = false;
            }
            // plain spaces keep cap_next as-is, so ". next" still capitalizes.
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn voice_commands_punctuation_and_caps() {
        assert_eq!(apply_voice_commands("hello period world", "en"), "Hello. World");
        assert_eq!(apply_voice_commands("really question mark", "en"), "Really?");
        assert_eq!(apply_voice_commands("wait comma what", "en"), "Wait, what");
    }

    #[test]
    fn voice_commands_line_breaks() {
        assert_eq!(
            apply_voice_commands("first line new line second line", "en"),
            "First line\nSecond line"
        );
        assert_eq!(
            apply_voice_commands("a new paragraph b", "en"),
            "A\n\nB"
        );
    }

    #[test]
    fn voice_commands_arabic_no_capitalization() {
        assert_eq!(
            apply_voice_commands("مرحبا نقطة سطر جديد كيف حالك", "ar"),
            "مرحبا.\nكيف حالك"
        );
    }

    #[test]
    fn voice_commands_leave_plain_text() {
        // No command words: only the first letter is capitalized (Latin).
        assert_eq!(apply_voice_commands("just some normal text", "en"), "Just some normal text");
        // Arabic plain text is returned untouched.
        assert_eq!(apply_voice_commands("نص عادي بدون أوامر", "ar"), "نص عادي بدون أوامر");
    }

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
