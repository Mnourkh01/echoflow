import { useState } from "react";
import { mockIPC } from "@tauri-apps/api/mocks";
import FirstRunModel from "../components/FirstRunModel";
import { api } from "../lib/api";
import { I18nProvider, type Lang } from "../lib/i18n";
import { fakeDownload, mockModels } from "./mockModels";

const params = new URLSearchParams(window.location.search);
const lang: Lang = params.get("lang") === "ar" ? "ar" : "en";
const fail = params.get("fail");
const models = mockModels((params.get("present") ?? "").split(","));
document.documentElement.lang = lang;
document.documentElement.dir = lang === "ar" ? "rtl" : "ltr";

mockIPC(
  (cmd, payload) => {
    const name = (payload as { name?: string } | undefined)?.name ?? "";
    switch (cmd) {
      case "list_models":
        return models.map((m) => ({ ...m }));
      case "download_model":
        fakeDownload(name, models, fail === "download");
        return null;
      case "set_model":
        if (fail === "set_model") throw "could not save the model choice";
        return null;
      default:
        return null;
    }
  },
  { shouldMockEvents: true },
);

/**
 * Dev-only harness (`npm run dev` + `/?preview=first-run`): the first-run model
 * picker with the backend mocked. Add `&lang=ar`, `&present=small`,
 * `&fail=download` or `&fail=set_model`. main.tsx gates it behind import.meta.env.DEV.
 */
export default function FirstRunPreview() {
  const [ready, setReady] = useState<string | null>(null);
  return (
    <I18nProvider lang={lang}>
      {ready ? (
        <p className="m-6 text-sm text-ink-200">Ready: {ready}</p>
      ) : (
        <FirstRunModel
          onReady={async (m) => {
            await api.setModel(m);
            setReady(m);
          }}
        />
      )}
    </I18nProvider>
  );
}
