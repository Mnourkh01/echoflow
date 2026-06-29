import { useState } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import { save } from "@tauri-apps/plugin-dialog";
import { Copy, Check, FileText, FileType, Captions, Languages, Clock, Pin, BookmarkPlus } from "lucide-react";
import type { RecordingResult } from "../lib/api";
import { api } from "../lib/api";
import { dirFor, fontFor, langName, fmtDuration } from "../lib/format";
import { useT } from "../lib/i18n";

interface Props {
  rec: RecordingResult | null;
  onTogglePin?: (id: number, pinned: boolean) => void;
  onSavePrompt?: (text: string) => void;
}

export default function TranscriptView({ rec, onTogglePin, onSavePrompt }: Props) {
  const { t } = useT();
  const [copied, setCopied] = useState(false);
  const [showSegments, setShowSegments] = useState(false);

  if (!rec) {
    return (
      <div className="flex flex-1 flex-col items-center justify-center text-center text-ink-500">
        <p className="text-lg text-ink-400">{t("no_transcript")}</p>
        <p className="mt-1 max-w-xs text-sm">{t("no_transcript_hint")}</p>
      </div>
    );
  }

  const dir = dirFor(rec.language);
  const font = fontFor(rec.language);

  async function copy() {
    if (!rec) return;
    await navigator.clipboard.writeText(rec.full_text);
    setCopied(true);
    setTimeout(() => setCopied(false), 1200);
  }

  async function exportAs(format: "txt" | "srt" | "docx") {
    if (!rec) return;
    const ext = format;
    const path = await save({
      defaultPath: `transcript-${rec.id}.${ext}`,
      filters: [{ name: format.toUpperCase(), extensions: [ext] }],
    });
    if (path) await api.exportRecording(rec.id, format, path);
  }

  return (
    <div className="flex flex-1 flex-col overflow-hidden">
      <div className="flex flex-wrap items-center gap-2 border-b border-white/[0.06] px-5 py-3">
        <span className="inline-flex items-center gap-1.5 rounded-full border border-white/[0.06] bg-white/[0.04] px-2.5 py-1 text-xs text-ink-300">
          <Languages className="h-3.5 w-3.5" />
          {langName(rec.language)}
          {rec.language_confidence > 0 && (
            <span className="text-ink-500">
              {Math.round(rec.language_confidence * 100)}%
            </span>
          )}
        </span>
        <span className="inline-flex items-center gap-1.5 rounded-full border border-white/[0.06] bg-white/[0.04] px-2.5 py-1 text-xs text-ink-300">
          <Clock className="h-3.5 w-3.5" />
          {fmtDuration(rec.duration_ms)}
        </span>
        <span className="rounded-full border border-white/[0.06] bg-white/[0.04] px-2.5 py-1 text-xs text-ink-500">
          {rec.model}
        </span>

        <div className="ms-auto flex items-center gap-1.5">
          {onTogglePin && (
            <button
              onClick={() => onTogglePin(rec.id, !rec.pinned)}
              className={`tool-btn ${rec.pinned ? "text-accent" : ""}`}
              title={rec.pinned ? t("unpin") : t("pin_keep")}
            >
              <Pin className="h-4 w-4" fill={rec.pinned ? "currentColor" : "none"} />
            </button>
          )}
          <button onClick={copy} className="tool-btn" title={t("copy_text")}>
            {copied ? <Check className="h-4 w-4 text-emerald-400" /> : <Copy className="h-4 w-4" />}
          </button>
          {onSavePrompt && rec.full_text.trim() && (
            <button
              onClick={() => onSavePrompt(rec.full_text)}
              className="tool-btn"
              title={t("save_as_prompt")}
            >
              <BookmarkPlus className="h-4 w-4" />
            </button>
          )}
          <button onClick={() => exportAs("txt")} className="tool-btn" title={t("export_txt")}>
            <FileText className="h-4 w-4" />
          </button>
          <button onClick={() => exportAs("srt")} className="tool-btn" title={t("export_srt")}>
            <Captions className="h-4 w-4" />
          </button>
          <button onClick={() => exportAs("docx")} className="tool-btn" title={t("export_docx")}>
            <FileType className="h-4 w-4" />
          </button>
        </div>
      </div>

      <div className="flex-1 overflow-y-auto overscroll-contain px-6 py-5">
        {showSegments ? (
          <div className="space-y-3">
            {rec.segments.map((s, i) => (
              <div key={i} className="flex gap-3" dir={dir}>
                <span className="shrink-0 pt-1 font-mono text-xs text-ink-500" dir="ltr">
                  {fmtDuration(s.start_ms)}
                </span>
                <p className={`selectable leading-relaxed ${font}`}>{s.text}</p>
              </div>
            ))}
          </div>
        ) : (
          <p
            dir={dir}
            className={`selectable whitespace-pre-wrap text-lg leading-relaxed ${font}`}
          >
            {rec.full_text || t("no_speech_detected")}
          </p>
        )}
      </div>

      <div className="flex items-center gap-3 border-t border-white/[0.06] px-5 py-3">
        <audio
          controls
          src={convertFileSrc(rec.audio_path)}
          className="h-9 flex-1"
        />
        <button
          onClick={() => setShowSegments((v) => !v)}
          className="rounded-lg px-3 py-1.5 text-xs text-ink-400 hover:bg-white/[0.06] hover:text-white"
        >
          {showSegments ? t("plain_text") : t("timestamps")}
        </button>
      </div>
    </div>
  );
}
