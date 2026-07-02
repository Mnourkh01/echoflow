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

// ── Custom vocabulary post-correction ───────────────────────────────────────
//
// Whisper's initial prompt only *biases* the decoder toward the user's terms;
// it rarely fixes spelling/casing of names on its own ("echo flow" stays
// lowercase and split). This pass makes the vocabulary feature actually land:
// after decoding, transcript words are fuzzy-matched against the vocab terms
// and rewritten to the exact user spelling. Deterministic, offline, and
// deliberately conservative so common words are never hijacked.

/// Split the user's vocabulary setting into clean terms (comma / newline /
/// semicolon separated). Shared by the prompt builder, the post-correction
/// pass, and the enhance system prompts.
pub fn vocab_terms(vocab: &str) -> Vec<&str> {
    vocab
        .split(|c| c == ',' || c == '\n' || c == ';' || c == '،' || c == '؛')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect()
}

/// A word in the source text: [start, end) byte span + lowercase form.
struct Word {
    start: usize,
    end: usize,
    lower: String,
}

/// Unicode-aware word split (runs of alphanumeric chars), keeping byte spans so
/// replacements can splice the original string precisely.
fn words_of(text: &str) -> Vec<Word> {
    let mut out = Vec::new();
    let mut start: Option<usize> = None;
    for (i, ch) in text.char_indices() {
        if ch.is_alphanumeric() {
            if start.is_none() {
                start = Some(i);
            }
        } else if let Some(s) = start.take() {
            out.push(Word {
                start: s,
                end: i,
                lower: text[s..i].to_lowercase(),
            });
        }
    }
    if let Some(s) = start {
        out.push(Word {
            start: s,
            end: text.len(),
            lower: text[s..].to_lowercase(),
        });
    }
    out
}

