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
/// Phrases Whisper produces on near-silence (subtitle credits and video outros
/// from its training data; openai/whisper discussions 2608 and 1150).
const SILENCE_PHRASES: &[&str] = &[
    "thank you for watching",
    "thanks for watching",
    "please subscribe",
    "subtitles by the amara.org community",
    "ترجمة نانسي قنقر",
    "اشتركوا في القناة",
    "شكرا للمشاهدة",
    "شكراً للمشاهدة",
];

fn normalize_words(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric() || c.is_whitespace())
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// True when a decode is one of Whisper's silence hallucinations: a stock
/// phrase, or a leak of its own initial prompt (the dialect prime or the
/// vocabulary list). A single word never counts as a leak, so saying one
/// vocabulary term survives. Callers only ask for quiet clips, so a user who
/// really says one of these at normal volume keeps it.
pub fn is_silence_artifact(text: &str, initial_prompt: Option<&str>) -> bool {
    let t = normalize_words(text);
    if t.is_empty() {
        return false;
    }
    if SILENCE_PHRASES.iter().any(|p| normalize_words(p) == t) {
        return true;
    }
    match initial_prompt {
        Some(prompt) if t.split(' ').count() >= 2 => {
            format!(" {} ", normalize_words(prompt)).contains(&format!(" {t} "))
        }
        _ => false,
    }
}

#[cfg(test)]
mod silence_artifact_tests {
    use super::is_silence_artifact;

    #[test]
    fn stock_phrases_are_artifacts() {
        assert!(is_silence_artifact("Thank you for watching!", None));
        assert!(is_silence_artifact("thanks for watching.", None));
        assert!(is_silence_artifact("ترجمة نانسي قنقر", None));
        assert!(!is_silence_artifact("Thank you.", None), "a real short thanks stays");
    }

    #[test]
    fn a_leaked_prompt_is_an_artifact_but_one_term_is_not() {
        let prime = "محادثة يومية بالعربية العامية. شلونك؟ إزيك يا باشا؟ شو أخبارك؟";
        assert!(is_silence_artifact("شلونك؟ إزيك يا باشا؟", Some(prime)));
        let vocab = "EchoFlow, Folowise, printmanjo.";
        assert!(is_silence_artifact("EchoFlow, Folowise.", Some(vocab)));
        assert!(!is_silence_artifact("EchoFlow.", Some(vocab)));
        assert!(!is_silence_artifact("Send it to Folowise today.", Some(vocab)));
        assert!(!is_silence_artifact("", Some(vocab)));
    }
}

/// Share of letters in Arabic script, 0.0 when there are no letters.
pub fn arabic_share(text: &str) -> f32 {
    let (mut arabic, mut letters) = (0usize, 0usize);
    for c in text.chars().filter(|c| c.is_alphabetic()) {
        letters += 1;
        if matches!(c, '\u{0600}'..='\u{06FF}' | '\u{0750}'..='\u{077F}' | '\u{FB50}'..='\u{FDFF}' | '\u{FE70}'..='\u{FEFF}') {
            arabic += 1;
        }
    }
    if letters == 0 {
        0.0
    } else {
        arabic as f32 / letters as f32
    }
}

/// True when a text would read as the wrong language under an English target:
/// more than 15% Arabic letters and at least 12 of them, so one Arabic name in
/// an English sentence does not count.
pub fn is_arabic_text(text: &str) -> bool {
    let arabic = text
        .chars()
        .filter(|c| matches!(c, '\u{0600}'..='\u{06FF}' | '\u{0750}'..='\u{077F}' | '\u{FB50}'..='\u{FDFF}' | '\u{FE70}'..='\u{FEFF}'))
        .count();
    arabic >= 12 && arabic_share(text) > 0.15
}

#[cfg(test)]
mod arabic_text_tests {
    use super::is_arabic_text;

    #[test]
    fn one_arabic_name_in_english_is_still_english() {
        assert!(!is_arabic_text("Send the report to محمد today, please."));
        assert!(!is_arabic_text("Send the report today, please."));
        assert!(is_arabic_text("أريد أن أعدل الصفحة الرئيسية وأضيف زرا جديدا للتسجيل."));
        assert!(is_arabic_text("Can you check the report? بكرة الصبح إن شاء الله and send it."));
    }
}

