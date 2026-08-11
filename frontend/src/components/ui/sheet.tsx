"use client";

import * as React from "react";
import { Dialog as BaseDialog } from "@base-ui/react/dialog";
import { X } from "lucide-react";
import { cn } from "@/lib/utils";

export const Sheet = BaseDialog.Root;
export const SheetTrigger = BaseDialog.Trigger;
export const SheetClose = BaseDialog.Close;

export function SheetContent({
  className,
  children,
  ...props
}: React.ComponentProps<typeof BaseDialog.Popup>) {
  return (
    <BaseDialog.Portal>
      <BaseDialog.Backdrop className="fixed inset-0 z-50 bg-black/50 data-[ending-style]:opacity-0 data-[starting-style]:opacity-0 transition-opacity" />
      <BaseDialog.Popup
        className={cn(
          "fixed right-0 top-0 z-50 h-dvh w-full max-w-md overflow-y-auto border-l border-border bg-surface p-6 shadow-2xl outline-none data-[ending-style]:translate-x-full data-[starting-style]:translate-x-full transition-transform",
          className,
        )}
        {...props}
      >
        {children}
        <BaseDialog.Close className="absolute right-4 top-4 rounded-md p-1 text-muted hover:bg-surface-2 hover:text-ink">
          <X className="h-4 w-4" />
        </BaseDialog.Close>
      </BaseDialog.Popup>
    </BaseDialog.Portal>
  );
}

export function SheetHeader({ className, ...props }: React.HTMLAttributes<HTMLDivElement>) {
  return <div className={cn("mb-4 flex flex-col gap-1", className)} {...props} />;
}

export function SheetTitle({ className, ...props }: React.ComponentProps<typeof BaseDialog.Title>) {
  return <BaseDialog.Title className={cn("text-lg font-semibold text-ink", className)} {...props} />;
}
