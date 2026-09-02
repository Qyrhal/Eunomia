"use client";

import { useEffect, useRef, useState } from "react";
import { Check, ChevronDown, ChevronRight, Flag, GripVertical, Link2, Pencil, Plus, Radar, Sparkles, Trash2, X } from "lucide-react";
import DueDatePicker from "@/components/DueDatePicker";
import TagInput from "@/components/TagInput";
import Toast, { ToastState } from "@/components/Toast";
import { api, GeneratedTaskDetails, Project, SuggestTasksResponse, Task, TaskContext, TaskSuggestion } from "@/lib/api";
import { parseQuickAdd } from "@/lib/quickAdd";

const RECURRENCES: { value: Task["recurrence"]; label: string }[] = [
  { value: "none", label: "Doesn't repeat" },
  { value: "daily", label: "Daily" },
  { value: "weekly", label: "Weekly" },
  { value: "monthly", label: "Monthly" },
  { value: "yearly", label: "Yearly" },
];

function RecurrenceSelect({ value, onChange }: { value: Task["recurrence"]; onChange: (v: Task["recurrence"]) => void }) {
  return (
    <select
      value={value}
      onChange={(e) => onChange(e.target.value as Task["recurrence"])}
      className="field px-2 py-1 text-[12px]"
      aria-label="Repeat"
    >
      {RECURRENCES.map((r) => (
        <option key={r.value} value={r.value}>
          {r.label}
        </option>
      ))}
    </select>
  );
}

const PRIORITIES: { value: 0 | 1 | 2 | 3; label: string; color: string }[] = [
  { value: 0, label: "None", color: "var(--text-muted)" },
  { value: 1, label: "Low", color: "var(--series-2)" },
  { value: 2, label: "Medium", color: "var(--series-1)" },
  { value: 3, label: "High", color: "var(--critical)" },
];

function RingCheck({ checked, onClick }: { checked: boolean; onClick: () => void }) {
  return (
    <button onClick={onClick} className="shrink-0 w-[18px] h-[18px] cursor-pointer" aria-label={checked ? "Mark not done" : "Mark done"}>
      <svg width={18} height={18} viewBox="0 0 18 18" fill="none">
        <circle cx="9" cy="9" r="7.5" stroke={checked ? "var(--accent)" : "var(--border-strong)"} strokeWidth="1.3" />
        {checked && <circle cx="9" cy="9" r="4.2" fill="var(--accent)" />}
      </svg>
    </button>
  );
}

function PrioritySelect({ value, onChange }: { value: 0 | 1 | 2 | 3; onChange: (v: 0 | 1 | 2 | 3) => void }) {
  return (
    <div className="flex items-center gap-1.5">
      {PRIORITIES.map((p) => (
        <button
          key={p.value}
          type="button"
          onClick={() => onChange(p.value)}
          title={p.label}
          className="w-6 h-6 flex items-center justify-center rounded-full"
          style={{ background: value === p.value ? "var(--surface-2)" : "transparent", border: value === p.value ? "1px solid var(--border-strong)" : "1px solid transparent" }}
        >
          <span className="w-2.5 h-2.5 rounded-full" style={{ background: p.color }} />
        </button>
      ))}
    </div>
  );
}

type Draft = {
  title: string;
  notes: string;
  due_at: string;
  project: string;
  priority: 0 | 1 | 2 | 3;
  tags: string[];
  recurrence: Task["recurrence"];
};

