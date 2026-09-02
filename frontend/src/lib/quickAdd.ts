// Parses natural shorthand typed into the quick-add box, e.g.
// "call mom tomorrow 3pm #family" -> { title: "call mom", tags: ["family"], due_at: <iso> }
// Deliberately simple regex matching, not general NLP.

const WEEKDAYS = ["sunday", "monday", "tuesday", "wednesday", "thursday", "friday", "saturday"];

function atTime(base: Date, hours: number, minutes: number): Date {
  const d = new Date(base);
  d.setHours(hours, minutes, 0, 0);
  return d;
}

function daysFromNow(days: number): Date {
  return new Date(Date.now() + days * 86_400_000);
}

function applyTimeOfDay(date: Date, text: string): { date: Date; consumed: string } {
  const match = text.match(/\bat\s+(\d{1,2})(?::(\d{2}))?\s*(am|pm)?\b/i) || text.match(/\b(\d{1,2})(?::(\d{2}))?\s*(am|pm)\b/i);
  if (!match) return { date: atTime(date, 9, 0), consumed: "" };
  let hours = parseInt(match[1], 10);
  const minutes = match[2] ? parseInt(match[2], 10) : 0;
  const meridiem = match[3]?.toLowerCase();
  if (meridiem === "pm" && hours < 12) hours += 12;
  if (meridiem === "am" && hours === 12) hours = 0;
  return { date: atTime(date, hours, minutes), consumed: match[0] };
}

export function parseQuickAdd(raw: string): { title: string; tags: string[]; due_at: string } {
  let text = raw;
  const tags: string[] = [];
  text = text.replace(/#(\w+)/g, (_, tag) => {
    tags.push(tag);
    return "";
  });

  let due: Date | null = null;

  if (/\btoday\b/i.test(text)) {
    due = new Date();
    text = text.replace(/\btoday\b/i, "");
  } else if (/\btomorrow\b/i.test(text)) {
    due = daysFromNow(1);
    text = text.replace(/\btomorrow\b/i, "");
  } else {
    const inDays = text.match(/\bin (\d+) days?\b/i);
    if (inDays) {
      due = daysFromNow(parseInt(inDays[1], 10));
      text = text.replace(inDays[0], "");
    } else {
      const nextWeekday = text.match(/\bnext (sunday|monday|tuesday|wednesday|thursday|friday|saturday)\b/i);
      if (nextWeekday) {
        const target = WEEKDAYS.indexOf(nextWeekday[1].toLowerCase());
        const delta = ((target - new Date().getDay() + 7) % 7 || 7) + 0;
        due = daysFromNow(delta);
        text = text.replace(nextWeekday[0], "");
      }
    }
  }

  if (due) {
    const { date, consumed } = applyTimeOfDay(due, text);
    due = date;
    if (consumed) text = text.replace(consumed, "");
  }

  return { title: text.replace(/\s+/g, " ").trim(), tags, due_at: due ? due.toISOString() : "" };
}
