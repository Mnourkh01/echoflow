import { useState } from "react";
import { Copy, Check, Trash2 } from "lucide-react";
import type { Prompt } from "../lib/api";
import { useT } from "../lib/i18n";

interface Props {
  prompt: Prompt | null;
  onDelete: (id: number) => void;
}

/** Main-pane view for a saved prompt: read it, copy it to reuse, or delete it. */
export default function PromptView({ prompt, onDelete }: Props) {
  const { t } = useT();
  const [copied, setCopied] = useState(false);

  if (!prompt) {
    return (
      <div className="flex flex-1 flex-col items-center justify-center text-center text-ink-500">
        <p className="text-lg text-ink-400">{t("no_prompt_selected")}</p>
        <p className="mt-1 max-w-xs text-sm">{t("no_prompt_hint")}</p>
      </div>
    );
  }

  async function copy() {
    if (!prompt) return;
    await navigator.clipboard.writeText(prompt.text);
    setCopied(true);
    setTimeout(() => setCopied(false), 1200);
  }

  return (
    <div className="flex flex-1 flex-col overflow-hidden">
      <div className="flex items-center gap-2 border-b border-ink-800 px-5 py-3">
        <h2 className="flex-1 truncate text-sm font-medium text-ink-200" dir="auto">
          {prompt.title}
        </h2>
        <button onClick={copy} className="tool-btn" title={t("copy_text")}>
          {copied ? <Check className="h-4 w-4 text-emerald-400" /> : <Copy className="h-4 w-4" />}
        </button>
        <button onClick={() => onDelete(prompt.id)} className="tool-btn" title={t("delete")}>
          <Trash2 className="h-4 w-4" />
        </button>
      </div>
      <div className="flex-1 overflow-y-auto px-6 py-5">
        <p dir="auto" className="selectable whitespace-pre-wrap text-base leading-relaxed text-ink-100">
          {prompt.text}
        </p>
      </div>
    </div>
  );
}
