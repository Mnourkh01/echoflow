import { createContext, useContext, type ReactNode } from "react";

export type Lang = "en" | "ar";

type Entry = { en: string; ar: string };

// One flat dictionary. Keep keys short and grouped by surface in comments.
const STRINGS = {
  // Header / app
  app_title: { en: "EchoFlow", ar: "EchoFlow" },
  settings: { en: "Settings", ar: "الإعدادات" },
  minimize_to_pill: { en: "Minimize to floating pill", ar: "تصغير إلى الشريط العائم" },
  model_missing: { en: "model missing", ar: "النموذج مفقود" },
  starting: { en: "starting", ar: "جارٍ البدء" },

  // Updates
  update_available: { en: "Update available", ar: "تحديث متاح" },
  update_notif_body: {
    en: "A new version is ready. Open EchoFlow to update.",
    ar: "إصدار جديد جاهز. افتح EchoFlow للتحديث.",
  },
  update_now: { en: "Update now", ar: "حدّث الآن" },
  updating: { en: "Updating", ar: "جارٍ التحديث" },
  up_to_date: { en: "You're on the latest version", ar: "أنت على أحدث إصدار" },
  check_updates: { en: "Check for updates", ar: "التحقق من التحديثات" },
  checking: { en: "Checking…", ar: "جارٍ التحقق…" },
  about: { en: "About", ar: "حول" },
  app_version: { en: "Version", ar: "الإصدار" },
  manual_dl: { en: "Manual download:", ar: "تنزيل يدوي:" },

  // First-run model download
  first_run_title: { en: "Download the speech model", ar: "تنزيل نموذج الكلام" },
  first_run_desc: {
    en: "EchoFlow needs a one-time ~465 MB model to transcribe. It's saved on this PC and kept across updates.",
    ar: "يحتاج EchoFlow إلى نموذج بحجم ~465 ميجابايت لمرة واحدة للتحويل. يُحفظ على هذا الجهاز ويبقى عبر التحديثات.",
  },
  first_run_button: { en: "Download (~465 MB)", ar: "تنزيل (~465 ميجابايت)" },

  // Output modes (shared by header switcher + settings)
  mode_raw: { en: "Raw text", ar: "نص كما هو" },
  mode_translate: { en: "Translate to English", ar: "ترجمة إلى الإنجليزية" },
  mode_polish: { en: "Clean writing", ar: "تحسين الصياغة" },
  mode_prompt: { en: "Prompt mode", ar: "وضع البرومبت" },
  mode_raw_desc: {
    en: "Write exactly what was said, in the spoken language.",
    ar: "يكتب ما قيل تماماً وباللغة المنطوقة.",
  },
  mode_translate_desc: {
    en: "Speak any language or Arabic dialect, get professional English. Uses the local CLI.",
    ar: "تحدث بأي لغة أو لهجة عربية واحصل على إنجليزية احترافية. يستخدم الـCLI المحلي.",
  },
  mode_polish_desc: {
    en: "Turn rough speech into a clean English paragraph. Uses the local CLI.",
    ar: "يحول الكلام العفوي إلى فقرة إنجليزية مرتبة. يستخدم الـCLI المحلي.",
  },
  mode_prompt_desc: {
    en: "Turn your idea into a senior, context-engineered prompt. Uses the local CLI.",
    ar: "يحول فكرتك إلى برومبت احترافي مهندَس السياق. يستخدم الـCLI المحلي.",
  },

  // Record control
  transcribing: { en: "Transcribing", ar: "جارٍ التحويل" },
  no_speech_notice: { en: "No speech detected, discarded", ar: "لم يُكتشف كلام، تم التجاهل" },
  translate_warning_notice: {
    en: "Still on Translate. You spoke the target language, so your words are kept as is.",
    ar: "ما زلت في وضع الترجمة. تحدثت باللغة الهدف، لذا عُرضت كلماتك كما هي.",
  },
  start_recording: { en: "Start recording", ar: "بدء التسجيل" },
  stop_recording: { en: "Stop recording", ar: "إيقاف التسجيل" },
  talk_hint_pre: { en: "Click or hold", ar: "انقر أو اضغط مطولاً" },
  talk_hint_post: { en: "to talk", ar: "للتحدث" },

  // History
  search_placeholder: { en: "Search transcripts", ar: "ابحث في النصوص" },
  nothing_saved: { en: "Nothing saved yet.", ar: "لا يوجد شيء محفوظ بعد." },
  no_results: { en: "No matches.", ar: "لا توجد نتائج." },
  no_speech_preview: { en: "(no speech)", ar: "(لا يوجد كلام)" },
  delete: { en: "Delete", ar: "حذف" },
  today: { en: "Today", ar: "اليوم" },
  yesterday: { en: "Yesterday", ar: "أمس" },

  // Prompts library
  tab_history: { en: "History", ar: "السجل" },
  tab_prompts: { en: "Prompts", ar: "البرومبتات" },
  no_prompts: { en: "No saved prompts yet.", ar: "لا توجد برومبتات محفوظة بعد." },
  save_as_prompt: { en: "Save as prompt", ar: "حفظ كبرومبت" },
  prompt_saved: { en: "Saved to prompts", ar: "حُفظ في البرومبتات" },
  no_prompt_selected: { en: "No prompt selected", ar: "لم يُختر برومبت" },
  no_prompt_hint: {
    en: "Pick a saved prompt to view and copy it. Save good ones from any result with 'Save as prompt'.",
    ar: "اختر برومبتاً محفوظاً لعرضه ونسخه. احفظ الجيد منها من أي نتيجة عبر 'حفظ كبرومبت'.",
  },
  copied: { en: "Copied", ar: "تم النسخ" },
  pin_keep: { en: "Pin (keep)", ar: "تثبيت (إبقاء)" },
  unpin: { en: "Unpin", ar: "إلغاء التثبيت" },
  pinned: { en: "Kept", ar: "محفوظ" },

  // Transcript view
  no_transcript: { en: "No transcript yet", ar: "لا يوجد نص بعد" },
  no_transcript_hint: {
    en: "Record something or pick an item from history. English and Arabic are detected automatically.",
    ar: "سجّل شيئاً أو اختر عنصراً من السجل. يتم التعرف على الإنجليزية والعربية تلقائياً.",
  },
  copy_text: { en: "Copy text", ar: "نسخ النص" },
  export_txt: { en: "Export .txt", ar: "تصدير .txt" },
  export_srt: { en: "Export .srt subtitles", ar: "تصدير ترجمات .srt" },
  export_docx: { en: "Export .docx", ar: "تصدير .docx" },
  no_speech_detected: { en: "(no speech detected)", ar: "(لم يُكتشف كلام)" },
  plain_text: { en: "Plain text", ar: "نص عادي" },
  timestamps: { en: "Timestamps", ar: "الطوابع الزمنية" },

  // Settings panel
  microphone: { en: "Microphone", ar: "الميكروفون" },
  system_default: { en: "System default", ar: "افتراضي النظام" },
  refresh: { en: "Refresh", ar: "تحديث" },
  test_mic: { en: "Test", ar: "اختبار" },
  stop_test: { en: "Stop", ar: "إيقاف" },
  mic_test_hint: {
    en: "Click Test and speak: the bar should move. Pick the mic or headset you want.",
    ar: "اضغط اختبار وتحدث: يجب أن يتحرك الشريط. اختر الميكروفون أو السماعة التي تريدها.",
  },
  translate_to: { en: "Translate into", ar: "الترجمة إلى" },
  model: { en: "Model", ar: "النموذج" },
  model_hint: {
    en: "Larger models are more accurate for Arabic but slower.",
    ar: "النماذج الأكبر أدق مع العربية لكنها أبطأ.",
  },
  not_downloaded: { en: "(not downloaded)", ar: "(غير مُنزَّل)" },
  download: { en: "Download", ar: "تنزيل" },
  downloading: { en: "Downloading", ar: "جارٍ التنزيل" },
  in_use: { en: "In use", ar: "قيد الاستخدام" },
  use_model: { en: "Use", ar: "استخدام" },
  download_failed: { en: "Download failed", ar: "فشل التنزيل" },
  bigger_better_arabic: {
    en: "Bigger models hear Arabic dialects far better, but are larger and slower.",
    ar: "النماذج الأكبر تسمع اللهجات العربية أفضل بكثير، لكنها أكبر حجماً وأبطأ.",
  },
  language: { en: "Language", ar: "اللغة" },
  auto_detect: { en: "Auto detect", ar: "كشف تلقائي" },
  english: { en: "English", ar: "الإنجليزية" },
  arabic: { en: "Arabic", ar: "العربية" },
  dialect: { en: "Arabic dialect", ar: "اللهجة العربية" },
  dialect_hint: {
    en: "Used whenever Arabic is detected or chosen. Picking your dialect transcribes spoken Arabic better than the generic prime.",
    ar: "يُستخدم متى ما اكتُشفت العربية أو اختيرت. اختيار لهجتك يحوّل الكلام العربي أفضل من التهيئة العامة.",
  },
  dialect_auto: { en: "Auto (mixed)", ar: "تلقائي (مختلط)" },
  dialect_egyptian: { en: "Egyptian", ar: "مصري" },
  dialect_levantine: { en: "Levantine", ar: "شامي" },
  dialect_gulf: { en: "Gulf", ar: "خليجي" },
  dialect_iraqi: { en: "Iraqi", ar: "عراقي" },
  dialect_maghrebi: { en: "Maghrebi", ar: "مغاربي" },
  output: { en: "Output", ar: "المخرجات" },
  cli_command: { en: "AI command", ar: "أمر الذكاء الاصطناعي" },
  cli_command_hint: {
    en: "The local CLI used for Translate, Clean writing and Prompt mode. Default: claude (your existing subscription, no API key).",
    ar: "الـCLI المحلي المستخدم للترجمة وتحسين الصياغة ووضع البرومبت. الافتراضي: claude (اشتراكك الحالي، بدون مفتاح API).",
  },
  ai_engine: { en: "AI engine", ar: "محرك الذكاء الاصطناعي" },
  engine_cli: { en: "Local CLI", ar: "CLI محلي" },
  engine_api: { en: "My API key", ar: "مفتاح API الخاص بي" },
  engine_hint: {
    en: "CLI uses the Claude command already on this PC (free). API uses your own key and is billed by the provider.",
    ar: "الـCLI يستخدم أمر claude الموجود على هذا الجهاز (مجاني). API يستخدم مفتاحك الخاص ويُحاسَب من المزوّد.",
  },
  api_provider: { en: "Provider", ar: "المزوّد" },
  api_key: { en: "API key", ar: "مفتاح API" },
  api_key_hint: {
    en: "Stored locally on this device only. Billed by your provider per use.",
    ar: "يُحفظ محلياً على هذا الجهاز فقط. يُحاسَب من مزوّدك لكل استخدام.",
  },
  api_model: { en: "Model", ar: "النموذج" },
  api_base_url: { en: "Base URL", ar: "عنوان URL الأساسي" },
  usage: { en: "Token usage", ar: "استهلاك التوكنز" },
  usage_line: { en: "in / out / calls", ar: "إدخال / إخراج / مكالمات" },
  reset: { en: "Reset", ar: "تصفير" },
  hotkey_mode: { en: "Hotkey mode", ar: "نمط الاختصار" },
  push_to_talk: { en: "Push to talk", ar: "اضغط للتحدث" },
  toggle: { en: "Toggle", ar: "تبديل" },
  hotkey_mode_hint: {
    en: "Push to talk: hold the hotkey while speaking. Toggle: press once to start, press again to stop.",
    ar: "اضغط للتحدث: استمر بالضغط أثناء الكلام. تبديل: اضغط مرة للبدء ومرة أخرى للإيقاف.",
  },
  global_hotkey: { en: "Global hotkey", ar: "اختصار عام" },
  hotkey_hint: {
    en: "Click, then press the keys you want. Works even when this window is not focused.",
    ar: "انقر ثم اضغط المفاتيح التي تريدها. يعمل حتى عندما لا تكون هذه النافذة نشطة.",
  },
  press_shortcut: { en: "Press your shortcut...", ar: "اضغط اختصارك..." },
  click_set_shortcut: { en: "Click to set a shortcut", ar: "انقر لتعيين اختصار" },
  clear: { en: "clear", ar: "مسح" },
  type_into_active: { en: "Type into the active app", ar: "الكتابة في التطبيق النشط" },
  type_into_active_desc: {
    en: "Insert the recognized text where your cursor is, in any app.",
    ar: "إدراج النص المُتعرَّف عليه عند مؤشرك في أي تطبيق.",
  },
  auto_copy: { en: "Copy result to clipboard", ar: "نسخ النتيجة إلى الحافظة" },
  auto_copy_desc: {
    en: "After each result, leave the text on your clipboard so you can paste it anywhere with Ctrl+V.",
    ar: "بعد كل نتيجة، يُبقي النص في الحافظة لتلصقه في أي مكان بـ Ctrl+V.",
  },
  keep_line_breaks: { en: "Keep line breaks", ar: "إبقاء فواصل الأسطر" },
  keep_line_breaks_desc: {
    en: "Off (default): typed as one line, never presses Enter, so terminals, chat boxes and search bars don't submit early. On: keeps newlines for editors and Word.",
    ar: "إيقاف (افتراضي): يُكتب في سطر واحد ولا يضغط Enter، فلا تُرسل الطرفية وصناديق الدردشة وأشرطة البحث مبكراً. تشغيل: يُبقي الأسطر للمحررات وWord.",
  },
  sound_cue: { en: "Sound cue", ar: "تنبيه صوتي" },
  sound_cue_desc: {
    en: "Soft tone when recording starts and stops.",
    ar: "نغمة خفيفة عند بدء التسجيل وإيقافه.",
  },
  noise_suppression: { en: "Noise suppression", ar: "كبح الضوضاء" },
  noise_suppression_desc: {
    en: "Clean background noise (fans, traffic, hum) from the mic before transcribing, so your voice comes through clearly. Recommended.",
    ar: "ينظّف ضوضاء الخلفية (مراوح، ضجيج، طنين) من الميكروفون قبل التحويل ليصل صوتك بوضوح. مُستحسن.",
  },
  app_language: { en: "App language", ar: "لغة التطبيق" },

  // Storage / retention
  storage: { en: "Storage", ar: "التخزين" },
  auto_delete: { en: "Auto-delete old recordings", ar: "حذف التسجيلات القديمة تلقائياً" },
  auto_delete_hint: {
    en: "Keeps storage small. Recordings older than this are removed permanently. This is normal for a dictation app, your text is meant to be used, not archived forever.",
    ar: "يبقي التخزين صغيراً. تُحذف التسجيلات الأقدم من هذه المدة نهائياً. هذا طبيعي لتطبيق إملاء، فنصوصك للاستخدام لا للأرشفة للأبد.",
  },
  retain_1w: { en: "1 week", ar: "أسبوع" },
  retain_2w: { en: "2 weeks", ar: "أسبوعان" },
  retain_1m: { en: "1 month", ar: "شهر" },
  retain_forever: { en: "Keep all", ar: "الاحتفاظ بالكل" },
  clear_history: { en: "Clear history", ar: "مسح السجل" },
  clear_history_desc: {
    en: "Delete every saved recording and its audio. This cannot be undone.",
    ar: "حذف كل تسجيل محفوظ وملفه الصوتي. لا يمكن التراجع عن هذا.",
  },
  clear_confirm_q: { en: "Delete everything permanently?", ar: "حذف كل شيء نهائياً؟" },
  clear_confirm_yes: { en: "Yes, delete all", ar: "نعم، احذف الكل" },
  history_cleared: { en: "History cleared", ar: "تم مسح السجل" },
  pinned_safe_hint: {
    en: "Kept (pinned) recordings are never auto-deleted.",
    ar: "التسجيلات المحفوظة (المثبّتة) لا تُحذف تلقائياً أبداً.",
  },
  free_memory: { en: "Free memory when idle", ar: "تحرير الذاكرة عند الخمول" },
  free_memory_hint: {
    en: "Unloads the speech model from RAM after this idle time, then reloads it on your next dictation. Keeps the app light.",
    ar: "يُفرّغ نموذج الكلام من الذاكرة بعد مدة الخمول هذه، ثم يعيد تحميله عند الإملاء التالي. يبقي التطبيق خفيفاً.",
  },
  idle_5: { en: "5 min", ar: "5 دقائق" },
  idle_15: { en: "15 min", ar: "15 دقيقة" },
  idle_never: { en: "Never", ar: "أبداً" },

  // Recognition language + diacritics
  language_hint: {
    en: "Force a language, or Auto to detect it. European languages keep their accents.",
    ar: "افرض لغة، أو اختر تلقائي لكشفها. اللغات الأوروبية تحتفظ بعلاماتها.",
  },
  restore_diacritics: { en: "Restore accents (café, résumé)", ar: "استعادة العلامات (café، résumé)" },
  restore_diacritics_desc: {
    en: "Put diacritics back on common European loanwords in English output.",
    ar: "يعيد العلامات التشكيلية على الكلمات الأوروبية الدخيلة في المخرجات الإنجليزية.",
  },

  // Onboarding walkthrough
  ob_skip: { en: "Skip", ar: "تخطّي" },
  ob_next: { en: "Next", ar: "التالي" },
  ob_back: { en: "Back", ar: "السابق" },
  ob_done: { en: "Get started", ar: "ابدأ الآن" },
  ob_welcome_t: { en: "Welcome to EchoFlow", ar: "مرحباً بك في EchoFlow" },
  ob_welcome_b: {
    en: "Offline voice to text for Windows. Your speech is transcribed locally with Whisper, no cloud, no account, nothing leaves your PC.",
    ar: "تحويل الصوت إلى نص دون اتصال على ويندوز. يُحوَّل كلامك محلياً عبر Whisper، بلا سحابة ولا حساب، ولا شيء يغادر جهازك.",
  },
  ob_ptt_t: { en: "Talk with one key", ar: "تحدّث بضغطة واحدة" },
  ob_ptt_b: {
    en: "Hold your global hotkey (default Ctrl+Shift+Space) anywhere and speak, or press once in Toggle mode. The text types straight into whatever app you're in.",
    ar: "اضغط مطوّلاً اختصارك العام (افتراضياً Ctrl+Shift+Space) في أي مكان وتحدث، أو اضغطه مرة في وضع التبديل. يُكتب النص مباشرة في التطبيق الذي تستخدمه.",
  },
  ob_modes_t: { en: "Four ways to output", ar: "أربع طرق للمخرجات" },
  ob_modes_b: {
    en: "Raw text, Clean writing, Prompt mode, or Translate to English. Switch from the header or by right-clicking the tray icon.",
    ar: "نص كما هو، أو تحسين الصياغة، أو وضع البرومبت، أو الترجمة إلى الإنجليزية. بدّل بينها من الأعلى أو بالنقر بالزر الأيمن على أيقونة الشريط.",
  },
  ob_lang_t: { en: "Languages & accents", ar: "اللغات واللهجات" },
  ob_lang_b: {
    en: "English, Arabic with dialects, and European languages with full diacritics. Auto-detect handles mixed speech.",
    ar: "الإنجليزية، والعربية بلهجاتها، ولغات أوروبية بعلاماتها التشكيلية الكاملة. الكشف التلقائي يتعامل مع الكلام المختلط.",
  },
  ob_hist_t: { en: "History & prompts", ar: "السجل والبرومبتات" },
  ob_hist_b: {
    en: "Every transcript is saved locally and searchable. Pin the ones you want to keep and save good prompts to reuse.",
    ar: "يُحفظ كل نص محلياً وقابل للبحث. ثبّت ما تريد الاحتفاظ به واحفظ البرومبتات الجيدة لإعادة استخدامها.",
  },
  ob_setup_t: { en: "Make it yours", ar: "خصّصه لك" },
  ob_setup_b: {
    en: "Pick a bigger model for higher accuracy, choose your mic and hotkey, and set how long history is kept, all in Settings.",
    ar: "اختر نموذجاً أكبر لدقة أعلى، واختر ميكروفونك واختصارك، وحدّد مدة حفظ السجل، كلها في الإعدادات.",
  },

  cancel: { en: "Cancel", ar: "إلغاء" },
  save: { en: "Save", ar: "حفظ" },
  hotkey_warn: {
    en: "That shortcut could not be registered (another app may already use it). Pick a different one.",
    ar: "تعذّر تسجيل هذا الاختصار (قد يستخدمه تطبيق آخر). اختر اختصاراً مختلفاً.",
  },
} satisfies Record<string, Entry>;

export type StringKey = keyof typeof STRINGS;

export function translate(lang: Lang, key: StringKey): string {
  return STRINGS[key][lang];
}

const I18nContext = createContext<Lang>("en");

export function I18nProvider({ lang, children }: { lang: Lang; children: ReactNode }) {
  return <I18nContext.Provider value={lang}>{children}</I18nContext.Provider>;
}

/** Returns a bound translator for the current app language. */
export function useT() {
  const lang = useContext(I18nContext);
  const t = (key: StringKey) => translate(lang, key);
  return { t, lang };
}
