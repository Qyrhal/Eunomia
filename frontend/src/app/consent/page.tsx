"use client";

import ErrorLine, { failure, type Failure } from "@/components/ErrorLine";
import { useEffect, useState } from "react";
import { Loader2, ShieldAlert } from "lucide-react";
import { oauth, type ConsentInfo } from "@/lib/api";
import { getMe } from "@/lib/gen";
import { call } from "@/lib/queries/client";
import EunomiaMark from "@/components/EunomiaMark";
import ThemeToggle from "@/components/ThemeToggle";
import { InputModeTracker } from "@/components/bits/motion";

/** The OAuth consent screen. /oauth/authorize validates the request and sends the browser here
 * with the same query string; Allow or Deny posts the decision and follows the redirect the
 * server returns (the client's callback with a one-time code, or the denial). */
export default function ConsentPage() {
  const [info, setInfo] = useState<ConsentInfo | null>(null);
  const [error, setError] = useState<Failure | null>(null);
  const [busy, setBusy] = useState<"allow" | "deny" | null>(null);
  const [logoFailed, setLogoFailed] = useState(false);

  useEffect(() => {
    const query = window.location.search.slice(1);
    oauth
      .consent(query)
      .then(setInfo)
      .catch(async (e) => {
        // No session: sign in first, then come straight back here.
        const signedIn = await call(getMe()).then(() => true, () => false);
        if (!signedIn) {
          const back = encodeURIComponent(`/consent?${query}`);
          window.location.replace(`/login?next=${back}`);
          return;
        }
        setError(failure(e, "This connection request is not valid. Start it again from the app."));
      });
  }, []);

  async function decide(approve: boolean) {
    setBusy(approve ? "allow" : "deny");
    setError(null);
    try {
      const params = Object.fromEntries(new URLSearchParams(window.location.search));
      const { redirect_to } = await oauth.decide(params, approve);
      window.location.assign(redirect_to);
    } catch (e) {
      setError(failure(e, "Could not complete the request. Start it again from the app."));
      setBusy(null);
    }
  }

  return (
    <div className="canvas-grid min-h-screen w-full flex flex-col px-6 py-6 sm:px-10">
      <InputModeTracker />
      <header className="flex items-center justify-between">
        <span className="inline-flex items-center gap-2 text-[14px] font-semibold tracking-[-0.01em]">
          <EunomiaMark size={22} />
          Eunomia
        </span>
        <ThemeToggle />
      </header>

      <main className="flex-1 flex items-center justify-center py-10">
        <div className="panel w-full max-w-[440px] p-7">
          {!info ? (
            error ? (
              <div className="flex flex-col gap-3">
                <h1 className="page-title">Can&apos;t connect this app</h1>
                <ErrorLine error={error} />
              </div>
            ) : (
              <div className="flex flex-col gap-4" aria-busy="true" aria-label="Loading">
                <div className="skeleton h-6 w-2/3" />
                <div className="skeleton h-4 w-full" />
                <div className="skeleton h-20 w-full" />
              </div>
            )
          ) : (
            <div className="flex flex-col gap-5">
              <div className="flex items-center gap-3">
                {info.client.logo_uri && !logoFailed ? (
                  // eslint-disable-next-line @next/next/no-img-element
                  <img
                    src={info.client.logo_uri}
                    alt=""
                    width={36}
                    height={36}
                    referrerPolicy="no-referrer"
                    onError={() => setLogoFailed(true)}
                    className="rounded-[8px] shrink-0"
                    style={{ border: "var(--hair) solid var(--border)", background: "var(--surface-raised)" }}
                  />
                ) : null}
                <h1 className="page-title">Connect {info.client.name}?</h1>
              </div>

              <p className="text-[13.5px] leading-[1.6]" style={{ color: "var(--ink-dim)" }}>
                <span className="font-medium" style={{ color: "var(--ink)" }}>
                  {info.client.name}
                </span>{" "}
                wants to use your memory as{" "}
                <span className="font-medium" style={{ color: "var(--ink)" }}>
                  {info.user_email}
                </span>
                . It will be able to:
              </p>

              <ul className="ledger hairline-rows">
                {info.scopes.map((s) => (
                  <li key={s.scope} className="px-3 py-2.5 text-[13px]">
                    {s.description}
                  </li>
                ))}
              </ul>

              <p className="label">
                After you decide, you go back to{" "}
                <code className="font-mono" style={{ color: "var(--ink-dim)" }}>
                  {info.redirect_host}
                </code>
                .
              </p>

              {info.loopback && (
                <div
                  className="flex items-start gap-2 rounded-[7px] px-3 py-2 text-[12.5px] leading-[1.45]"
                  style={{ background: "var(--critical-soft)", color: "var(--critical)" }}
                >
                  <ShieldAlert size={14} strokeWidth={1.75} className="mt-[2px] shrink-0" aria-hidden />
                  <span>This app is on your own computer. Only continue if you just started this connection from it.</span>
                </div>
              )}

              {error && <ErrorLine error={error} />}

              <div className="flex gap-2">
                <button onClick={() => decide(true)} disabled={busy !== null} aria-busy={busy === "allow"} className="btn btn-primary h-9 px-4">
                  {busy === "allow" && <Loader2 size={14} strokeWidth={1.75} className="animate-spin" aria-hidden />}
                  Allow
                </button>
                <button onClick={() => decide(false)} disabled={busy !== null} aria-busy={busy === "deny"} className="btn h-9 px-4">
                  Deny
                </button>
              </div>
              <p className="label">You can disconnect it any time in Settings, under Connected apps.</p>
            </div>
          )}
        </div>
      </main>
    </div>
  );
}
