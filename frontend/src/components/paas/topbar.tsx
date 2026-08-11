import Link from "next/link";
import { Boxes } from "lucide-react";

export function Topbar({ children }: { children?: React.ReactNode }) {
  return (
    <header className="flex h-14 shrink-0 items-center justify-between border-b border-border px-6">
      <Link href="/" className="flex items-center gap-2 text-sm font-semibold text-ink">
        <Boxes className="h-5 w-5 text-primary" />
        Dockyard
      </Link>
      <div className="flex items-center gap-3">{children}</div>
    </header>
  );
}
