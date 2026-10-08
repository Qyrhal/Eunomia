"use client";

import { useEffect, useState } from "react";
import { useRouter, usePathname } from "next/navigation";
import { useQueryClient } from "@tanstack/react-query";
import { bootstrapQuery, meQuery } from "@/lib/queries/auth";

/** Gates every page under the `(app)` route group behind a session check.
 * No session (401 from `/api/auth/me`) sends a fresh visitor to `/register`
 * if no account exists yet anywhere on the instance, else `/login`.
 * An existing-but-unonboarded session is sent to `/onboarding` first. */
export default function AuthGuard({ children }: { children: React.ReactNode }) {
  const router = useRouter();
  const pathname = usePathname();
  const [ready, setReady] = useState(false);
  const queryClient = useQueryClient();

  useEffect(() => {
    let cancelled = false;
    queryClient
      .fetchQuery(meQuery())
      .then((me) => {
        if (cancelled) return;
        if (!me.onboarded) {
          router.replace("/onboarding");
          return;
        }
        setReady(true);
      })
      .catch(async () => {
        if (cancelled) return;
        const { has_users } = await queryClient.fetchQuery(bootstrapQuery()).catch(() => ({ has_users: true }));
        router.replace(has_users ? "/login" : "/register");
      });
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [pathname]);

  if (!ready) return null;
  return <>{children}</>;
}
