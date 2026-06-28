//! Post-process a transcript through a local CLI (default: Claude Code) using
//! the user's own subscription. No API key, no extra billing.

use std::io::Write;
use std::process::{Command, Stdio};

use anyhow::{anyhow, Result};
use serde_json::{json, Value};

/// Professional translation into `target` (any language / any Arabic dialect in).
/// Built per-call so the user can pick the target language.
pub fn translate_system(target: &str) -> String {
    let target = target.trim();
    let target = if target.is_empty() { "English" } else { target };
    format!(
        "You are a professional translator. The user gives you transcribed speech \
that may be in any language. If it is Arabic, it may be Modern Standard Arabic or \
ANY spoken dialect (Egyptian, Levantine, Gulf/Khaleeji, Iraqi, Maghrebi, Sudanese, \
Yemeni, etc.), with colloquial words, slang, and code-switching. Understand the \
intended meaning from context and translate it into clear, natural, professional \
{target}. Convey the full meaning and tone faithfully; smooth out speech \
disfluencies and repetition, but do not add, omit, or invent information, and do \
not summarize. If a word is ambiguous, choose the most likely meaning. Output ONLY \
the {target} translation, with no preamble, notes, transliteration, quotes, or markdown."
    )
}

/// Turn rough, spoken text into a clean English paragraph.
pub const POLISH_SYSTEM: &str = "\
You are an expert writing assistant. The user gives you rough spoken text, \
possibly in Arabic or broken English. Rewrite it as clear, natural, \
well-structured English prose that faithfully conveys their meaning. Fix \
grammar, remove filler and repetition, keep their intent and tone, and do not \
add information they did not say. Output ONLY the rewritten English text, with \
no preamble, explanation, quotes, or markdown.";

/// Lightly clean a rough spoken idea into a clear prompt that stays close to
/// the user's own words. Improve, don't rewrite from scratch.
pub const PROMPT_SYSTEM: &str = "\
The user dictates a rough idea (English or Arabic) of what they want to ask an \
AI. Rewrite it as a clear prompt in English that STAYS CLOSE to their own words \
and intent. Only fix grammar, wording, and clarity, and add a little context if \
it makes the request easier to understand. Keep it short and natural, the same \
length and scope they gave. Do NOT invent requirements, do NOT add a role, \
constraints, output-format sections, or any scaffolding they did not ask for, \
and do NOT answer the request. Output ONLY the cleaned-up prompt, with no \
preamble, commentary, quotes, or markdown.";

/// Result of an API call: the text plus token usage for the cost meter.
pub struct ApiResult {
    pub text: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
}

