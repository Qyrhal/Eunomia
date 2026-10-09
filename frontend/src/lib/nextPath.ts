/** Where to go after sign-in or onboarding: `?next=` when it is a same-site path (the OAuth consent page, the page a
 * session ended on), else the dashboard. Reads the current URL, so call it from an effect or an event handler. */
export function nextPath(): string {
  const next = new URLSearchParams(window.location.search).get("next");
  return next && next.startsWith("/") && !next.startsWith("//") && !next.includes("\\") ? next : "/";
}

/** `path` carrying the current `next` along, so a detour (register, onboarding) still ends where the visitor was headed. */
export function keepNext(path: string): string {
  const next = nextPath();
  return next === "/" ? path : `${path}?next=${encodeURIComponent(next)}`;
}
