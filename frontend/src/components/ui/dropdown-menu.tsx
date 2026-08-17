"use client";

import * as React from "react";
import { Menu as BaseMenu } from "@base-ui/react/menu";
import { cn } from "@/lib/utils";

export const DropdownMenu = BaseMenu.Root;
export const DropdownMenuTrigger = BaseMenu.Trigger;

export function DropdownMenuContent({
  className,
  children,
  align = "end",
  ...props
}: React.ComponentProps<typeof BaseMenu.Popup> & { align?: "start" | "end" }) {
  return (
    <BaseMenu.Portal>
      <BaseMenu.Positioner side="bottom" align={align} sideOffset={6}>
        <BaseMenu.Popup
          className={cn(
            "min-w-[12rem] rounded-md border border-border bg-surface p-1 shadow-2xl outline-none data-[ending-style]:opacity-0 data-[starting-style]:opacity-0 transition-opacity",
            className,
          )}
          {...props}
        >
          {children}
        </BaseMenu.Popup>
      </BaseMenu.Positioner>
    </BaseMenu.Portal>
  );
}

export function DropdownMenuItem({
  className,
  destructive,
  ...props
}: React.ComponentProps<typeof BaseMenu.Item> & { destructive?: boolean }) {
  return (
    <BaseMenu.Item
      className={cn(
        "flex cursor-pointer items-center gap-2 rounded-sm px-2.5 py-2 text-sm text-ink outline-none transition-colors data-[highlighted]:bg-surface-2",
        destructive && "text-danger data-[highlighted]:bg-danger/10",
        className,
      )}
      {...props}
    />
  );
}
