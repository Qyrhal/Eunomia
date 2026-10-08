"use client";

import { QueryClientProvider, type QueryClient } from "@tanstack/react-query";
import { makeQueryClient } from "@/lib/queries/client";

let browserClient: QueryClient | undefined;

// A fresh client per server render, one shared client in the browser.
const getQueryClient = () => (typeof window === "undefined" ? makeQueryClient() : (browserClient ??= makeQueryClient()));

export default function Providers({ children }: { children: React.ReactNode }) {
  return <QueryClientProvider client={getQueryClient()}>{children}</QueryClientProvider>;
}
