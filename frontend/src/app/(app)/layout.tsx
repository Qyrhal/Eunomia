import AuthGuard from "@/components/AuthGuard";
import CommandPalette from "@/components/CommandPalette";
import Sidebar from "@/components/Sidebar";

export default function AppLayout({ children }: { children: React.ReactNode }) {
  return (
    <AuthGuard>
      <Sidebar />
      <main className="flex-1 min-w-0 p-8 md:p-10">{children}</main>
      <CommandPalette />
    </AuthGuard>
  );
}
