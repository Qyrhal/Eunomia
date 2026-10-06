import AuthGuard from "@/components/AuthGuard";
import CommandPalette from "@/components/CommandPalette";
import Sidebar from "@/components/Sidebar";

export default function AppLayout({ children }: { children: React.ReactNode }) {
  return (
    <AuthGuard>
      <div className="flex flex-col md:flex-row flex-1 min-w-0">
        <Sidebar />
        <main className="flex-1 min-w-0 p-6 md:p-10">{children}</main>
      </div>
      <CommandPalette />
    </AuthGuard>
  );
}