/// Call a hosted LLM with the user's own API key (Anthropic, OpenAI, or an
/// OpenAI-compatible custom endpoint). Uses curl + serde_json so we add no heavy
/// async HTTP dependency. The API key is never logged.
pub fn run_api(
    provider: &str,
    base_url: &str,
    api_key: &str,
    model: &str,
    system: &str,
    user: &str,
) -> Result<ApiResult> {
    if api_key.trim().is_empty() {
        return Err(anyhow!("API key is not set"));
    }
    if model.trim().is_empty() {
        return Err(anyhow!("API model is not set"));
    }

    let (url, headers, body) = if provider == "anthropic" {
        let body = json!({
            "model": model,
            "max_tokens": 2048,
            "system": system,
            "messages": [{ "role": "user", "content": user }],
        });
        let headers = vec![
            format!("x-api-key: {api_key}"),
            "anthropic-version: 2023-06-01".to_string(),
            "content-type: application/json".to_string(),
        ];
        (
            "https://api.anthropic.com/v1/messages".to_string(),
            headers,
            body,
        )
    } else {
        // openai or custom OpenAI-compatible endpoint
        let base = if provider == "custom" && !base_url.trim().is_empty() {
            base_url.trim().trim_end_matches('/').to_string()
        } else {
            "https://api.openai.com/v1".to_string()
        };
        let body = json!({
            "model": model,
            "messages": [
                { "role": "system", "content": system },
                { "role": "user", "content": user },
            ],
        });
        let headers = vec![
            format!("Authorization: Bearer {api_key}"),
            "content-type: application/json".to_string(),
        ];
        (format!("{base}/chat/completions"), headers, body)
    };

    // Pass the URL, headers (which carry the API key) and body to curl via a
    // config file read with `-K`, NOT as argv. Command-line arguments are
    // visible in the OS process table to any local process, so an `-H
    // "Authorization: Bearer <key>"` arg would leak the key while curl runs.
    // The config file lives in the user's temp dir, has a unique name, and is
    // deleted immediately after. The body file (the dictated text) is unique too.
    let uniq = format!(
        "{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    );
    let body_path = std::env::temp_dir().join(format!("echoflow-req-{uniq}.json"));
    let cfg_path = std::env::temp_dir().join(format!("echoflow-cfg-{uniq}.txt"));
    std::fs::write(&body_path, serde_json::to_vec(&body)?)?;

    // Unquoted curl-config values are taken literally to end of line, so Windows
    // backslash paths and header colons need no escaping.
    let mut cfg = String::new();
    cfg.push_str(&format!("url = {url}\n"));
    cfg.push_str("request = POST\n");
    for h in &headers {
        cfg.push_str(&format!("header = {h}\n"));
    }
    cfg.push_str("max-time = 120\n");
    cfg.push_str("silent\n");
    cfg.push_str(&format!("data-binary = @{}\n", body_path.display()));
    if let Err(e) = std::fs::write(&cfg_path, &cfg) {
        let _ = std::fs::remove_file(&body_path);
        return Err(anyhow!("could not write request config: {e}"));
    }

    let out = Command::new("curl")
        .arg("-K")
        .arg(&cfg_path)
        .output()
        .map_err(|e| anyhow!("could not start curl: {e}"));
    // Remove both temp files (key-bearing config + request body) right away.
    let _ = std::fs::remove_file(&cfg_path);
    let _ = std::fs::remove_file(&body_path);
    let out = out?;
    if !out.status.success() {
        return Err(anyhow!(
            "request failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }

    let v: Value =
        serde_json::from_slice(&out.stdout).map_err(|e| anyhow!("invalid API response: {e}"))?;
    if let Some(err) = v.get("error") {
        let msg = err
            .get("message")
            .and_then(|m| m.as_str())
            .unwrap_or("unknown API error");
        return Err(anyhow!("{msg}"));
    }

    let (text, input_tokens, output_tokens) = if provider == "anthropic" {
        let text = v["content"]
            .as_array()
            .and_then(|a| a.iter().find_map(|p| p.get("text").and_then(|t| t.as_str())))
            .unwrap_or("")
            .trim()
            .to_string();
        let u = &v["usage"];
        (
            text,
            u.get("input_tokens").and_then(Value::as_u64).unwrap_or(0),
            u.get("output_tokens").and_then(Value::as_u64).unwrap_or(0),
        )
    } else {
        let text = v["choices"][0]["message"]["content"]
            .as_str()
            .unwrap_or("")
            .trim()
            .to_string();
        let u = &v["usage"];
        (
            text,
            u.get("prompt_tokens").and_then(Value::as_u64).unwrap_or(0),
            u.get("completion_tokens").and_then(Value::as_u64).unwrap_or(0),
        )
    };

    if text.is_empty() {
        return Err(anyhow!("API returned no text"));
    }
    Ok(ApiResult {
        text,
        input_tokens,
        output_tokens,
    })
}

/// Run `<command> -p --output-format text --append-system-prompt <system>` with
/// `user_text` piped on stdin. Returns the CLI's stdout. Errors propagate so the
/// caller can fall back to the raw transcript.
pub fn run_cli(command: &str, system_prompt: &str, user_text: &str) -> Result<String> {
    let command = command.trim();
    let exe = if command.is_empty() { "claude" } else { command };

    let mut child = Command::new(exe)
        .arg("-p")
        .arg("--output-format")
        .arg("text")
        .arg("--append-system-prompt")
        .arg(system_prompt)
        .current_dir(std::env::temp_dir()) // neutral cwd: don't pick up a project CLAUDE.md
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| anyhow!("could not start '{exe}': {e}"))?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(user_text.as_bytes())?;
    }

    let output = child.wait_with_output()?;
    if !output.status.success() {
        return Err(anyhow!(
            "{exe} failed ({}): {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }

    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if text.is_empty() {
        return Err(anyhow!("{exe} returned no output"));
    }
    Ok(text)
}
