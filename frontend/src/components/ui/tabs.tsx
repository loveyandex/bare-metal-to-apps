"use client";

import * as React from "react";
import { Tabs as BaseTabs } from "@base-ui/react/tabs";
import { cn } from "@/lib/utils";

export const Tabs = BaseTabs.Root;

export function TabsList({ className, ...props }: React.ComponentProps<typeof BaseTabs.List>) {
  return (
    <BaseTabs.List
      className={cn("inline-flex items-center gap-1 rounded-md border border-border bg-canvas p-1", className)}
      {...props}
    />
  );
}

export function TabsTab({ className, ...props }: React.ComponentProps<typeof BaseTabs.Tab>) {
  return (
    <BaseTabs.Tab
      className={cn(
        "rounded-sm px-3 py-1.5 text-sm text-muted transition-colors data-[selected]:bg-surface-2 data-[selected]:text-ink",
        className,
      )}
      {...props}
    />
  );
}

export function TabsPanel({ className, ...props }: React.ComponentProps<typeof BaseTabs.Panel>) {
  return <BaseTabs.Panel className={cn("mt-4", className)} {...props} />;
}
