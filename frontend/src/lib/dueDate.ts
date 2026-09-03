// Formats a task's due_at as a relative, human-scannable label instead of a
// raw locale timestamp: "Today, 3:00 PM" / "Tomorrow, 9:00 AM" / "Jun 3,
// 2:00 PM" (falls back to a year when not this year). `overdue` is true when
// the due time has passed — callers decide whether that still matters (e.g.
// a completed task isn't "overdue").
export function formatDueDate(iso: string, now: Date = new Date()): { label: string; overdue: boolean } {
  const due = new Date(iso);
  const overdue = due.getTime() < now.getTime();

  const time = due.toLocaleTimeString([], { hour: "numeric", minute: "2-digit" });
  const tomorrow = new Date(now);
  tomorrow.setDate(now.getDate() + 1);

  let day: string;
  if (due.toDateString() === now.toDateString()) {
    day = "Today";
  } else if (due.toDateString() === tomorrow.toDateString()) {
    day = "Tomorrow";
  } else {
    day = due.toLocaleDateString([], {
      month: "short",
      day: "numeric",
      year: due.getFullYear() === now.getFullYear() ? undefined : "numeric",
    });
  }

  return { label: `${day}, ${time}`, overdue };
}
