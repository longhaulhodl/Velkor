import { useState, useRef } from 'react';
import { api, type DocumentMeta } from '../lib/api';

// Default workspace for Phase 1 — single-workspace mode
const DEFAULT_WORKSPACE = '00000000-0000-0000-0000-000000000001';

interface Props {
  onClose: () => void;
}

export default function FileUpload({ onClose }: Props) {
  const [uploading, setUploading] = useState(false);
  const [uploaded, setUploaded] = useState<DocumentMeta[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [dragOver, setDragOver] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);

  const handleFiles = async (files: FileList | File[]) => {
    setUploading(true);
    setError(null);

    const fileArray = Array.from(files);
    const results: DocumentMeta[] = [];

    for (const file of fileArray) {
      try {
        const res = await api.uploadDocument(DEFAULT_WORKSPACE, file);
        results.push(res.document);
      } catch (e) {
        setError(`Failed to upload ${file.name}: ${(e as Error).message}`);
      }
    }

    setUploaded((prev) => [...prev, ...results]);
    setUploading(false);
  };

  const handleDrop = (e: React.DragEvent) => {
    e.preventDefault();
    setDragOver(false);
    if (e.dataTransfer.files.length) {
      handleFiles(e.dataTransfer.files);
    }
  };

  const handleDragOver = (e: React.DragEvent) => {
    e.preventDefault();
    setDragOver(true);
  };

  const formatSize = (bytes: number | null) => {
    if (!bytes) return '';
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  };

  return (
    <div className="border-t border-zinc-800 bg-zinc-900/50 px-4 py-3">
      <div className="max-w-3xl mx-auto">
        <div className="flex items-center justify-between mb-2">
          <span className="text-xs text-zinc-400 font-medium">Upload Documents</span>
          <button
            onClick={onClose}
            className="text-zinc-500 hover:text-zinc-300 text-xs transition-colors"
          >
            Close
          </button>
        </div>

        {/* Drop zone */}
        <div
          onDrop={handleDrop}
          onDragOver={handleDragOver}
          onDragLeave={() => setDragOver(false)}
          onClick={() => inputRef.current?.click()}
          className={`border-2 border-dashed rounded-lg p-4 text-center cursor-pointer transition-colors ${
            dragOver
              ? 'border-zinc-400 bg-zinc-800/50'
              : 'border-zinc-700 hover:border-zinc-600'
          }`}
        >
          <input
            ref={inputRef}
            type="file"
            multiple
            accept=".txt,.md,.pdf,.docx"
            onChange={(e) => e.target.files && handleFiles(e.target.files)}
            className="hidden"
          />
          {uploading ? (
            <div className="flex items-center justify-center gap-2 text-sm text-zinc-400">
              <span className="inline-block w-4 h-4 border-2 border-zinc-600 border-t-zinc-300 rounded-full animate-spin" />
              Uploading...
            </div>
          ) : (
            <p className="text-sm text-zinc-500">
              Drop files here or <span className="text-zinc-300 underline">browse</span>
              <br />
              <span className="text-xs text-zinc-600 mt-1 inline-block">
                Supports TXT, Markdown, PDF, DOCX
              </span>
            </p>
          )}
        </div>

        {/* Error */}
        {error && (
          <p className="text-xs text-red-400 mt-2">{error}</p>
        )}

        {/* Uploaded files list */}
        {uploaded.length > 0 && (
          <div className="mt-2 space-y-1">
            {uploaded.map((doc) => (
              <div
                key={doc.id}
                className="flex items-center justify-between text-xs bg-zinc-800 rounded-lg px-3 py-2"
              >
                <div className="flex items-center gap-2 text-zinc-300 min-w-0">
                  <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" className="shrink-0">
                    <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" />
                    <polyline points="14 2 14 8 20 8" />
                  </svg>
                  <span className="truncate">{doc.filename}</span>
                </div>
                <span className="text-zinc-500 shrink-0 ml-2">
                  {formatSize(doc.file_size)}
                </span>
              </div>
            ))}
            <p className="text-xs text-zinc-600 mt-1">
              The agent can now read these documents using the document_read tool.
            </p>
          </div>
        )}
      </div>
    </div>
  );
}
