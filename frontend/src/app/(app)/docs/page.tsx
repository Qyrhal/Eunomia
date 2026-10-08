"use client";

import { isValidElement, useEffect, useRef, useState, type ReactNode } from "react";
import ReactMarkdown, { type Components } from "react-markdown";
import remarkGfm from "remark-gfm";
import { ArrowLeft, ArrowRight, Check, Copy, Link2 } from "lucide-react";
import { docs } from "@/lib/api";

type Topic = { topic: string; title: string };

function textOf(node: ReactNode): string {
  if (typeof node === "string" || typeof node === "number") return String(node);
  if (Array.isArray(node)) return node.map(textOf).join("");
  if (isValidElement(node)) return textOf((node.props as { children?: ReactNode }).children);
  return "";
}

// ponytail: duplicate headings in one doc share an id; the first one wins the anchor.
const slugify = (s: string) =>
  s
    .toLowerCase()
    .replace(/[^\w\s-]/g, "")
    .trim()
    .replace(/\s+/g, "-");

function CopyButton({ text, label }: { text: () => string; label: string }) {
  const [copied, setCopied] = useState(false);
  return (
    <button
      type="button"
      className="btn btn-sm btn-icon btn-ghost"
      aria-label={copied ? "Copied" : label}
      title={copied ? "Copied" : label}
      onClick={() =>
        navigator.clipboard?.writeText(text()).then(() => {
          setCopied(true);
          setTimeout(() => setCopied(false), 1400);
        })
      }
    >
      <span key={String(copied)} className="pop-in inline-flex">
        {copied ? <Check size={14} strokeWidth={1.75} /> : <Copy size={14} strokeWidth={1.75} />}
      </span>
    </button>
  );
}

function CodeBlock({ children }: { children?: ReactNode }) {
  const ref = useRef<HTMLPreElement>(null);
  return (
    <div className="relative">
      <pre ref={ref} className="pr-11">
        {children}
      </pre>
      <div className="absolute top-1.5 right-1.5 rounded-[7px]" style={{ background: "var(--surface-raised)" }}>
        <CopyButton label="Copy code" text={() => ref.current?.innerText.trimEnd() ?? ""} />
      </div>
    </div>
  );
}

function Heading({ level, children }: { level: 2 | 3; children?: ReactNode }) {
  const Tag = `h${level}` as const;
  const id = slugify(textOf(children));
  return (
    <Tag id={id} className="group scroll-mt-6">
      {children}
      <a
        href={`#${id}`}
        aria-hidden
        tabIndex={-1}
        className="ml-2 inline-flex align-middle opacity-0 transition-opacity duration-150 group-hover:opacity-100"
        style={{ color: "var(--ink-faint)", textDecoration: "none" }}
      >
        <Link2 size={14} strokeWidth={1.75} />
      </a>
    </Tag>
  );
}