export default function TasksPage() {
  const [projects, setProjects] = useState<Project[]>([]);
  const [activeProject, setActiveProject] = useState<string | "all">("all");
  const [tasks, setTasks] = useState<Task[]>([]);
  const [newTitle, setNewTitle] = useState("");
  const [newNotes, setNewNotes] = useState("");
  const [newDueAt, setNewDueAt] = useState("");
  const [newPriority, setNewPriority] = useState<0 | 1 | 2 | 3>(0);
  const [newRecurrence, setNewRecurrence] = useState<Task["recurrence"]>("none");
  const [generating, setGenerating] = useState(false);
  const [generateError, setGenerateError] = useState<string | null>(null);
  const [showCompleted, setShowCompleted] = useState(false);
  const [suggestions, setSuggestions] = useState<TaskSuggestion[]>([]);
  const [scanning, setScanning] = useState(false);
  const [scanError, setScanError] = useState<string | null>(null);
  const [scanned, setScanned] = useState(false);
  const [editingId, setEditingId] = useState<string | null>(null);
  const [editDraft, setEditDraft] = useState<Draft | null>(null);
  const [saving, setSaving] = useState(false);
  const [showNewProject, setShowNewProject] = useState(false);
  const [newProjectName, setNewProjectName] = useState("");
  const [newProjectColor, setNewProjectColor] = useState("#a8752f");
  const [editingProjectId, setEditingProjectId] = useState<string | null>(null);
  const [editProjectDraft, setEditProjectDraft] = useState<{ name: string; color: string } | null>(null);
  const [newTags, setNewTags] = useState<string[]>([]);
  const [allTags, setAllTags] = useState<string[]>([]);
  const [activeTag, setActiveTag] = useState<string | null>(null);
  const [openSubtasks, setOpenSubtasks] = useState<Record<string, Task[]>>({});
  const [newSubtaskTitle, setNewSubtaskTitle] = useState<Record<string, string>>({});
  const [taskContext, setTaskContext] = useState<Record<string, TaskContext>>({});
  const [toast, setToast] = useState<ToastState>(null);
  const pendingDeleteRef = useRef<{ id: string; timer: ReturnType<typeof setTimeout> } | null>(null);
  const [draggedTaskId, setDraggedTaskId] = useState<string | null>(null);
  const [draggedProjectId, setDraggedProjectId] = useState<string | null>(null);

  const loadProjects = () => api.get<Project[]>("/api/projects/").then(setProjects);
  const loadAllTags = () => api.get<{ id: number; name: string }[]>("/api/tags/").then((tags) => setAllTags(tags.map((t) => t.name)));
  const loadTasks = () => {
    const params = new URLSearchParams();
    if (activeProject !== "all") params.set("project", activeProject);
    if (!showCompleted) params.set("completed", "false");
    if (activeTag) params.set("tag", activeTag);
    params.set("top_level", "true");
    api.get<Task[]>(`/api/tasks/?${params}`).then(setTasks);
  };

  useEffect(() => {
    loadProjects();
    loadAllTags();
  }, []);
  useEffect(() => {
    loadTasks();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [activeProject, showCompleted, activeTag]);

  async function createProject() {
    if (!newProjectName.trim()) return;
    const created = await api.post<Project>("/api/projects/", { name: newProjectName, color: newProjectColor });
    setNewProjectName("");
    setNewProjectColor("#a8752f");
    setShowNewProject(false);
    await loadProjects();
    setActiveProject(created.id);
  }

  async function reorderProjects(draggedId: string, targetId: string) {
    if (draggedId === targetId) return;
    const from = projects.findIndex((p) => p.id === draggedId);
    const to = projects.findIndex((p) => p.id === targetId);
    if (from === -1 || to === -1) return;
    const reordered = [...projects];
    const [moved] = reordered.splice(from, 1);
    reordered.splice(to, 0, moved);
    setProjects(reordered);
    await Promise.all(reordered.map((p, i) => api.patch(`/api/projects/${p.id}/`, { order: i })));
  }

  function startEditProject(p: Project) {
    setEditingProjectId(p.id);
    setEditProjectDraft({ name: p.name, color: p.color });
  }

  function cancelEditProject() {
    setEditingProjectId(null);
    setEditProjectDraft(null);
  }

  async function saveEditProject() {
    if (!editingProjectId || !editProjectDraft || !editProjectDraft.name.trim()) return;
    await api.patch(`/api/projects/${editingProjectId}/`, editProjectDraft);
    setEditingProjectId(null);
    setEditProjectDraft(null);
    loadProjects();
  }

  async function deleteProject(p: Project) {
    if (!window.confirm(`Delete "${p.name}"? This will also delete all of its tasks.`)) return;
    await api.del(`/api/projects/${p.id}/`);
    await loadProjects();
    if (activeProject === p.id) {
      // switching away re-fetches tasks via the effect below, with the new
      // filter — fetching here too would race it with the stale (deleted) id
      setActiveProject("all");
    } else {
      loadTasks();
    }
  }

  async function addTask() {
    if (!newTitle.trim()) return;
    let projectId = activeProject !== "all" ? activeProject : projects[0]?.id;
    if (!projectId) {
      const created = await api.post<Project>("/api/projects/", { name: "Inbox" });
      projectId = created.id;
      loadProjects();
    }
    // shorthand like "#tag" or "tomorrow 3pm" in the title only fills in fields
    // the user hasn't already set explicitly via the date picker / tag input
    const parsed = parseQuickAdd(newTitle);
    await api.post("/api/tasks/", {
      title: parsed.title || newTitle.trim(),
      notes: newNotes,
      project: projectId,
      due_at: newDueAt || parsed.due_at || null,
      priority: newPriority,
      tags: newTags.length ? newTags : parsed.tags,
      recurrence: newRecurrence,
    });
    setNewTitle("");
    setNewNotes("");
    setNewDueAt("");
    setNewPriority(0);
    setNewTags([]);
    setNewRecurrence("none");
    loadTasks();
    loadAllTags();
  }

  async function generateDescription() {
    if (!newTitle.trim() || generating) return;
    setGenerating(true);
    setGenerateError(null);
    try {
      const activeProjectName = projects.find((p) => p.id === activeProject)?.name;
      const res = await api.post<GeneratedTaskDetails>("/api/ai/generate-task-details", {
        title: newTitle,
        notes: newNotes,
        project_name: activeProjectName,
      });
      setNewNotes(res.description);
    } catch (e) {
      setGenerateError(e instanceof Error ? e.message : "Could not generate notes.");
    } finally {
      setGenerating(false);
    }
  }

  async function scanForSuggestions() {
    if (scanning) return;
    setScanning(true);
    setScanError(null);
    try {
      const res = await api.post<SuggestTasksResponse>("/api/ai/suggest-tasks");
      setSuggestions(res.suggestions);
      setScanned(true);
    } catch (e) {
      setScanError(e instanceof Error ? e.message : "Could not scan for suggestions.");
    } finally {
      setScanning(false);
    }
  }

  async function approveSuggestion(index: number) {
    const s = suggestions[index];
    let projectId = activeProject !== "all" ? activeProject : projects[0]?.id;
    if (!projectId) {
      const created = await api.post<Project>("/api/projects/", { name: "Inbox" });
      projectId = created.id;
      loadProjects();
    }
    await api.post("/api/tasks/", { title: s.title, notes: s.notes || "", due_at: s.due_at || null, project: projectId });
    setSuggestions((cur) => cur.filter((_, i) => i !== index));
    loadTasks();
  }

  function dismissSuggestion(index: number) {
    setSuggestions((cur) => cur.filter((_, i) => i !== index));
  }

  async function toggleComplete(task: Task) {
    await api.patch(`/api/tasks/${task.id}/`, { completed: !task.completed });
    loadTasks();
  }

  async function toggleFlag(task: Task) {
    await api.patch(`/api/tasks/${task.id}/`, { flagged: !task.flagged });
    loadTasks();
  }

  function flushPendingDelete() {
    const pending = pendingDeleteRef.current;
    if (!pending) return;
    clearTimeout(pending.timer);
    api.del(`/api/tasks/${pending.id}/`);
    pendingDeleteRef.current = null;
  }

  function remove(task: Task) {
    if (editingId === task.id) setEditingId(null);
    flushPendingDelete();
    setTasks((cur) => cur.filter((x) => x.id !== task.id));
    const timer = setTimeout(() => {
      api.del(`/api/tasks/${task.id}/`);
      pendingDeleteRef.current = null;
      setToast(null);
    }, 5000);
    pendingDeleteRef.current = { id: task.id, timer };
    setToast({
      message: `Deleted "${task.title}"`,
      onUndo: () => {
        clearTimeout(timer);
        pendingDeleteRef.current = null;
        loadTasks();
      },
    });
  }

  async function reorderTasks(draggedId: string, targetId: string) {
    if (draggedId === targetId) return;
    const from = tasks.findIndex((t) => t.id === draggedId);
    const to = tasks.findIndex((t) => t.id === targetId);
    if (from === -1 || to === -1) return;
    const reordered = [...tasks];
    const [moved] = reordered.splice(from, 1);
    reordered.splice(to, 0, moved);
    setTasks(reordered);
    await Promise.all(reordered.map((t, i) => api.patch(`/api/tasks/${t.id}/`, { order: i })));
  }

  function startEdit(task: Task) {
    setEditingId(task.id);
    setEditDraft({
      title: task.title,
      notes: task.notes,
      due_at: task.due_at || "",
      project: task.project,
      priority: task.priority,
      tags: task.tags,
      recurrence: task.recurrence,
    });
  }

  function cancelEdit() {
    setEditingId(null);
    setEditDraft(null);
  }

  async function saveEdit() {
    if (!editingId || !editDraft || saving) return;
    if (!editDraft.title.trim()) return;
    setSaving(true);
    try {
      await api.patch(`/api/tasks/${editingId}/`, {
        title: editDraft.title,
        notes: editDraft.notes,
        due_at: editDraft.due_at || null,
        project: editDraft.project,
        priority: editDraft.priority,
        tags: editDraft.tags,
        recurrence: editDraft.recurrence,
      });
      setEditingId(null);
      setEditDraft(null);
      loadTasks();
      loadAllTags();
    } finally {
      setSaving(false);
    }
  }

  async function toggleSubtasks(task: Task) {
    setOpenSubtasks((cur) => {
      if (task.id in cur) {
        const next = { ...cur };
        delete next[task.id];
        return next;
      }
      return cur;
    });
    if (openSubtasks[task.id]) return;
    const subs = await api.get<Task[]>(`/api/tasks/?parent=${task.id}`);
    setOpenSubtasks((cur) => ({ ...cur, [task.id]: subs }));
    if (!(task.id in taskContext)) {
      api
        .get<TaskContext>(`/api/tasks/${task.id}/context/`)
        .then((ctx) => setTaskContext((cur) => ({ ...cur, [task.id]: ctx })))
        .catch(() => {});
    }
  }

  async function refreshSubtasks(parentId: string) {
    const subs = await api.get<Task[]>(`/api/tasks/?parent=${parentId}`);
    setOpenSubtasks((cur) => ({ ...cur, [parentId]: subs }));
  }

  async function addSubtask(task: Task) {
    const title = (newSubtaskTitle[task.id] || "").trim();
    if (!title) return;
    await api.post("/api/tasks/", { title, project: task.project, parent: task.id });
    setNewSubtaskTitle((cur) => ({ ...cur, [task.id]: "" }));
    await refreshSubtasks(task.id);
    loadTasks();
  }

  async function toggleSubtaskComplete(sub: Task, parentId: string) {
    await api.patch(`/api/tasks/${sub.id}/`, { completed: !sub.completed });
    refreshSubtasks(parentId);
  }

  async function deleteSubtask(sub: Task, parentId: string) {
    await api.del(`/api/tasks/${sub.id}/`);
    await refreshSubtasks(parentId);
    loadTasks();
  }

  return (
    <div className="flex gap-10 max-w-5xl">
      <div className="w-52 shrink-0 flex flex-col gap-0.5">
        <div className="flex items-center justify-between mb-3">
          <div className="eyebrow">Projects</div>
          <button onClick={() => setShowNewProject((v) => !v)} aria-label="New project" className="shrink-0">
            <Plus size={13} color="var(--text-muted)" />
          </button>
        </div>

        {showNewProject && (
          <div className="flex items-center gap-1.5 mb-2 px-1">
            <input
              type="color"
              value={newProjectColor}
              onChange={(e) => setNewProjectColor(e.target.value)}
              className="w-6 h-6 field p-0 shrink-0"
              aria-label="Project color"
            />
            <input
              value={newProjectName}
              onChange={(e) => setNewProjectName(e.target.value)}
              onKeyDown={(e) => e.key === "Enter" && createProject()}
              placeholder="Project name…"
              className="flex-1 field px-2 py-1 text-[12.5px] min-w-0"
              autoFocus
            />
          </div>
        )}

        <button
          onClick={() => setActiveProject("all")}
          className="text-left px-2.5 py-1.5 text-[13px]"
          style={{ color: activeProject === "all" ? "var(--text-primary)" : "var(--text-secondary)", fontWeight: activeProject === "all" ? 600 : 400 }}
        >
          All projects
        </button>
        {projects.map((p) =>
          editingProjectId === p.id && editProjectDraft ? (
            <div key={p.id} className="flex items-center gap-1.5 px-1 py-0.5">
              <input
                type="color"
                value={editProjectDraft.color}
                onChange={(e) => setEditProjectDraft({ ...editProjectDraft, color: e.target.value })}
                className="w-6 h-6 field p-0 shrink-0"
                aria-label="Project color"
              />
              <input
                value={editProjectDraft.name}
                onChange={(e) => setEditProjectDraft({ ...editProjectDraft, name: e.target.value })}
                onKeyDown={(e) => e.key === "Enter" && saveEditProject()}
                aria-label="Project name"
                className="flex-1 field px-2 py-1 text-[12.5px] min-w-0"
                autoFocus
              />
              <button onClick={saveEditProject} aria-label="Save project" className="shrink-0">
                <Check size={13} color="var(--good)" />
              </button>
              <button onClick={cancelEditProject} aria-label="Cancel" className="shrink-0">
                <X size={13} color="var(--text-muted)" />
              </button>
            </div>
          ) : (
            <div
              key={p.id}
              className="group flex items-center px-2.5 py-1.5 text-[13px]"
              onDragOver={(e) => e.preventDefault()}
              onDrop={() => draggedProjectId && reorderProjects(draggedProjectId, p.id)}
            >
              <span
                draggable
                onDragStart={() => setDraggedProjectId(p.id)}
                onDragEnd={() => setDraggedProjectId(null)}
                className="hidden group-hover:block shrink-0 cursor-grab"
                aria-label="Drag to reorder"
              >
                <GripVertical size={12} color="var(--text-muted)" />
              </span>
              <button
                onClick={() => setActiveProject(p.id)}
                className="flex-1 min-w-0 text-left flex items-center gap-2"
                style={{ color: activeProject === p.id ? "var(--text-primary)" : "var(--text-secondary)", fontWeight: activeProject === p.id ? 600 : 400 }}
              >
                <span className="w-1.5 h-1.5 shrink-0" style={{ background: p.color }} />
                <span className="truncate">{p.name}</span>
              </button>
              <span
                className="font-mono text-[11px] group-hover:hidden"
                style={{ color: "var(--text-muted)" }}
              >
                {p.open_count}
              </span>
              <div className="hidden group-hover:flex items-center gap-1.5 shrink-0">
                <button onClick={() => startEditProject(p)} aria-label="Rename project">
                  <Pencil size={12} color="var(--text-muted)" />
                </button>
                <button onClick={() => deleteProject(p)} aria-label="Delete project">
                  <Trash2 size={12} color="var(--text-muted)" />
                </button>
              </div>
            </div>
          )
        )}
        {allTags.length > 0 && (
          <div className="mt-5 pt-4" style={{ borderTop: "1px solid var(--border)" }}>
            <div className="eyebrow mb-2 px-2.5">Tags</div>
            <div className="flex flex-wrap gap-1.5 px-2.5">
              {allTags.map((tag) => (
                <button
                  key={tag}
                  onClick={() => setActiveTag((cur) => (cur === tag ? null : tag))}
                  className="px-2 py-0.5 text-[11.5px]"
                  style={{
                    background: activeTag === tag ? "var(--accent)" : "var(--surface-2)",
                    color: activeTag === tag ? "#fff" : "var(--text-secondary)",
                  }}
                >
                  #{tag}
                </button>
              ))}
            </div>
          </div>
        )}

        <label className="flex items-center gap-2 px-2.5 py-2 mt-5 text-[12px]" style={{ color: "var(--text-muted)", borderTop: "1px solid var(--border)" }}>
          <input type="checkbox" checked={showCompleted} onChange={(e) => setShowCompleted(e.target.checked)} className="mt-2" />
          Show completed
        </label>
      </div>

      <div className="flex-1 min-w-0">
        <div className="eyebrow mb-2">Tasks</div>
        <h1 className="font-display text-3xl mb-6">Open items</h1>

        <div className="ledger p-5 mb-6 flex flex-col gap-3">
          <div className="flex items-center justify-between">
            <div>
              <div className="eyebrow mb-1">Suggestions</div>
              <p className="text-[12.5px]" style={{ color: "var(--text-secondary)" }}>
                Scans your calendar, email, and recent transactions for things that should
                probably be a task. Nothing is added until you approve it.
              </p>
            </div>
            <button
              onClick={scanForSuggestions}
              disabled={scanning}
              className="field px-4 py-2 text-[13px] flex items-center gap-1.5 shrink-0 disabled:opacity-40"
              style={{ color: "var(--accent)" }}
            >
              <Radar size={14} className={scanning ? "animate-pulse" : ""} />
              {scanning ? "Scanning…" : "Scan"}
            </button>
          </div>

          {scanError && (
            <div className="text-[12px]" style={{ color: "var(--critical)" }}>
              {scanError}
            </div>
          )}

          {scanned && !scanError && suggestions.length === 0 && (
            <div className="text-[12.5px]" style={{ color: "var(--text-muted)" }}>
              Nothing found that needs a task right now.
            </div>
          )}

          {suggestions.length > 0 && (
            <ul className="hairline-rows -mx-5 px-5">
              {suggestions.map((s, i) => (
                <li key={i} className="flex items-start gap-3 py-3">
                  <div className="flex-1 min-w-0">
                    <div className="text-[13.5px]">{s.title}</div>
                    {s.notes && (
                      <div className="text-[12px] mt-0.5" style={{ color: "var(--text-secondary)" }}>
                        {s.notes}
                      </div>
                    )}
                    {s.due_at && (
                      <div className="text-[11.5px] font-mono mt-0.5" style={{ color: "var(--text-muted)" }}>
                        {new Date(s.due_at).toLocaleString()}
                      </div>
                    )}
                  </div>
                  <button
                    onClick={() => approveSuggestion(i)}
                    className="px-3 py-1.5 text-[12px] font-medium text-white shrink-0 flex items-center gap-1"
                    style={{ background: "var(--accent)" }}
                  >
                    <Check size={12} /> Approve
                  </button>
                  <button onClick={() => dismissSuggestion(i)} className="shrink-0" aria-label="Dismiss">
                    <X size={14} color="var(--text-muted)" />
                  </button>
                </li>
              ))}
            </ul>
          )}
        </div>

        <div className="ledger p-3.5 mb-6 flex flex-col gap-2.5">
          <div className="flex gap-2">
            <input
              value={newTitle}
              onChange={(e) => setNewTitle(e.target.value)}
              onKeyDown={(e) => e.key === "Enter" && !e.shiftKey && addTask()}
              placeholder="New reminder… try “call mom tomorrow 3pm #family”"
              className="flex-1 field px-3 py-2 text-[13.5px]"
            />
            <button
              onClick={generateDescription}
              disabled={!newTitle.trim() || generating}
              title="Draft notes with AI — it can pull context from your calendar, email, and bank transactions"
              className="field px-3 text-[13px] flex items-center gap-1.5 disabled:opacity-40"
              style={{ color: "var(--accent)" }}
            >
              <Sparkles size={14} className={generating ? "animate-pulse" : ""} />
              {generating ? "Thinking…" : "Generate"}
            </button>
            <button onClick={addTask} className="px-4 text-[13px] font-medium field flex items-center gap-1.5" style={{ color: "var(--accent)" }}>
              <Plus size={14} /> Add
            </button>
          </div>
          <textarea
            value={newNotes}
            onChange={(e) => setNewNotes(e.target.value)}
            placeholder="Notes (optional) — write your own, or generate with AI"
            rows={2}
            className="field px-3 py-2 text-[12.5px]"
          />
          <div className="flex items-center justify-between flex-wrap gap-3">
            <DueDatePicker value={newDueAt} onChange={setNewDueAt} />
            <div className="flex items-center gap-3">
              <div className="flex items-center gap-2">
                <span className="eyebrow">Priority</span>
                <PrioritySelect value={newPriority} onChange={setNewPriority} />
              </div>
              <RecurrenceSelect value={newRecurrence} onChange={setNewRecurrence} />
            </div>
          </div>
          <TagInput value={newTags} onChange={setNewTags} suggestions={allTags} />
          {generateError && (
            <div className="text-[12px]" style={{ color: "var(--critical)" }}>
              {generateError}
            </div>
          )}
        </div>

        <ul className="ledger overflow-hidden hairline-rows">
          {tasks.map((t) =>
            editingId === t.id && editDraft ? (
              <li key={t.id} className="px-4 py-4 flex flex-col gap-2.5" style={{ background: "var(--surface-2)" }}>
                <input
                  value={editDraft.title}
                  onChange={(e) => setEditDraft({ ...editDraft, title: e.target.value })}
                  className="field px-3 py-2 text-[13.5px]"
                  autoFocus
                />
                <textarea
                  value={editDraft.notes}
                  onChange={(e) => setEditDraft({ ...editDraft, notes: e.target.value })}
                  placeholder="Notes"
                  rows={2}
                  className="field px-3 py-2 text-[12.5px]"
                />
                <div className="flex items-center justify-between flex-wrap gap-3">
                  <DueDatePicker value={editDraft.due_at} onChange={(v) => setEditDraft({ ...editDraft, due_at: v })} />
                  <div className="flex items-center gap-3">
                    <div className="flex items-center gap-2">
                      <span className="eyebrow">Priority</span>
                      <PrioritySelect value={editDraft.priority} onChange={(v) => setEditDraft({ ...editDraft, priority: v })} />
                    </div>
                    <RecurrenceSelect value={editDraft.recurrence} onChange={(v) => setEditDraft({ ...editDraft, recurrence: v })} />
                  </div>
                </div>
                <TagInput value={editDraft.tags} onChange={(tags) => setEditDraft({ ...editDraft, tags })} suggestions={allTags} />
                <label className="flex items-center gap-2 text-[12px]" style={{ color: "var(--text-muted)" }}>
                  Project
                  <select
                    value={editDraft.project}
                    onChange={(e) => setEditDraft({ ...editDraft, project: e.target.value })}
                    className="field px-2.5 py-1.5 text-[12.5px]"
                  >
                    {projects.map((p) => (
                      <option key={p.id} value={p.id}>
                        {p.name}
                      </option>
                    ))}
                  </select>
                </label>
                <div className="flex items-center gap-2 pt-1">
                  <button
                    onClick={saveEdit}
                    disabled={saving || !editDraft.title.trim()}
                    className="px-4 py-2 text-[13px] font-medium text-white disabled:opacity-40"
                    style={{ background: "var(--accent)" }}
                  >
                    {saving ? "Saving…" : "Save"}
                  </button>
                  <button onClick={cancelEdit} className="px-4 py-2 text-[13px]" style={{ color: "var(--text-muted)" }}>
                    Cancel
                  </button>
                </div>
              </li>
            ) : (
              <li
                key={t.id}
                className="flex flex-col"
                onDragOver={(e) => e.preventDefault()}
                onDrop={() => draggedTaskId && reorderTasks(draggedTaskId, t.id)}
              >
                <div className="px-4 py-3 flex items-center gap-3 group">
                  <span
                    draggable
                    onDragStart={() => setDraggedTaskId(t.id)}
                    onDragEnd={() => setDraggedTaskId(null)}
                    className="shrink-0 cursor-grab opacity-0 group-hover:opacity-100"
                    aria-label="Drag to reorder"
                  >
                    <GripVertical size={14} color="var(--text-muted)" />
                  </span>
                  <button
                    onClick={() => toggleSubtasks(t)}
                    className="shrink-0"
                    aria-label={openSubtasks[t.id] ? "Collapse subtasks" : "Expand subtasks"}
                  >
                    {openSubtasks[t.id] ? (
                      <ChevronDown size={14} color="var(--text-muted)" />
                    ) : (
                      <ChevronRight size={14} color="var(--text-muted)" />
                    )}
                  </button>
                  <RingCheck checked={t.completed} onClick={() => toggleComplete(t)} />
                  <div className="flex-1 min-w-0 cursor-pointer" onClick={() => startEdit(t)}>
                    <div
                      className="text-[13.5px] truncate flex items-center gap-1.5"
                      style={{ textDecoration: t.completed ? "line-through" : "none", color: t.completed ? "var(--text-muted)" : "var(--text-primary)" }}
                    >
                      {t.priority > 0 && (
                        <span className="w-1.5 h-1.5 rounded-full shrink-0" style={{ background: PRIORITIES[t.priority].color }} />
                      )}
                      {t.title}
                      {t.created_by_ai && (
                        <span className="eyebrow" style={{ color: "var(--accent)" }}>
                          ai
                        </span>
                      )}
                      {t.subtask_count > 0 && (
                        <span className="eyebrow shrink-0">
                          {t.subtask_count} subtask{t.subtask_count > 1 ? "s" : ""}
                        </span>
                      )}
                      {t.recurrence !== "none" && (
                        <span className="eyebrow shrink-0">↻ {t.recurrence}</span>
                      )}
                    </div>
                    {t.notes && (
                      <div className="text-[12px] truncate mt-0.5" style={{ color: "var(--text-secondary)" }}>
                        {t.notes}
                      </div>
                    )}
                    <div className="flex items-center gap-2 mt-1 flex-wrap">
                      {t.due_at && (
                        <span className="text-[11.5px] font-mono" style={{ color: "var(--text-muted)" }}>
                          {new Date(t.due_at).toLocaleString()}
                        </span>
                      )}
                      {t.tags.map((tag) => (
                        <span key={tag} className="px-1.5 py-0.5 text-[10.5px]" style={{ background: "var(--surface-2)", color: "var(--text-muted)" }}>
                          #{tag}
                        </span>
                      ))}
                    </div>
                  </div>
                  <button onClick={() => startEdit(t)} className="shrink-0" aria-label="Edit">
                    <Pencil size={14} color="var(--text-muted)" />
                  </button>
                  <button onClick={() => toggleFlag(t)} className="shrink-0" aria-label={t.flagged ? "Unflag" : "Flag"}>
                    <Flag size={14} fill={t.flagged ? "var(--warning)" : "none"} color={t.flagged ? "var(--warning)" : "var(--text-muted)"} />
                  </button>
                  <button onClick={() => remove(t)} className="shrink-0" aria-label="Delete">
                    <Trash2 size={14} color="var(--text-muted)" />
                  </button>
                </div>

                {openSubtasks[t.id] && (
                  <div className="pl-11 pr-4 pb-3 flex flex-col gap-2" style={{ background: "var(--surface-2)" }}>
                    {taskContext[t.id] &&
                      (taskContext[t.id].events.length > 0 || taskContext[t.id].emails.length > 0) && (
                        <div className="flex flex-col gap-1 pt-2 pb-1">
                          <div className="eyebrow flex items-center gap-1" style={{ color: "var(--text-muted)" }}>
                            <Link2 size={11} /> Related
                          </div>
                          {taskContext[t.id].events.map((e, i) => (
                            <div key={`e${i}`} className="text-[12px] truncate" style={{ color: "var(--text-secondary)" }}>
                              📅 {e.summary}
                            </div>
                          ))}
                          {taskContext[t.id].emails.map((e, i) => (
                            <div key={`m${i}`} className="text-[12px] truncate" style={{ color: "var(--text-secondary)" }}>
                              ✉️ {e.subject}
                            </div>
                          ))}
                        </div>
                      )}
                    {openSubtasks[t.id].map((sub) => (
                      <div key={sub.id} className="flex items-center gap-2.5">
                        <RingCheck checked={sub.completed} onClick={() => toggleSubtaskComplete(sub, t.id)} />
                        <span
                          className="flex-1 text-[12.5px] truncate"
                          style={{ textDecoration: sub.completed ? "line-through" : "none", color: sub.completed ? "var(--text-muted)" : "var(--text-primary)" }}
                        >
                          {sub.title}
                        </span>
                        <button onClick={() => deleteSubtask(sub, t.id)} aria-label="Delete subtask" className="shrink-0">
                          <Trash2 size={12} color="var(--text-muted)" />
                        </button>
                      </div>
                    ))}
                    <div className="flex items-center gap-2 pt-0.5">
                      <input
                        value={newSubtaskTitle[t.id] || ""}
                        onChange={(e) => setNewSubtaskTitle((cur) => ({ ...cur, [t.id]: e.target.value }))}
                        onKeyDown={(e) => e.key === "Enter" && addSubtask(t)}
                        placeholder="Add subtask…"
                        className="flex-1 field px-2 py-1 text-[12px]"
                      />
                      <button onClick={() => addSubtask(t)} className="text-[12px] font-medium" style={{ color: "var(--accent)" }}>
                        Add
                      </button>
                    </div>
                  </div>
                )}
              </li>
            )
          )}
          {tasks.length === 0 && (
            <li className="text-[13px] py-10 text-center" style={{ color: "var(--text-muted)" }}>
              Nothing here.
            </li>
          )}
        </ul>
      </div>
      <Toast toast={toast} onDismiss={() => setToast(null)} />
    </div>
  );
}