/// True when a quiet decode is a leak of one prompt line (the mixed-language
/// prime): it sits inside that line and either uses both scripts or covers most
/// of it. Everyday runs the line happens to contain ("I think") are speech.
pub fn is_prompt_line_leak(text: &str, line: &str) -> bool {
    let t = normalize_words(text);
    if t.is_empty() {
        return false;
    }
    let l = normalize_words(line);
    if !format!(" {l} ").contains(&format!(" {t} ")) {
        return false;
    }
    let has_arabic = t.chars().any(|c| matches!(c, '\u{0600}'..='\u{06FF}'));
    let has_latin = t.chars().any(|c| c.is_ascii_alphabetic());
    let covered = t.split(' ').count() as f32 / l.split(' ').count().max(1) as f32;
    (has_arabic && has_latin) || covered >= 0.6
}

const TAIL_STOP_WORDS: &[&str] = &[
    "about", "actually", "also", "basically", "been", "could", "does", "done", "from",
    "going", "gonna", "have", "here", "into", "just", "know", "like", "make", "made", "mean",
    "more", "most", "much", "really", "should", "some", "something", "sort", "than", "that",
    "their", "them", "then", "there", "these", "they", "thing", "things", "think", "this",
    "those", "very", "want", "were", "what", "when", "which", "will", "with", "would", "yeah",
    "your",
];

fn tail_content_words(s: &str) -> Vec<String> {
    s.split(|c: char| !(c.is_ascii_alphanumeric() || c == '\''))
        .map(|w| w.trim_matches('\'').to_ascii_lowercase())
        .filter(|w| w.len() >= 4 && !TAIL_STOP_WORDS.contains(&w.as_str()))
        .map(|w| {
            for suffix in ["ing", "ed", "es", "s"] {
                if w.len() >= suffix.len() + 4 && w.ends_with(suffix) {
                    return w[..w.len() - suffix.len()].to_string();
                }
            }
            w
        })
        .collect()
}

/// Share of the content words in the last 30% of what was said (Latin script)
/// that appear in the AI reply. A low value means the reply dropped the end of a
/// long take. None when the take is short or not written in Latin script.
pub fn tail_coverage(said: &str, reply: &str) -> Option<f32> {
    let said_words = tail_content_words(said);
    if said_words.len() < 12 {
        return None;
    }
    let tail = &said_words[said_words.len() * 7 / 10..];
    let reply_words: std::collections::HashSet<String> = tail_content_words(reply).into_iter().collect();
    let hit = tail.iter().filter(|w| reply_words.contains(*w)).count();
    Some(hit as f32 / tail.len() as f32)
}

#[cfg(test)]
mod prompt_leak_and_coverage_tests {
    use super::{is_prompt_line_leak, tail_coverage};
    use crate::whisper::MIXED_PRIME;

    #[test]
    fn everyday_words_from_the_mixed_prime_are_speech() {
        assert!(!is_prompt_line_leak("I think.", MIXED_PRIME));
        assert!(!is_prompt_line_leak("Change the design.", MIXED_PRIME));
        assert!(!is_prompt_line_leak("Send it today.", MIXED_PRIME));
    }

    #[test]
    fn a_leaked_mixed_prime_is_caught() {
        assert!(is_prompt_line_leak(MIXED_PRIME, MIXED_PRIME));
        assert!(is_prompt_line_leak("So يعني I think", MIXED_PRIME));
    }

    #[test]
    fn tail_coverage_flags_a_dropped_ending() {
        let said = "the booking page lets people pick a past date so first fix that then check \
the payment reminder emails and the invoice totals because the numbers looked wrong on friday \
and finally clean up the admin table filters that keep resetting";
        let full = "The booking page lets people pick a past date, so fix that first. Then check the \
payment reminder emails and the invoice totals, because the numbers looked wrong on Friday. Finally, \
clean up the admin table filters that keep resetting.";
        let cut = "The booking page lets people pick a past date, so fix that first.";
        assert!(tail_coverage(said, full).unwrap() >= 0.8);
        assert!(tail_coverage(said, cut).unwrap() < 0.4);
    }

    #[test]
    fn tail_coverage_skips_short_and_arabic_takes() {
        assert_eq!(tail_coverage("please send the file", "Please send the file."), None);
        assert_eq!(tail_coverage("ابعتلي الملف بكرة الصبح لو سمحت ضروري جداً قبل الاجتماع", "Send me the file."), None);
    }
}

#[cfg(test)]
mod arabic_share_tests {
    #[test]
    fn measures_the_arabic_part() {
        assert_eq!(super::arabic_share("Hello there"), 0.0);
        assert_eq!(super::arabic_share("مرحبا"), 1.0);
        assert_eq!(super::arabic_share("..."), 0.0);
        let mixed = super::arabic_share("send the تقرير today");
        assert!(mixed > 0.2 && mixed < 0.5, "{mixed}");
    }
}

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

