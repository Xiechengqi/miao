"use client";

import { forwardRef, ButtonHTMLAttributes } from "react";
import { cn } from "@/lib/utils";
import { Loader2 } from "lucide-react";
import { ButtonVariant, ButtonSize } from "@/types/components";

interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: ButtonVariant;
  size?: ButtonSize;
  loading?: boolean;
}

const variantStyles: Record<ButtonVariant, string> = {
  primary: "bg-gradient-to-r from-indigo-600 to-violet-600 text-white shadow-[0_4px_14px_0_rgba(79,70,229,0.3)]",
  secondary: "bg-white text-slate-700 border border-slate-200 hover:bg-slate-50 hover:border-slate-300",
  ghost: "text-slate-700 hover:bg-indigo-50 hover:text-indigo-700",
  danger: "bg-gradient-to-r from-red-500 to-red-600 text-white shadow-[0_4px_14px_0_rgba(239,68,68,0.3)]",
};

const sizeStyles: Record<ButtonSize, string> = {
  sm: "h-10 px-4 text-sm rounded-lg",
  md: "h-11 px-6 text-base rounded-lg",
  lg: "h-12 px-8 text-lg rounded-lg",
};

export const Button = forwardRef<HTMLButtonElement, ButtonProps>(
  ({ className, variant = "primary", size = "md", loading, children, disabled, ...props }, ref) => {
    return (
      <button
        ref={ref}
        className={cn(
          "inline-flex items-center justify-center font-semibold",
          "transition-all duration-200 ease-out",
          "hover:-translate-y-0.5",
          variant === "primary" && "hover:shadow-[0_6px_20px_0_rgba(79,70,229,0.4)]",
          variant === "danger" && "hover:shadow-[0_6px_20px_0_rgba(239,68,68,0.4)]",
          "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-indigo-500 focus-visible:ring-offset-2",
          "disabled:opacity-50 disabled:cursor-not-allowed disabled:hover:translate-y-0 disabled:hover:shadow-none",
          variantStyles[variant],
          sizeStyles[size],
          className
        )}
        disabled={disabled || loading}
        {...props}
      >
        {loading && <Loader2 className="mr-2 h-4 w-4 animate-spin" />}
        {children}
      </button>
    );
  }
);

Button.displayName = "Button";
