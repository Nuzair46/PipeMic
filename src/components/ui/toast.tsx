import * as React from "react";
import * as ToastPrimitive from "@radix-ui/react-toast";
import { AlertTriangle, CheckCircle2, Info, X } from "lucide-react";
import { cn } from "@/lib/utils";

export type ToastTone = "info" | "warn" | "fail" | "success";

export type AppToast = {
  id: number;
  title: string;
  description?: string;
  tone?: ToastTone;
};

type ToastStackProps = {
  toasts: AppToast[];
  onDismiss: (id: number) => void;
};

export const ToastProvider = ToastPrimitive.Provider;

export function ToastStack({ toasts, onDismiss }: ToastStackProps) {
  return (
    <>
      {toasts.map((toast) => {
        const tone = toast.tone ?? "info";

        return (
          <ToastPrimitive.Root
            key={toast.id}
            open
            duration={3000}
            onOpenChange={(open) => {
              if (!open) {
                onDismiss(toast.id);
              }
            }}
            className={cn(
              "relative grid w-full grid-cols-[32px_minmax(0,1fr)_28px] items-start gap-3 overflow-hidden rounded-md border bg-[#101619]/[0.98] p-3.5 pr-3 shadow-[0_18px_52px_rgba(0,0,0,0.42)] backdrop-blur-sm data-[swipe=move]:translate-x-[var(--radix-toast-swipe-move-x)] data-[swipe=end]:animate-out data-[swipe=end]:translate-x-[var(--radix-toast-swipe-end-x)]",
              tone === "fail" && "border-destructive/45",
              tone === "warn" && "border-accent/45",
              tone === "success" && "border-primary/45",
              tone === "info" && "border-border",
            )}
          >
            <ToastIcon tone={tone} />
            <div className="min-w-0 pt-0.5">
              <ToastPrimitive.Title className="text-sm font-semibold leading-5 text-foreground">{toast.title}</ToastPrimitive.Title>
              {toast.description ? (
                <ToastPrimitive.Description className="mt-1 max-h-10 overflow-hidden text-xs leading-5 text-muted-foreground">
                  {toast.description}
                </ToastPrimitive.Description>
              ) : null}
            </div>
            <ToastPrimitive.Close asChild>
              <button
                type="button"
                className="flex h-7 w-7 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-secondary hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
              >
                <X className="h-4 w-4" />
                <span className="sr-only">Dismiss</span>
              </button>
            </ToastPrimitive.Close>
          </ToastPrimitive.Root>
        );
      })}
      <ToastPrimitive.Viewport className="fixed bottom-4 right-4 z-[100] flex w-[390px] max-w-[calc(100vw-32px)] flex-col gap-2 outline-none" />
    </>
  );
}

function ToastIcon({ tone }: { tone: ToastTone }) {
  const iconClass = "h-4 w-4";

  if (tone === "fail") {
    return (
      <div className="flex h-8 w-8 items-center justify-center rounded-md bg-destructive/10 text-destructive">
        <AlertTriangle className={iconClass} />
      </div>
    );
  }
  if (tone === "warn") {
    return (
      <div className="flex h-8 w-8 items-center justify-center rounded-md bg-accent/10 text-accent">
        <AlertTriangle className={iconClass} />
      </div>
    );
  }
  if (tone === "success") {
    return (
      <div className="flex h-8 w-8 items-center justify-center rounded-md bg-primary/10 text-primary">
        <CheckCircle2 className={iconClass} />
      </div>
    );
  }
  return (
    <div className="flex h-8 w-8 items-center justify-center rounded-md bg-secondary text-muted-foreground">
      <Info className={iconClass} />
    </div>
  );
}