/// Levenshtein distance over chars, early-exiting when it must exceed `cap`.
fn levenshtein(a: &str, b: &str, cap: usize) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    if a.len().abs_diff(b.len()) > cap {
        return cap + 1;
    }
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut cur = vec![0usize; b.len() + 1];
    for (i, &ca) in a.iter().enumerate() {
        cur[0] = i + 1;
        let mut row_min = cur[0];
        for (j, &cb) in b.iter().enumerate() {
            let cost = if ca == cb { 0 } else { 1 };
            cur[j + 1] = (prev[j] + cost).min(prev[j + 1] + 1).min(cur[j] + 1);
            row_min = row_min.min(cur[j + 1]);
        }
        if row_min > cap {
            return cap + 1;
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[b.len()]
}

/// Edit-distance budget for a term of `n` chars. Short terms must match
/// exactly (too easy to collide with real words); longer names tolerate the
/// one-or-two-letter mishearings Whisper typically produces.
fn fuzz_cap(n: usize) -> usize {
    match n {
        0..=4 => 0,
        5..=7 => 1,
        _ => 2,
    }
}

/// Rewrite fuzzy matches of the user's vocabulary terms to their exact
/// spelling. Matches windows of adjacent words (so "echo flow" -> "EchoFlow"),
/// compares joined lowercase forms, and splices the canonical term over the
/// original span. Case-only fixes count as matches. Same-word-count windows may
/// differ by the fuzz budget; merged/split windows must match exactly.
pub fn apply_custom_vocab(text: &str, vocab: &str) -> String {
    let mut terms = vocab_terms(vocab);
    if terms.is_empty() || text.trim().is_empty() {
        return text.to_string();
    }
    // Longer terms first so "EchoFlow Pro" wins over "EchoFlow" on overlap.
    terms.sort_by_key(|t| std::cmp::Reverse(t.chars().count()));

    let words = words_of(text);
    if words.is_empty() {
        return text.to_string();
    }

    // Collect non-overlapping replacements (byte spans), then splice right-to-left.
    let mut repl: Vec<(usize, usize, &str)> = Vec::new();
    let overlaps = |a: usize, b: usize, list: &[(usize, usize, &str)]| {
        list.iter().any(|&(s, e, _)| a < e && s < b)
    };

    for term in terms {
        let term_words: Vec<String> = term
            .split(|c: char| !c.is_alphanumeric())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_lowercase())
            .collect();
        if term_words.is_empty() {
            continue;
        }
        let joined_term = term_words.concat();
        let cap = fuzz_cap(joined_term.chars().count());
        let n = term_words.len();
        // The spoken form may merge or split words vs. the written term, so try
        // windows one word shorter/longer than the term itself.
        let sizes: Vec<usize> = [n.saturating_sub(1).max(1), n, n + 1]
            .iter()
            .copied()
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect();

        let mut i = 0;
        while i < words.len() {
            let mut matched = false;
            for &size in sizes.iter().rev() {
                if i + size > words.len() {
                    continue;
                }
                let start = words[i].start;
                let end = words[i + size - 1].end;
                if overlaps(start, end, &repl) {
                    continue;
                }
                let joined: String = words[i..i + size].iter().map(|w| w.lower.as_str()).collect();
                // Same-length windows may fuzz; merged/split windows must match
                // the joined form exactly (fuzz there causes false positives).
                let budget = if size == n { cap } else { 0 };
                if levenshtein(&joined, &joined_term, budget) <= budget {
                    if &text[start..end] != term {
                        repl.push((start, end, term));
                    }
                    i += size;
                    matched = true;
                    break;
                }
            }
            if !matched {
                i += 1;
            }
        }
    }

    if repl.is_empty() {
        return text.to_string();
    }
    repl.sort_by_key(|&(s, _, _)| std::cmp::Reverse(s));
    let mut out = text.to_string();
    for (start, end, term) in repl {
        out.replace_range(start..end, term);
    }
    out
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
    fn vocab_exact_case_fix() {
        // Case-only mismatch is corrected to the canonical spelling.
        assert_eq!(
            apply_custom_vocab("i opened echoflow today", "EchoFlow"),
            "i opened EchoFlow today"
        );
    }

    #[test]
    fn vocab_merges_split_words() {
        assert_eq!(
            apply_custom_vocab("launch echo flow now", "EchoFlow, Tauri"),
            "launch EchoFlow now"
        );
    }

    #[test]
    fn vocab_fuzzy_fixes_mishearing() {
        // One-letter mishearing on a 5+ char name.
        assert_eq!(apply_custom_vocab("ask mnoor about it", "Mnour"), "ask Mnour about it");
        // Two-letter slip on a long name.
        assert_eq!(
            apply_custom_vocab("the wisperr model", "Whisperr"),
            "the Whisperr model"
        );
    }

    #[test]
    fn vocab_leaves_common_words_alone() {
        // Short terms match exactly only; nearby words must never be hijacked.
        assert_eq!(apply_custom_vocab("a minor issue", "Mnour"), "a minor issue");
        assert_eq!(apply_custom_vocab("the cat sat", "Cats"), "the cat sat");
        // Already-correct text is untouched.
        assert_eq!(apply_custom_vocab("EchoFlow is here", "EchoFlow"), "EchoFlow is here");
    }

    #[test]
    fn vocab_arabic_terms() {
        // Arabic term with a dropped letter is restored (7 chars -> fuzz 1).
        assert_eq!(
            apply_custom_vocab("رحت على القاهره امس", "القاهرة"),
            "رحت على القاهرة امس"
        );
    }

    #[test]
    fn vocab_multiword_term_and_punctuation() {
        assert_eq!(
            apply_custom_vocab("meet visual studio code, please", "Visual Studio Code"),
            "meet Visual Studio Code, please"
        );
    }

    #[test]
    fn vocab_terms_split_on_all_separators() {
        assert_eq!(
            vocab_terms("a, b\nc; d، e؛ f"),
            vec!["a", "b", "c", "d", "e", "f"]
        );
        assert!(vocab_terms("  ,\n; ").is_empty());
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
