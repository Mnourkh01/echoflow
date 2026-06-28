//! Export a recording to .txt, .srt, or .docx.

use std::fs::File;
use std::io::Write;
use std::path::Path;

use anyhow::Result;
use docx_rs::{AlignmentType, Docx, Paragraph, Run};

use crate::models::RecordingResult;

const RTL_LANGS: [&str; 4] = ["ar", "he", "fa", "ur"];

fn is_rtl(lang: &str) -> bool {
    RTL_LANGS.contains(&lang)
}

pub fn export(rec: &RecordingResult, format: &str, path: &Path) -> Result<()> {
    match format {
        "txt" => export_txt(rec, path),
        "srt" => export_srt(rec, path),
        "docx" => export_docx(rec, path),
        other => Err(anyhow::anyhow!("unknown export format: {other}")),
    }
}

fn export_txt(rec: &RecordingResult, path: &Path) -> Result<()> {
    let mut f = File::create(path)?;
    f.write_all(rec.full_text.as_bytes())?;
    Ok(())
}

fn fmt_srt_time(ms: i64) -> String {
    let ms = ms.max(0);
    let h = ms / 3_600_000;
    let m = (ms % 3_600_000) / 60_000;
    let s = (ms % 60_000) / 1000;
    let milli = ms % 1000;
    format!("{h:02}:{m:02}:{s:02},{milli:03}")
}

fn export_srt(rec: &RecordingResult, path: &Path) -> Result<()> {
    let mut f = File::create(path)?;
    if rec.segments.is_empty() {
        // No timestamps: emit the whole transcript as a single cue.
        writeln!(
            f,
            "1\n{} --> {}\n{}\n",
            fmt_srt_time(0),
            fmt_srt_time(rec.duration_ms),
            rec.full_text
        )?;
        return Ok(());
    }
    for (i, seg) in rec.segments.iter().enumerate() {
        writeln!(
            f,
            "{}\n{} --> {}\n{}\n",
            i + 1,
            fmt_srt_time(seg.start_ms),
            fmt_srt_time(seg.end_ms),
            seg.text
        )?;
    }
    Ok(())
}

fn export_docx(rec: &RecordingResult, path: &Path) -> Result<()> {
    let rtl = is_rtl(&rec.language);
    let align = if rtl {
        AlignmentType::Right
    } else {
        AlignmentType::Left
    };

    let mut docx = Docx::new();

    let lines: Vec<&str> = if rec.segments.is_empty() {
        vec![rec.full_text.as_str()]
    } else {
        rec.segments.iter().map(|s| s.text.as_str()).collect()
    };

    for line in lines {
        if line.trim().is_empty() {
            continue;
        }
        // Right alignment gives Arabic the correct visual flow in Word.
        let para = Paragraph::new().add_run(Run::new().add_text(line)).align(align);
        docx = docx.add_paragraph(para);
    }

    let file = File::create(path)?;
    docx.build().pack(file)?;
    Ok(())
}
