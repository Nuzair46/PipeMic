import * as React from "react";
import { cn } from "@/lib/utils";

type BadgeTone = "neutral" | "run" | "warn" | "fail";

const toneClass: Record<BadgeTone, string> = {
  neutral: "border-border bg-secondary text-muted-foreground",
  run: "border-primary/50 bg-primary/10 text-primary",
  warn: "border-accent/60 bg-accent/10 text-accent",
  fail: "border-destructive/60 bg-destructive/10 text-destructive",
};

export function Badge({
  className,
  tone = "neutral",
  ...props
}: React.HTMLAttributes<HTMLSpanElement> & { tone?: BadgeTone }) {
  return (
    <span
      className={cn(
        "inline-flex h-6 shrink-0 items-center rounded-sm border px-2 text-xs font-semibold uppercase",
        toneClass[tone],
        className,
      )}
      {...props}
    />
  );
}

