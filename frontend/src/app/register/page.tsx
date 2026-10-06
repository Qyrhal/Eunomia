"use client";

import { useState } from "react";
import Link from "next/link";
import { useRouter } from "next/navigation";
import { auth } from "@/lib/api";

export default function RegisterPage() {
  const router = useRouter();
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [confirmPassword, setConfirmPassword] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  async function submit(e: React.FormEvent) {
    e.preventDefault();
    if (password !== confirmPassword) {
      setError("Passwords don't match.");
      return;
    }
    setBusy(true);
    setError(null);
    try {
      await auth.register(email, password);
      router.replace("/onboarding");
    } catch (err) {
      setError(err instanceof Error ? err.message : "Could not register.");
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="min-h-screen w-full flex items-center justify-center">
      <form onSubmit={submit} className="surface w-full max-w-sm p-8 flex flex-col gap-5">
        <div>
          <div className="eyebrow mb-2">Eunomia</div>
          <h1 className="font-display text-2xl">Create your account</h1>
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
            minLength={8}
            maxLength={72}
            className="field px-3 py-2.5 text-[13.5px]"
            value={password}
            onChange={(e) => setPassword(e.target.value)}
          />
        </label>

        <label className="text-[13px] flex flex-col gap-1.5" style={{ color: "var(--ink-dim)" }}>
          Confirm password
          <input
            type="password"
            required
            minLength={8}
            maxLength={72}
            className="field px-3 py-2.5 text-[13.5px]"
            value={confirmPassword}
            onChange={(e) => setConfirmPassword(e.target.value)}
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
          {busy ? "Creating account…" : "Create account"}
        </button>

        <p className="text-[12.5px]" style={{ color: "var(--ink-faint)" }}>
          Already have an account?{" "}
          <Link href="/login" className="underline" style={{ color: "var(--ink)" }}>
            Sign in
          </Link>
        </p>
      </form>
    </div>
  );
}
