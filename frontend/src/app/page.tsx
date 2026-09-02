"use client";

import { useEffect, useMemo, useState } from "react";
import Link from "next/link";
import { Bot, Calendar, Mail, Landmark, Mic } from "lucide-react";
import {
  Bar,
  BarChart,
  Cell,
  LabelList,
  ResponsiveContainer,
  Tooltip,
  XAxis,
} from "recharts";
import HourRing from "@/components/HourRing";
import {
  AiContribution,
  Completion,
  FinanceSummary,
  Overview,
  PocketSummary,
  PriorityBreakdown,
  ProjectBreakdown,
  Snapshot,
  WeekOverWeek,
  api,
} from "@/lib/api";

const PRIORITY_COLOR = ["var(--text-muted)", "var(--series-2)", "var(--series-1)", "var(--critical)"];
const CATEGORY_COLORS = ["var(--series-1)", "var(--series-2)", "var(--series-3)", "var(--series-4)"];

function weekBuckets(completions: Completion[]) {
  const byWeek = new Map<string, number>();
  for (const { day, count } of completions) {
    const d = new Date(day + "T00:00:00");
    const monday = new Date(d);
    monday.setDate(d.getDate() - ((d.getDay() + 6) % 7));
    const key = monday.toISOString().slice(0, 10);
    byWeek.set(key, (byWeek.get(key) || 0) + count);
  }
  const weeks = [...byWeek.entries()].sort(([a], [b]) => (a < b ? -1 : 1)).slice(-8);
  return weeks.map(([week, count], i) => {
    const prev = weeks[i - 1]?.[1];
    const delta = prev ? Math.round(((count - prev) / prev) * 100) : null;
    return {
      week: new Date(week + "T00:00:00").toLocaleDateString(undefined, { month: "short", day: "numeric" }),
      count,
      delta,
    };
  });
}

function weekdayDots(completions: Completion[]) {
  const totals = [0, 0, 0, 0, 0, 0, 0]; // Mon..Sun
  for (const { day, count } of completions) {
    const d = new Date(day + "T00:00:00");
    totals[(d.getDay() + 6) % 7] += count;
  }
  const max = Math.max(1, ...totals);
  const labels = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
  return labels.map((label, i) => ({ label, count: totals[i], dots: Math.round((totals[i] / max) * 8) }));
}

