"use client";

import { useRef, useState } from "react";
import { Upload, X } from "lucide-react";
import { cn } from "@/lib/utils";

interface FileUploadProps {
  onFileSelect: (file: File | null) => void;
  accept?: string;
  maxSize?: number;
}

export function FileUpload({ onFileSelect, accept, maxSize = 100 }: FileUploadProps) {
  const [file, setFile] = useState<File | null>(null);
  const [dragOver, setDragOver] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);

  const handleFile = (selectedFile: File | null) => {
    if (!selectedFile) return;

    if (maxSize && selectedFile.size > maxSize * 1024 * 1024) {
      alert(`文件大小不能超过 ${maxSize}MB`);
      return;
    }

    setFile(selectedFile);
    onFileSelect(selectedFile);
  };

  const handleClear = () => {
    setFile(null);
    onFileSelect(null);
    if (inputRef.current) inputRef.current.value = "";
  };

  return (
    <div className="space-y-2">
      <div
        className={cn(
          "border-2 border-dashed rounded-lg p-6 text-center cursor-pointer transition-colors",
          dragOver ? "border-indigo-500 bg-indigo-50" : "border-slate-300 hover:border-indigo-400",
          file && "bg-slate-50"
        )}
        onDragOver={(e) => { e.preventDefault(); setDragOver(true); }}
        onDragLeave={() => setDragOver(false)}
        onDrop={(e) => {
          e.preventDefault();
          setDragOver(false);
          handleFile(e.dataTransfer.files[0]);
        }}
        onClick={() => !file && inputRef.current?.click()}
      >
        <input
          ref={inputRef}
          type="file"
          accept={accept}
          className="hidden"
          onChange={(e) => handleFile(e.target.files?.[0] || null)}
        />

        {file ? (
          <div className="flex items-center justify-between">
            <div className="flex items-center gap-2">
              <Upload className="w-5 h-5 text-indigo-600" />
              <div className="text-left">
                <div className="font-medium text-slate-700">{file.name}</div>
                <div className="text-sm text-slate-500">{(file.size / 1024 / 1024).toFixed(2)} MB</div>
              </div>
            </div>
            <button
              onClick={(e) => { e.stopPropagation(); handleClear(); }}
              className="p-1 hover:bg-slate-200 rounded"
            >
              <X className="w-4 h-4" />
            </button>
          </div>
        ) : (
          <div>
            <Upload className="w-8 h-8 mx-auto mb-2 text-slate-400" />
            <p className="text-slate-600">点击或拖拽文件到此处</p>
            <p className="text-sm text-slate-400 mt-1">最大 {maxSize}MB</p>
          </div>
        )}
      </div>
    </div>
  );
}
