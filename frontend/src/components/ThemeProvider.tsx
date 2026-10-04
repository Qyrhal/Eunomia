"use client";

import { useEffect } from "react";
import { settings } from "@/lib/api";

export default function ThemeProvider({ children }: { children: React.ReactNode }) {
  useEffect(() => {
    settings
      .get()
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
