"use client";

import { useEffect, useState } from "react";
import { CheckCircle, AlertCircle, Info, AlertTriangle, X, ChevronDown, ChevronUp } from "lucide-react";
import { cn } from "@/lib/utils";
import { ToastMessage, ToastProps } from "@/types/components";
import { motion, AnimatePresence } from "framer-motion";

const toastIcons = {
  success: <CheckCircle className="w-5 h-5 text-emerald-500" />,
  error: <AlertCircle className="w-5 h-5 text-red-500" />,
  info: <Info className="w-5 h-5 text-sky-500" />,
  warning: <AlertTriangle className="w-5 h-5 text-amber-500" />,
};

const toastStyles = {
  success: "border-l-emerald-500 bg-emerald-50/80",
  error: "border-l-red-500 bg-red-50/80",
  info: "border-l-sky-500 bg-sky-50/80",
  warning: "border-l-amber-500 bg-amber-50/80",
};

export function Toast({ toast, onClose }: ToastProps) {
  const [expanded, setExpanded] = useState(false);

  useEffect(() => {
    const timer = setTimeout(() => {
      onClose(toast.id);
    }, 8000);
    return () => clearTimeout(timer);
  }, [toast.id, onClose]);

  const isLongMessage = toast.message.length > 100;

  return (
    <motion.div
      initial={{ opacity: 0, x: 100 }}
      animate={{ opacity: 1, x: 0 }}
      exit={{ opacity: 0, x: 100 }}
      className={cn(
        "flex flex-col gap-2 px-4 py-3",
        "rounded-lg shadow-lg",
        "border-l-4",
        toastStyles[toast.type],
        "max-w-md"
      )}
    >
      <div className="flex items-start gap-3">
        {toastIcons[toast.type]}
        <p className={cn(
          "flex-1 text-sm font-medium text-slate-900",
          !expanded && isLongMessage && "line-clamp-2"
        )}>
          {toast.message}
        </p>
        <div className="flex items-center gap-1">
          {isLongMessage && (
            <button
              onClick={() => setExpanded(!expanded)}
              className="p-1 rounded-lg hover:bg-black/5 transition-colors"
              title={expanded ? "收起" : "展开"}
            >
              {expanded
                ? <ChevronUp className="w-4 h-4 text-slate-500" />
                : <ChevronDown className="w-4 h-4 text-slate-500" />
              }
            </button>
          )}
          <button
            onClick={() => onClose(toast.id)}
            className="p-1 rounded-lg hover:bg-black/5 transition-colors"
          >
            <X className="w-4 h-4 text-slate-500" />
          </button>
        </div>
      </div>
    </motion.div>
  );
}

interface ToastContainerProps {
  toasts: ToastMessage[];
  onClose: (id: string) => void;
}

export function ToastContainer({ toasts, onClose }: ToastContainerProps) {
  return (
    <div className="fixed top-4 right-4 z-[100] flex flex-col gap-2 max-w-sm">
      <AnimatePresence mode="popLayout">
        {toasts.map((toast) => (
          <Toast key={toast.id} toast={toast} onClose={onClose} />
        ))}
      </AnimatePresence>
    </div>
  );
}
