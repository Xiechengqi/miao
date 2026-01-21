"use client";

const UNITS = ["B", "KB", "MB", "GB", "TB", "PB"];

export function formatBytes(bytes: number, precision = 1): string {
  if (!Number.isFinite(bytes)) {
    return "-";
  }
  if (bytes === 0) {
    return "0 B";
  }
  const base = 1024;
  const exponent = Math.min(
    Math.floor(Math.log(bytes) / Math.log(base)),
    UNITS.length - 1
  );
  const value = bytes / Math.pow(base, exponent);
  return `${value.toFixed(precision)} ${UNITS[exponent]}`;
}

export function formatKb(kb: number, precision = 1): string {
  return formatBytes(kb * 1024, precision);
}

export function formatPercent(value?: number): string {
  if (value === undefined || value === null || Number.isNaN(value)) {
    return "-";
  }
  return `${Math.round(value)}%`;
}