export default function Dashboard() {
  const [overview, setOverview] = useState<Overview | null>(null);
  const [completions, setCompletions] = useState<Completion[]>([]);
  const [breakdown, setBreakdown] = useState<ProjectBreakdown[]>([]);
  const [priority, setPriority] = useState<PriorityBreakdown[]>([]);
  const [ai, setAi] = useState<AiContribution | null>(null);
  const [wow, setWow] = useState<WeekOverWeek | null>(null);
  const [snapshot, setSnapshot] = useState<Snapshot | null>(null);
  const [finance, setFinance] = useState<FinanceSummary | null>(null);
  const [financeConnected, setFinanceConnected] = useState(true);
  const [pocket, setPocket] = useState<PocketSummary | null>(null);
  const [pocketConnected, setPocketConnected] = useState(true);
  const [today, setToday] = useState("");

  useEffect(() => {
    setToday(new Date().toLocaleDateString(undefined, { weekday: "long", month: "long", day: "numeric" }));
  }, []);

  useEffect(() => {
    api.get<Overview>("/api/analytics/overview").then(setOverview).catch(() => {});
    api.get<Completion[]>("/api/analytics/completions?days=56").then(setCompletions).catch(() => {});
    api.get<ProjectBreakdown[]>("/api/analytics/project-breakdown").then(setBreakdown).catch(() => {});
    api.get<PriorityBreakdown[]>("/api/analytics/priority-breakdown").then(setPriority).catch(() => {});
    api.get<AiContribution>("/api/analytics/ai-contribution").then(setAi).catch(() => {});
    api.get<WeekOverWeek>("/api/analytics/week-over-week").then(setWow).catch(() => {});
    api.get<Snapshot>("/api/connectors/snapshot").then(setSnapshot).catch(() => {});
    api
      .get<FinanceSummary>("/api/connectors/up_bank/finance-summary?days=30")
      .then(setFinance)
      .catch(() => setFinanceConnected(false));
    api
      .get<PocketSummary>("/api/connectors/pocketai/summary?days=30")
      .then(setPocket)
      .catch(() => setPocketConnected(false));
  }, []);

  const weekly = useMemo(() => weekBuckets(completions), [completions]);
  const dots = useMemo(() => weekdayDots(completions), [completions]);
  const aiTotal = (ai?.ai_created ?? 0) + (ai?.human_created ?? 0);
  const aiPct = aiTotal ? Math.round(((ai?.ai_created ?? 0) / aiTotal) * 100) : 0;
  const completionRate =
    overview && overview.open + overview.completed
      ? Math.round((overview.completed / (overview.open + overview.completed)) * 100)
      : 0;
  const spend30d = finance?.spend_by_category.reduce((sum, c) => sum + c.amount, 0) ?? 0;
  const maxCategory = Math.max(1, ...(finance?.spend_by_category.map((c) => c.amount) ?? [0]));

  return (
    <div className="flex flex-col gap-7 max-w-6xl">
      <div className="flex items-end justify-between">
        <div>
          <div className="eyebrow mb-2">{today || " "}</div>
          <h1 className="font-display text-4xl" style={{ color: "var(--text-primary)" }}>
            The register
          </h1>
        </div>
        <div className="flex gap-2">
          <Link href="/tasks" className="px-4 py-2 text-[13px] font-medium ledger">
            Tasks
          </Link>
          <Link href="/finance" className="px-4 py-2 text-[13px] font-medium ledger">
            Finance
          </Link>
        </div>
      </div>

      {/* stat tiles */}
      <div className="grid grid-cols-2 md:grid-cols-5 gap-3">
        {[
          { label: "Due today", value: overview?.due_today },
          { label: "Open", value: overview?.open },
          { label: "Overdue", value: overview?.overdue, color: "var(--critical)" },
          { label: "Flagged", value: overview?.flagged, color: "var(--warning)" },
          { label: "Completed", value: overview?.completed, color: "var(--good)" },
        ].map((t) => (
          <div key={t.label} className="ledger p-5">
            <div className="font-mono text-[28px] leading-none" style={{ color: t.color ?? "var(--text-primary)" }}>
              {t.value ?? "–"}
            </div>
            <div className="eyebrow mt-2">{t.label}</div>
          </div>
        ))}
      </div>

      {/* completions bar + ai split */}
      <div className="grid md:grid-cols-[1.4fr_1fr] gap-5">
        <div className="ledger p-6">
          <div className="eyebrow mb-1">Completions, by week</div>
          <div className="font-mono text-[28px] mb-4">{overview?.completed ?? "–"}</div>
          <ResponsiveContainer width="100%" height={190}>
            <BarChart data={weekly} margin={{ top: 24 }}>
              <XAxis dataKey="week" stroke="var(--text-muted)" fontSize={10.5} tickLine={false} axisLine={false} fontFamily="var(--font-mono)" />
              <Tooltip
                cursor={{ fill: "var(--surface-2)" }}
                contentStyle={{ background: "var(--surface)", border: "1px solid var(--border)", fontSize: 12, borderRadius: 4 }}
              />
              <Bar dataKey="count" radius={[2, 2, 0, 0]} maxBarSize={28}>
                <LabelList
                  dataKey="delta"
                  position="top"
                  formatter={(v: React.ReactNode) =>
                    v === null || v === undefined || v === "" ? "" : `${Number(v) > 0 ? "+" : ""}${v}%`
                  }
                  style={{ fill: "var(--text-secondary)", fontSize: 10.5, fontWeight: 600, fontFamily: "var(--font-mono)" }}
                />
                {weekly.map((_, i) => (
                  <Cell key={i} fill="var(--accent)" />
                ))}
              </Bar>
            </BarChart>
          </ResponsiveContainer>
        </div>

        <div className="ledger p-6 flex flex-col">
          <div className="eyebrow mb-1">This week vs. last</div>
          <div className="font-mono text-[28px] mb-1">{wow?.this_week ?? "–"}</div>
          {wow?.delta_pct !== null && wow?.delta_pct !== undefined && (
            <div className="text-[12.5px] font-medium mb-5" style={{ color: wow.delta_pct >= 0 ? "var(--good)" : "var(--critical)" }}>
              {wow.delta_pct > 0 ? "+" : ""}
              {wow.delta_pct}% against {wow.last_week} last week
            </div>
          )}

          <div className="mt-auto flex flex-col gap-2 pt-4" style={{ borderTop: "1px solid var(--border)" }}>
            <div className="eyebrow">Authored by</div>
            <div className="flex h-2 overflow-hidden" style={{ background: "var(--surface-2)" }}>
              <div style={{ width: `${aiPct}%`, background: "var(--accent)" }} />
              <div style={{ width: `${100 - aiPct}%`, background: "var(--verdigris)" }} />
            </div>
            <div className="flex justify-between text-[12.5px]" style={{ color: "var(--text-secondary)" }}>
              <span className="flex items-center gap-1.5">
                <span className="w-2 h-2" style={{ background: "var(--accent)" }} /> Assistant ({ai?.ai_created ?? 0})
              </span>
              <span className="flex items-center gap-1.5">
                <span className="w-2 h-2" style={{ background: "var(--verdigris)" }} /> You ({ai?.human_created ?? 0})
              </span>
            </div>
          </div>
        </div>
      </div>

      {/* priority + projects + completion rate */}
      <div className="grid md:grid-cols-3 gap-5">
        <div className="ledger p-6">
          <div className="eyebrow mb-4">Open, by priority</div>
          <div className="flex flex-col gap-2.5">
            {priority.map((p) => (
              <div key={p.priority} className="flex items-center gap-3">
                <span className="w-16 text-[12.5px]" style={{ color: "var(--text-secondary)" }}>
                  {p.label}
                </span>
                <div className="flex-1 h-1.5" style={{ background: "var(--surface-2)" }}>
                  <div
                    className="h-1.5"
                    style={{
                      width: `${Math.round((p.count / Math.max(1, ...priority.map((x) => x.count))) * 100)}%`,
                      background: PRIORITY_COLOR[p.priority],
                    }}
                  />
                </div>
                <span className="font-mono text-[12.5px] w-5 text-right">{p.count}</span>
              </div>
            ))}
            {priority.length === 0 && (
              <span className="text-[12.5px]" style={{ color: "var(--text-muted)" }}>
                Nothing open.
              </span>
            )}
          </div>
        </div>

        <div className="ledger p-6">
          <div className="eyebrow mb-4">Open, by project</div>
          <div className="flex flex-col gap-2.5">
            {breakdown.slice(0, 5).map((b, i) => (
              <div key={b.project__name} className="flex items-center gap-3 text-[12.5px]">
                <span className="font-mono w-4" style={{ color: "var(--text-muted)" }}>
                  {String(i + 1).padStart(2, "0")}
                </span>
                <span className="flex-1 truncate">{b.project__name}</span>
                <span className="font-mono font-medium">{b.count}</span>
              </div>
            ))}
            {breakdown.length === 0 && (
              <span className="text-[12.5px]" style={{ color: "var(--text-muted)" }}>
                No projects yet.
              </span>
            )}
          </div>
        </div>

        <div
          className="ledger p-6 flex flex-col justify-between relative overflow-hidden"
          style={{ background: "var(--text-primary)", borderColor: "var(--text-primary)" }}
        >
          <div className="absolute -right-6 -bottom-6 opacity-[0.14]">
            <HourRing size={140} color="var(--plane)" ticks={12} />
          </div>
          <div className="eyebrow relative" style={{ color: "var(--plane)", opacity: 0.65 }}>
            Completion rate
          </div>
          <div className="font-mono text-5xl my-2 relative" style={{ color: "var(--plane)" }}>
            {completionRate}%
          </div>
          <div className="text-[12px] relative" style={{ color: "var(--plane)", opacity: 0.6 }}>
            {overview?.completed ?? 0} of {(overview?.completed ?? 0) + (overview?.open ?? 0)} tasks, all time
          </div>
        </div>
      </div>

      {/* money */}
      <div className="ledger p-6">
        <div className="flex items-center justify-between mb-4">
          <div className="eyebrow">Money, last 30 days</div>
          <Link href="/finance" className="text-[12px]" style={{ color: "var(--accent)" }}>
            Full ledger
          </Link>
        </div>
        {financeConnected && finance ? (
          <div className="grid md:grid-cols-[auto_1fr] gap-8">
            <div className="flex md:flex-col gap-8 md:gap-6 md:w-40 shrink-0">
              <div>
                <div className="font-mono text-2xl">${finance.balance.toFixed(2)}</div>
                <div className="eyebrow mt-1.5">Balance</div>
              </div>
              <div>
                <div className="font-mono text-2xl">${spend30d.toFixed(2)}</div>
                <div className="eyebrow mt-1.5">Spent</div>
              </div>
              <div>
                <div className="font-mono text-2xl">{finance.recent_transactions.length}</div>
                <div className="eyebrow mt-1.5">Transactions</div>
              </div>
            </div>
            <div className="flex flex-col gap-4">
              <div>
                <div className="eyebrow mb-2">Spend by day</div>
                {finance.spend_by_day.length > 0 ? (
                  <ResponsiveContainer width="100%" height={100}>
                    <BarChart data={finance.spend_by_day}>
                      <Tooltip
                        cursor={{ fill: "var(--surface-2)" }}
                        formatter={(v) => [`$${Number(v).toFixed(2)}`, "Spent"]}
                        labelFormatter={(d) => (d ? new Date(`${d}T00:00:00`).toLocaleDateString(undefined, { month: "short", day: "numeric" }) : "")}
                        contentStyle={{ background: "var(--surface)", border: "1px solid var(--border)", fontSize: 12, borderRadius: 4 }}
                      />
                      <Bar dataKey="amount" radius={[2, 2, 0, 0]} fill="var(--series-1)" maxBarSize={10} />
                    </BarChart>
                  </ResponsiveContainer>
                ) : (
                  <span className="text-[12.5px]" style={{ color: "var(--text-muted)" }}>
                    No spending in this period.
                  </span>
                )}
              </div>

              <div className="flex flex-col gap-2 pt-2" style={{ borderTop: "1px solid var(--border)" }}>
                <div className="eyebrow">Top categories</div>
                {finance.spend_by_category.slice(0, 3).map((c, i) => (
                  <div key={c.category} className="flex items-center gap-3">
                    <span className="w-28 text-[12.5px] truncate" style={{ color: "var(--text-secondary)" }}>
                      {c.category}
                    </span>
                    <div className="flex-1 h-1.5" style={{ background: "var(--surface-2)" }}>
                      <div
                        className="h-1.5"
                        style={{ width: `${Math.round((c.amount / maxCategory) * 100)}%`, background: CATEGORY_COLORS[i % CATEGORY_COLORS.length] }}
                      />
                    </div>
                    <span className="font-mono text-[12.5px] font-medium w-16 text-right">${c.amount.toFixed(2)}</span>
                  </div>
                ))}
              </div>
            </div>
          </div>
        ) : (
          <div className="flex items-center justify-between">
            <p className="text-[13px]" style={{ color: "var(--text-muted)" }}>
              Connect Up Bank to see balance, spend, and category breakdown here.
            </p>
            <Link href="/connectors" className="px-4 py-2 text-[13px] font-medium text-white shrink-0" style={{ background: "var(--accent)" }}>
              Connect
            </Link>
          </div>
        )}
      </div>

      {/* pocket */}
      <div className="ledger p-6">
        <div className="eyebrow mb-4">Pocket, last 30 days</div>
        {pocketConnected && pocket ? (
          <div className="grid md:grid-cols-[auto_1fr] gap-8">
            <div className="flex md:flex-col gap-8 md:gap-6 md:w-40 shrink-0">
              <div>
                <div className="font-mono text-2xl">{pocket.recordings_count}</div>
                <div className="eyebrow mt-1.5">Recordings</div>
              </div>
              <div>
                <div className="font-mono text-2xl">{(pocket.total_duration_minutes / 60).toFixed(1)}h</div>
                <div className="eyebrow mt-1.5">Recorded time</div>
              </div>
            </div>
            <div className="flex flex-col gap-4">
              <div className="flex flex-col gap-2">
                <div className="eyebrow">By tag</div>
                {pocket.tag_breakdown.length > 0 ? (
                  pocket.tag_breakdown.slice(0, 4).map((t, i) => (
                    <div key={t.tag} className="flex items-center gap-3">
                      <span className="w-24 text-[12.5px] truncate capitalize" style={{ color: "var(--text-secondary)" }}>
                        {t.tag}
                      </span>
                      <div className="flex-1 h-1.5" style={{ background: "var(--surface-2)" }}>
                        <div
                          className="h-1.5"
                          style={{
                            width: `${Math.round((t.count / Math.max(1, ...pocket.tag_breakdown.map((x) => x.count))) * 100)}%`,
                            background: CATEGORY_COLORS[i % CATEGORY_COLORS.length],
                          }}
                        />
                      </div>
                      <span className="font-mono text-[12.5px] font-medium w-8 text-right">{t.count}</span>
                    </div>
                  ))
                ) : (
                  <span className="text-[12.5px]" style={{ color: "var(--text-muted)" }}>
                    No tagged recordings.
                  </span>
                )}
              </div>

              <div className="flex flex-col gap-1.5 pt-2" style={{ borderTop: "1px solid var(--border)" }}>
                <div className="eyebrow mb-1">Recent</div>
                {pocket.recent_recordings.slice(0, 4).map((r, i) => (
                  <div key={i} className="flex items-center justify-between text-[12.5px]">
                    <span className="truncate">{r.title}</span>
                    <span className="font-mono shrink-0 ml-3" style={{ color: "var(--text-muted)" }}>
                      {r.duration_minutes.toFixed(0)}m
                    </span>
                  </div>
                ))}
                {pocket.recent_recordings.length === 0 && (
                  <span className="text-[12.5px]" style={{ color: "var(--text-muted)" }}>
                    No recordings in this period.
                  </span>
                )}
              </div>
            </div>
          </div>
        ) : (
          <div className="flex items-center justify-between">
            <p className="text-[13px]" style={{ color: "var(--text-muted)" }}>
              Connect PocketAI to see recording activity here.
            </p>
            <Link href="/connectors" className="px-4 py-2 text-[13px] font-medium text-white shrink-0" style={{ background: "var(--accent)" }}>
              Connect
            </Link>
          </div>
        )}
      </div>

      {/* connected accounts + weekday activity */}
      <div className="grid md:grid-cols-2 gap-5">
        <div className="ledger p-6">
          <div className="flex items-center justify-between mb-4">
            <div className="eyebrow">Connected accounts</div>
            <Link href="/connectors" className="text-[12px]" style={{ color: "var(--accent)" }}>
              Manage
            </Link>
          </div>
          <div className="flex flex-col gap-3.5 hairline-rows">
            <ConnectorRow
              icon={<Calendar size={15} />}
              label="Google Calendar"
              value={snapshot?.google && !snapshot.google.error ? `${snapshot.google.calendar_events_today} events today` : null}
            />
            <ConnectorRow
              icon={<Mail size={15} />}
              label="Gmail"
              value={snapshot?.google && !snapshot.google.error ? `${snapshot.google.gmail_unread} unread` : null}
            />
            <ConnectorRow
              icon={<Landmark size={15} />}
              label="Up Bank"
              value={
                snapshot?.up_bank && !snapshot.up_bank.error
                  ? `${snapshot.up_bank.transaction_count} txns · $${snapshot.up_bank.spent.toFixed(2)} this week`
                  : null
              }
            />
            <ConnectorRow
              icon={<Mic size={15} />}
              label="PocketAI"
              value={snapshot?.pocketai && !snapshot.pocketai.error ? `${snapshot.pocketai.recordings_count} recent recordings` : null}
            />
          </div>
        </div>

        <div className="ledger p-6">
          <div className="flex items-center justify-between mb-4">
            <div className="eyebrow">Completions, by weekday</div>
            <span className="eyebrow">last 8 weeks</span>
          </div>
          <div className="flex flex-col gap-2.5">
            {dots.map((d) => (
              <div key={d.label} className="flex items-center gap-3">
                <span className="w-8 text-[12px] font-mono" style={{ color: "var(--text-muted)" }}>
                  {d.label}
                </span>
                <div className="flex gap-1">
                  {Array.from({ length: 8 }).map((_, i) => (
                    <span key={i} className="w-2.5 h-2.5" style={{ background: i < d.dots ? "var(--accent)" : "var(--surface-2)" }} />
                  ))}
                </div>
                <span className="font-mono text-[12px] ml-1">{d.count}</span>
              </div>
            ))}
          </div>
        </div>
      </div>

      <div className="ledger p-5 flex items-center gap-4">
        <div className="w-9 h-9 flex items-center justify-center shrink-0" style={{ background: "var(--surface-2)" }}>
          <Bot size={16} />
        </div>
        <div className="flex-1 text-[13px]" style={{ color: "var(--text-secondary)" }}>
          Your assistant has entered <b style={{ color: "var(--text-primary)" }}>{ai?.ai_created ?? 0}</b> of your{" "}
          <b style={{ color: "var(--text-primary)" }}>{aiTotal}</b> open tasks into the register.
        </div>
        <Link href="/chat" className="px-4 py-2 text-[13px] font-medium shrink-0 text-white" style={{ background: "var(--accent)" }}>
          Open assistant
        </Link>
      </div>
    </div>
  );
}

function ConnectorRow({ icon, label, value }: { icon: React.ReactNode; label: string; value: string | null }) {
  return (
    <div className="flex items-center gap-3 pt-3.5 first:pt-0">
      <div className="w-7 h-7 flex items-center justify-center shrink-0" style={{ color: "var(--text-secondary)" }}>
        {icon}
      </div>
      <span className="text-[13px] flex-1">{label}</span>
      <span className="text-[12px] font-medium font-mono" style={{ color: value ? "var(--good)" : "var(--text-muted)" }}>
        {value ?? "not connected"}
      </span>
    </div>
  );
}
