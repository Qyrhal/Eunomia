"use client";

export type ToastState = { message: string; onUndo: () => void } | null;

export default function Toast({ toast, onDismiss }: { toast: ToastState; onDismiss: () => void }) {
  if (!toast) return null;
  return (
    <div
      className="fixed bottom-6 left-1/2 -translate-x-1/2 flex items-center gap-3 px-4 py-2.5 text-[13px] shadow-lg z-50"
      style={{ background: "var(--surface-2)", border: "1px solid var(--border-strong)", color: "var(--text-primary)" }}
    >
      {toast.message}
      <button
        onClick={() => {
          toast.onUndo();
          onDismiss();
        }}
        className="font-medium"
        style={{ color: "var(--accent)" }}
      >
        Undo
      </button>
    </div>
  );
}
