"use client";

import { useEffect } from "react";
import { api, AppSettings } from "@/lib/api";

export default function ThemeProvider({ children }: { children: React.ReactNode }) {
  useEffect(() => {
    api
      .get<AppSettings>("/api/settings")
      .then((s) => {
        const root = document.documentElement;
        if (s.theme.mode === "light" || s.theme.mode === "dark") {
          root.dataset.theme = s.theme.mode;
        } else {
          delete root.dataset.theme;
        }
        if (s.theme.accent) {
          root.style.setProperty("--accent", s.theme.accent);
          root.style.setProperty("--series-1", s.theme.accent);
        }
      })
      .catch(() => {});
  }, []);

  return <>{children}</>;
}
