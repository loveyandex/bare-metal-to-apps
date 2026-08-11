"use client";

import * as React from "react";
import { Dialog as BaseDialog } from "@base-ui/react/dialog";
import { X } from "lucide-react";
import { cn } from "@/lib/utils";

export const Dialog = BaseDialog.Root;
export const DialogTrigger = BaseDialog.Trigger;
export const DialogClose = BaseDialog.Close;

export function DialogContent({
  className,
  children,
  showClose = true,
  ...props
}: React.ComponentProps<typeof BaseDialog.Popup> & { showClose?: boolean }) {
  return (
    <BaseDialog.Portal>
      <BaseDialog.Backdrop className="fixed inset-0 z-50 bg-black/60 backdrop-blur-sm data-[ending-style]:opacity-0 data-[starting-style]:opacity-0 transition-opacity" />
      <BaseDialog.Popup
        className={cn(
          "fixed left-1/2 top-1/2 z-50 w-full max-w-md -translate-x-1/2 -translate-y-1/2 rounded-lg border border-border bg-surface p-6 shadow-2xl outline-none data-[ending-style]:opacity-0 data-[ending-style]:scale-95 data-[starting-style]:opacity-0 data-[starting-style]:scale-95 transition-all",
          className,
        )}
        {...props}
      >
        {children}
        {showClose && (
          <BaseDialog.Close className="absolute right-4 top-4 rounded-md p-1 text-muted hover:bg-surface-2 hover:text-ink">
            <X className="h-4 w-4" />
          </BaseDialog.Close>
        )}
      </BaseDialog.Popup>
    </BaseDialog.Portal>
  );
}

export function DialogHeader({ className, ...props }: React.HTMLAttributes<HTMLDivElement>) {
  return <div className={cn("mb-4 flex flex-col gap-1", className)} {...props} />;
}

export function DialogTitle({ className, ...props }: React.ComponentProps<typeof BaseDialog.Title>) {
  return <BaseDialog.Title className={cn("text-lg font-semibold text-ink", className)} {...props} />;
}

export function DialogDescription({
  className,
  ...props
}: React.ComponentProps<typeof BaseDialog.Description>) {
  return <BaseDialog.Description className={cn("text-sm text-muted", className)} {...props} />;
}