/// Everyday short words. A fuzzy match that has to merge one of these into a
/// window is an ordinary phrase ("follow is", "for print"), not a misheard term.
const FUNCTION_WORDS: &[&str] = &[
    "a", "an", "the", "is", "are", "was", "were", "be", "am", "to", "of", "for", "in", "on",
    "at", "by", "and", "or", "but", "if", "it", "its", "as", "so", "we", "i", "you", "he",
    "she", "they", "my", "your", "our", "this", "that", "with", "from", "not", "no", "do",
    "me", "us", "في", "من", "على", "عن", "مع", "الى", "إلى",
];

/// Extra checks on a fuzzy (not exact) match. A window merged from more words
/// than the term may not contain a function word, and a single word two edits
/// away must share the term's first three letters: "footprint" is not
/// "foloprint", "followprint" is.
fn fuzzy_match_allowed(
    size: usize,
    term_words: usize,
    has_function_word: bool,
    joined: &str,
    term: &str,
    distance: usize,
) -> bool {
    if distance == 0 {
        return true;
    }
    if size > term_words && has_function_word {
        return false;
    }
    if size == 1 && distance >= 2 {
        return joined.chars().take(3).eq(term.chars().take(3));
    }
    true
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
            // Evaluate every window size at this position and keep the CLOSEST
            // match (ties go to the smaller window). Trying sizes largest-first
            // with a fuzz budget would swallow an innocent neighbor word:
            // "EchoFlow is" (distance 2 to "echoflow") must lose to the exact
            // "EchoFlow" (distance 0) right under it.
            let mut best: Option<(usize, usize)> = None; // (distance, size)
            for &size in sizes.iter() {
                if i + size > words.len() {
                    continue;
                }
                let start = words[i].start;
                let end = words[i + size - 1].end;
                if overlaps(start, end, &repl) {
                    continue;
                }
                let joined: String = words[i..i + size].iter().map(|w| w.lower.as_str()).collect();
                // Same-length windows may fuzz. Merged/split windows (the term
                // heard as one-more/one-fewer words) get the budget only for
                // long terms (8+ chars): Whisper writing "Folowise" as
                // "follow wise" needs 2 edits AND a merge, and long rare names
                // are safe to fuzz — short ones must still match exactly or
                // common word pairs would get hijacked.
                let budget = if size == n || joined_term.chars().count() >= 8 {
                    cap
                } else {
                    0
                };
                let d = levenshtein(&joined, &joined_term, budget);
                let has_function_word = words[i..i + size]
                    .iter()
                    .any(|w| FUNCTION_WORDS.contains(&w.lower.as_str()));
                let allowed = fuzzy_match_allowed(size, n, has_function_word, &joined, &joined_term, d);
                if d <= budget && allowed && best.map_or(true, |(bd, _)| d < bd) {
                    best = Some((d, size));
                }
            }
            if let Some((_, size)) = best {
                let start = words[i].start;
                let end = words[i + size - 1].end;
                if &text[start..end] != term {
                    repl.push((start, end, term));
                }
                i += size;
            } else {
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
    fn vocab_fuzzy_merge_on_long_terms() {
        // Long term (8+ chars) misheard AND split: fuzz applies across the merge.
        assert_eq!(
            apply_custom_vocab("open follow wise now", "Folowise"),
            "open Folowise now"
        );
        // Short terms still require an exact joined match — no hijacking.
        assert_eq!(apply_custom_vocab("go for it", "Fort"), "go for it");
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
    fn vocab_leaves_real_words_near_a_term() {
        // Ordinary phrases two edits (plus a merge) away from his long terms.
        let vocab = "foloprint, folowise";
        for s in [
            "reduce the footprint of the app",
            "it is ready for print today",
            "the rule to follow is simple",
        ] {
            assert_eq!(apply_custom_vocab(s, vocab), s);
        }
    }

    #[test]
    fn vocab_still_fixes_his_real_mishearings() {
        let vocab = "foloprint, folowise";
        assert_eq!(apply_custom_vocab("for like full print", vocab), "for like foloprint");
        assert_eq!(apply_custom_vocab("fix the followprint issue", vocab), "fix the foloprint issue");
        assert_eq!(apply_custom_vocab("open follow wise now", vocab), "open folowise now");
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
