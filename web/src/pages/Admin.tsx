import { useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import { useNavigate } from 'react-router-dom';
import { api, type AuditEntry, type RetentionStatus } from '../lib/api';

const EVENT_TYPES = [
  '', // all
  'agent.message.received',
  'agent.message.sent',
  'agent.model.response',
  'agent.tool.called',
  'agent.tool.result',
  'agent.memory.stored',
  'user.login',
  'user.register',
  'retention.sweep',
];

function formatTimestamp(ts: string) {
  const d = new Date(ts);
  return d.toLocaleString(undefined, {
    month: 'short',
    day: 'numeric',
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
  });
}

function EventBadge({ type }: { type: string }) {
  const colors: Record<string, string> = {
    'agent.message.received': 'bg-blue-900/50 text-blue-300',
    'agent.message.sent': 'bg-green-900/50 text-green-300',
    'agent.model.response': 'bg-purple-900/50 text-purple-300',
    'agent.tool.called': 'bg-yellow-900/50 text-yellow-300',
    'agent.tool.result': 'bg-yellow-900/30 text-yellow-400',
    'agent.memory.stored': 'bg-cyan-900/50 text-cyan-300',
    'user.login': 'bg-zinc-800 text-zinc-300',
    'user.register': 'bg-zinc-800 text-zinc-300',
    'retention.sweep': 'bg-red-900/50 text-red-300',
  };
  const cls = colors[type] || 'bg-zinc-800 text-zinc-400';
  const short = type.split('.').slice(-2).join('.');
  return (
    <span className={`inline-block px-2 py-0.5 rounded text-xs font-mono ${cls}`}>
      {short}
    </span>
  );
}

function DetailCell({ entry }: { entry: AuditEntry }) {
  const d = entry.details;
  const parts: string[] = [];

  if (d.tool) parts.push(`tool: ${d.tool}`);
  if (d.content_length) parts.push(`${d.content_length} chars`);
  if (d.iterations) parts.push(`${d.iterations} iterations`);
  if (d.stop_reason) parts.push(`stop: ${d.stop_reason}`);
  if (d.is_error) parts.push('ERROR');
  if (d.output_summary) parts.push(String(d.output_summary).slice(0, 60));
  if (d.scope) parts.push(`scope: ${d.scope}`);
  if (d.category) parts.push(`cat: ${d.category}`);

  if (parts.length === 0 && Object.keys(d).length > 0) {
    parts.push(JSON.stringify(d).slice(0, 80));
  }

  return (
    <span className="text-zinc-500 text-xs font-mono truncate block max-w-md">
      {parts.join(' | ') || '-'}
    </span>
  );
}

function TokenCost({ entry }: { entry: AuditEntry }) {
  if (!entry.tokens_input && !entry.tokens_output) return <span className="text-zinc-600">-</span>;
  const cost = entry.cost_usd ? `$${Number(entry.cost_usd).toFixed(4)}` : '';
  return (
    <span className="text-xs text-zinc-500 font-mono">
      {entry.tokens_input ?? 0}/{entry.tokens_output ?? 0}
      {cost && <span className="text-zinc-600 ml-1">{cost}</span>}
    </span>
  );
}

export default function Admin() {
  const navigate = useNavigate();
  const [activeTab, setActiveTab] = useState<'audit' | 'retention'>('audit');
  const [eventFilter, setEventFilter] = useState('');
  const [page, setPage] = useState(0);
  const pageSize = 50;

  const { data: entries, isLoading } = useQuery({
    queryKey: ['audit', eventFilter, page],
    queryFn: () =>
      api.searchAudit({
        event_type: eventFilter || undefined,
        limit: pageSize,
        offset: page * pageSize,
      }),
    refetchInterval: 15000,
  });

  const { data: retention } = useQuery({
    queryKey: ['retention-status'],
    queryFn: () => api.getRetentionStatus(),
    refetchInterval: 30000,
  });

  // Aggregate stats from current page
  const stats = (entries ?? []).reduce(
    (acc, e) => {
      acc.total++;
      if (e.tokens_input) acc.tokens_in += e.tokens_input;
      if (e.tokens_output) acc.tokens_out += e.tokens_output;
      if (e.cost_usd) acc.cost += Number(e.cost_usd);
      return acc;
    },
    { total: 0, tokens_in: 0, tokens_out: 0, cost: 0 },
  );

  return (
    <div className="min-h-screen bg-zinc-950 text-white">
      <header className="border-b border-zinc-800 px-6 py-4 flex items-center justify-between">
        <div className="flex items-center gap-4">
          <button
            onClick={() => navigate('/')}
            className="text-zinc-400 hover:text-white transition-colors text-sm"
          >
            &larr; Back to chat
          </button>
          <h1 className="text-lg font-medium">Admin</h1>
        </div>
      </header>

      <div className="max-w-6xl mx-auto py-6 px-4">
        {/* Tabs */}
        <div className="flex gap-1 mb-6 border-b border-zinc-800">
          {(['audit', 'retention'] as const).map((tab) => (
            <button
              key={tab}
              onClick={() => setActiveTab(tab)}
              className={`px-4 py-2 text-sm capitalize transition-colors border-b-2 -mb-px ${
                activeTab === tab
                  ? 'border-white text-white'
                  : 'border-transparent text-zinc-500 hover:text-zinc-300'
              }`}
            >
              {tab === 'audit' ? 'Audit Log' : 'Retention'}
            </button>
          ))}
        </div>

        {activeTab === 'audit' && (
          <div>
            {/* Stats bar */}
            <div className="flex gap-6 mb-4 text-xs text-zinc-500">
              <span>Showing: <span className="text-zinc-300">{stats.total}</span></span>
              <span>Tokens in/out: <span className="text-zinc-300">{stats.tokens_in.toLocaleString()}/{stats.tokens_out.toLocaleString()}</span></span>
              <span>Cost: <span className="text-zinc-300">${stats.cost.toFixed(4)}</span></span>
            </div>

            {/* Filters */}
            <div className="flex gap-3 mb-4">
              <select
                value={eventFilter}
                onChange={(e) => { setEventFilter(e.target.value); setPage(0); }}
                className="bg-zinc-900 border border-zinc-700 rounded-lg px-3 py-1.5 text-sm text-zinc-300 focus:outline-none focus:border-zinc-500"
              >
                <option value="">All events</option>
                {EVENT_TYPES.filter(Boolean).map((t) => (
                  <option key={t} value={t}>{t}</option>
                ))}
              </select>
            </div>

            {/* Table */}
            <div className="border border-zinc-800 rounded-lg overflow-hidden">
              <table className="w-full text-sm">
                <thead>
                  <tr className="bg-zinc-900/50 text-zinc-500 text-xs uppercase tracking-wide">
                    <th className="text-left px-4 py-2 font-medium">Time</th>
                    <th className="text-left px-4 py-2 font-medium">Event</th>
                    <th className="text-left px-4 py-2 font-medium">Details</th>
                    <th className="text-right px-4 py-2 font-medium">Tokens / Cost</th>
                  </tr>
                </thead>
                <tbody>
                  {isLoading && (
                    <tr>
                      <td colSpan={4} className="text-center py-8 text-zinc-600">Loading...</td>
                    </tr>
                  )}
                  {!isLoading && (entries ?? []).length === 0 && (
                    <tr>
                      <td colSpan={4} className="text-center py-8 text-zinc-600">No audit entries found</td>
                    </tr>
                  )}
                  {(entries ?? []).map((entry) => (
                    <tr key={entry.id} className="border-t border-zinc-800/50 hover:bg-zinc-900/30">
                      <td className="px-4 py-2 text-xs text-zinc-400 whitespace-nowrap font-mono">
                        {formatTimestamp(entry.timestamp)}
                      </td>
                      <td className="px-4 py-2">
                        <EventBadge type={entry.event_type} />
                      </td>
                      <td className="px-4 py-2">
                        <DetailCell entry={entry} />
                      </td>
                      <td className="px-4 py-2 text-right">
                        <TokenCost entry={entry} />
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>

            {/* Pagination */}
            <div className="flex items-center justify-between mt-3">
              <button
                onClick={() => setPage((p) => Math.max(0, p - 1))}
                disabled={page === 0}
                className="text-xs text-zinc-500 hover:text-zinc-300 disabled:opacity-30 disabled:cursor-not-allowed"
              >
                Previous
              </button>
              <span className="text-xs text-zinc-600">Page {page + 1}</span>
              <button
                onClick={() => setPage((p) => p + 1)}
                disabled={(entries ?? []).length < pageSize}
                className="text-xs text-zinc-500 hover:text-zinc-300 disabled:opacity-30 disabled:cursor-not-allowed"
              >
                Next
              </button>
            </div>
          </div>
        )}

        {activeTab === 'retention' && (
          <div className="space-y-6">
            {!retention ? (
              <p className="text-zinc-600 text-sm">Loading retention status...</p>
            ) : (
              <>
                <div className="border border-zinc-800 rounded-lg p-6">
                  <h3 className="text-sm font-medium text-zinc-300 mb-4">Retention Policy</h3>
                  <div className="grid grid-cols-2 gap-4 text-sm">
                    <div>
                      <span className="text-zinc-500">Conversations</span>
                      <p className="text-zinc-300 mt-1">{retention.config.default_retention_days} days</p>
                    </div>
                    <div>
                      <span className="text-zinc-500">Delete mode</span>
                      <p className="text-zinc-300 mt-1">{retention.config.hard_delete ? 'Hard delete' : 'Soft delete'}</p>
                    </div>
                    <div>
                      <span className="text-zinc-500">Sweep interval</span>
                      <p className="text-zinc-300 mt-1">
                        {retention.config.interval_secs >= 86400
                          ? `${Math.round(retention.config.interval_secs / 86400)}d`
                          : retention.config.interval_secs >= 3600
                          ? `${Math.round(retention.config.interval_secs / 3600)}h`
                          : `${retention.config.interval_secs}s`}
                      </p>
                    </div>
                    <div>
                      <span className="text-zinc-500">Task status</span>
                      <p className="mt-1 flex items-center gap-2">
                        <span className={`inline-block w-2 h-2 rounded-full ${retention.running ? 'bg-green-500' : 'bg-red-500'}`} />
                        <span className="text-zinc-300 text-sm">{retention.running ? 'Running' : 'Stopped'}</span>
                      </p>
                    </div>
                  </div>
                </div>

                <div className="border border-zinc-800 rounded-lg p-6">
                  <h3 className="text-sm font-medium text-zinc-300 mb-4">Sweep History</h3>
                  <div className="grid grid-cols-2 gap-4 text-sm">
                    <div>
                      <span className="text-zinc-500">Total sweeps</span>
                      <p className="text-zinc-300 mt-1">{retention.total_sweeps}</p>
                    </div>
                    <div>
                      <span className="text-zinc-500">Total deleted</span>
                      <p className="text-zinc-300 mt-1">{retention.total_deleted} conversations</p>
                    </div>
                    <div>
                      <span className="text-zinc-500">Last sweep</span>
                      <p className="text-zinc-300 mt-1">
                        {retention.last_sweep_at
                          ? new Date(retention.last_sweep_at).toLocaleString()
                          : 'Not yet'}
                      </p>
                    </div>
                    <div>
                      <span className="text-zinc-500">Last sweep deleted</span>
                      <p className="text-zinc-300 mt-1">{retention.last_sweep_deleted} conversations</p>
                    </div>
                  </div>
                </div>
              </>
            )}
          </div>
        )}
      </div>
    </div>
  );
}
