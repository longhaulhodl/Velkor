const TOOL_LABELS: Record<string, string> = {
  web_search: 'Searching the web',
  web_fetch: 'Fetching page',
  memory_store: 'Saving to memory',
  memory_search: 'Searching memory',
  document_read: 'Reading document',
  document_search: 'Searching documents',
};

interface Props {
  tool: string;
  status: 'started' | 'completed' | 'failed';
}

export default function ToolIndicator({ tool, status }: Props) {
  const label = TOOL_LABELS[tool] ?? tool;

  return (
    <div className="flex items-center gap-2 text-xs text-zinc-500">
      {status === 'started' && (
        <span className="inline-block w-3 h-3 border-2 border-zinc-600 border-t-zinc-300 rounded-full animate-spin" />
      )}
      {status === 'completed' && (
        <span className="text-green-500">&#10003;</span>
      )}
      {status === 'failed' && (
        <span className="text-red-500">&#10007;</span>
      )}
      <span>{label}{status === 'started' ? '...' : ''}</span>
    </div>
  );
}