// The repo's docs/ folder, served by the same `docs` tool agents call over MCP.
export default function DocsPage() {
  const [topics, setTopics] = useState<Topic[]>([]);
  const [topic, setTopic] = useState("quickstart");
  const [body, setBody] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [outline, setOutline] = useState<{ id: string; text: string }[]>([]);
  const [active, setActive] = useState<string | null>(null);
  const articleRef = useRef<HTMLElement>(null);
  const first = useRef(true);

  useEffect(() => {
    docs.list().then((r) => setTopics(r.docs)).catch(() => setError("Could not load the docs."));
  }, []);

  useEffect(() => {
    docs
      .get(topic)
      .then((d) => {
        setError(null);
        setBody(d.markdown);
      })
      .catch(() => setError("Could not load that doc."));
    if (first.current) first.current = false;
    else window.scrollTo({ top: 0 });
  }, [topic]);

  // "On this page": read the h2s back out of the rendered article.
  useEffect(() => {
    const el = articleRef.current;
    if (!el || body === null) return;
    const hs = [...el.querySelectorAll<HTMLHeadingElement>("h2[id]")];
    setOutline(hs.map((h) => ({ id: h.id, text: h.textContent ?? "" })));
    setActive(hs[0]?.id ?? null);
    const io = new IntersectionObserver(
      (entries) => {
        const hit = entries.find((e) => e.isIntersecting);
        if (hit) setActive(hit.target.id);
      },
      { rootMargin: "0px 0px -70% 0px" },
    );
    hs.forEach((h) => io.observe(h));
    return () => io.disconnect();
  }, [body]);

  const components: Components = {
    h2: ({ children }) => <Heading level={2}>{children}</Heading>,
    h3: ({ children }) => <Heading level={3}>{children}</Heading>,
    pre: ({ children }) => <CodeBlock>{children}</CodeBlock>,
    a: ({ href = "", children, title }) => {
      const props = { children, title };
      const doc = href.match(/^([\w-]+)\.md(#.*)?$/);
      if (doc)
        return (
          <a
            {...props}
            href={`#${doc[1]}`}
            onClick={(e) => {
              e.preventDefault();
              setTopic(doc[1]);
            }}
          />
        );
      if (href.startsWith("#")) return <a {...props} href={href} />;
      return <a {...props} href={href} target="_blank" rel="noopener noreferrer" />;
    },
  };

  const index = topics.findIndex((t) => t.topic === topic);
  const prev = index > 0 ? topics[index - 1] : null;
  const next = index >= 0 && index < topics.length - 1 ? topics[index + 1] : null;

  return (
    <div className="grid gap-5 md:grid-cols-[176px_minmax(0,1fr)] md:gap-10 xl:grid-cols-[176px_minmax(0,760px)_184px]">
      <nav aria-label="Docs" className="md:sticky md:top-8 md:self-start">
        <ul className="hidden md:flex flex-col gap-0.5">
          {topics.length === 0 &&
            [64, 80, 72, 92, 60].map((w) => (
              <li key={w} className="h-8 flex items-center px-2.5">
                <span className="skeleton h-3" style={{ width: `${w}%` }} />
              </li>
            ))}
          {topics.map((t) => (
            <li key={t.topic}>
              <button
                onClick={() => setTopic(t.topic)}
                aria-current={t.topic === topic ? "page" : undefined}
                data-active={t.topic === topic ? "" : undefined}
                className="nav-link w-full text-left h-8 px-2.5 rounded-[7px] text-[13px] truncate active:scale-[0.98]"
              >
                {t.title}
              </button>
            </li>
          ))}
        </ul>
        <label className="md:hidden flex flex-col gap-1.5">
          <span className="label">Doc</span>
          <select className="field h-9 px-2.5 text-[14px]" value={topic} onChange={(e) => setTopic(e.target.value)}>
            {topics.length === 0 && <option value={topic}>Loading docs</option>}
            {topics.map((t) => (
              <option key={t.topic} value={t.topic}>
                {t.title}
              </option>
            ))}
          </select>
        </label>
      </nav>

      <div className="min-w-0 flex flex-col gap-4">
        <article ref={articleRef} className="ledger px-5 py-6 md:px-10 md:py-9 text-[14px] leading-[1.7] docs">
          {error ? (
            <p className="text-[13px] rounded-[7px] px-3 py-2" style={{ color: "var(--critical)", background: "var(--critical-soft)" }}>
              {error} Check that the backend is running, then reload the page.
            </p>
          ) : body === null ? (
            <div className="flex flex-col gap-3" aria-busy="true" aria-label="Loading doc">
              <span className="skeleton h-7 w-48 mb-3" />
              {[100, 94, 88, 97, 60].map((w) => (
                <span key={w} className="skeleton h-3.5" style={{ width: `${w}%` }} />
              ))}
            </div>
          ) : (
            <div className="md">
              <ReactMarkdown remarkPlugins={[remarkGfm]} components={components}>
                {body}
              </ReactMarkdown>
            </div>
          )}
        </article>

        {(prev || next) && (
          <div className="grid grid-cols-2 gap-3">
            {prev ? (
              <button onClick={() => setTopic(prev.topic)} className="btn h-auto py-2.5 flex-col items-start gap-1 text-left min-w-0">
                <span className="label inline-flex items-center gap-1">
                  <ArrowLeft size={12} strokeWidth={1.75} /> Previous
                </span>
                <span className="truncate max-w-full">{prev.title}</span>
              </button>
            ) : (
              <span />
            )}
            {next && (
              <button onClick={() => setTopic(next.topic)} className="btn h-auto py-2.5 flex-col items-end gap-1 text-right min-w-0">
                <span className="label inline-flex items-center gap-1">
                  Next <ArrowRight size={12} strokeWidth={1.75} />
                </span>
                <span className="truncate max-w-full">{next.title}</span>
              </button>
            )}
          </div>
        )}
      </div>

      {outline.length > 1 && (
        <aside className="hidden xl:block sticky top-8 self-start" aria-label="On this page">
          <p className="label mb-2 pl-3">On this page</p>
          <ul className="flex flex-col text-[12.5px]" style={{ borderLeft: "1px solid var(--border)" }}>
            {outline.map((h) => (
              <li key={h.id}>
                <a
                  href={`#${h.id}`}
                  className="block py-1 pl-3 -ml-px transition-colors duration-150"
                  style={{
                    borderLeft: `1px solid ${active === h.id ? "var(--ink)" : "transparent"}`,
                    color: active === h.id ? "var(--ink)" : "var(--ink-faint)",
                  }}
                >
                  {h.text}
                </a>
              </li>
            ))}
          </ul>
        </aside>
      )}
    </div>
  );
}
