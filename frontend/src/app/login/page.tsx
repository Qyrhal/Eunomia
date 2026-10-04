"use client";

import { useState } from "react";
import Link from "next/link";
import { useRouter } from "next/navigation";
import { auth } from "@/lib/api";

export default function LoginPage() {
  const router = useRouter();
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  async function submit(e: React.FormEvent) {
    e.preventDefault();
    setBusy(true);
    setError(null);
    try {
      await auth.login(email, password);
      router.replace("/");
    } catch (err) {
      setError(err instanceof Error ? err.message : "Could not log in.");
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="min-h-screen w-full flex items-center justify-center">
      <form onSubmit={submit} className="surface w-full max-w-sm p-8 flex flex-col gap-5">
        <div>
          <div className="eyebrow mb-2">Eunomia</div>
          <h1 className="font-display text-2xl">Sign in</h1>
        </div>

        <label className="text-[13px] flex flex-col gap-1.5" style={{ color: "var(--ink-dim)" }}>
          Email
          <input
            type="email"
            required
            autoFocus
            className="field px-3 py-2.5 text-[13.5px]"
            value={email}
            onChange={(e) => setEmail(e.target.value)}
          />
        </label>

        <label className="text-[13px] flex flex-col gap-1.5" style={{ color: "var(--ink-dim)" }}>
          Password
          <input
            type="password"
            required
            className="field px-3 py-2.5 text-[13.5px]"
            value={password}
            onChange={(e) => setPassword(e.target.value)}
          />
        </label>

        {error && (
          <p className="text-[12.5px]" style={{ color: "var(--critical)" }}>
            {error}
          </p>
        )}

        <button
          type="submit"
          disabled={busy}
          className="px-4 py-2.5 text-[13px] font-medium rounded-xl disabled:opacity-50"
          style={{ background: "var(--felt)", color: "var(--canvas)" }}
        >
          {busy ? "Signing in…" : "Sign in"}
        </button>

        <p className="text-[12.5px]" style={{ color: "var(--ink-faint)" }}>
          No account yet?{" "}
          <Link href="/register" className="underline" style={{ color: "var(--ink)" }}>
            Register
          </Link>
        </p>
      </form>
    </div>
  );
}
