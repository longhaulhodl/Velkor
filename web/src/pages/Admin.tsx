import { useState } from 'react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { useNavigate } from 'react-router-dom';
import { api, type AuditEntry, type RetentionStatus, type SkillSummary, type LearnedSkill, type SkillDetail } from '../lib/api';

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

function SourceBadge({ source }: { source: string }) {
  const cls = source === 'installed'
    ? 'bg-blue-900/50 text-blue-300'
    : 'bg-emerald-900/50 text-emerald-300';
  return (
    <span className={`inline-block px-2 py-0.5 rounded text-xs font-mono ${cls}`}>
      {source}
    </span>
  );
}

function SkillsTab() {
  const queryClient = useQueryClient();
  const [viewingSkill, setViewingSkill] = useState<string | null>(null);
  const [showCreate, setShowCreate] = useState(false);
  const [createForm, setCreateForm] = useState({
    name: '', description: '', content: '', category: '', type: 'learned' as 'learned' | 'installed',
  });
  const [error, setError] = useState('');

  const { data: allSkills, isLoading } = useQuery({
    queryKey: ['skills'],
    queryFn: () => api.listSkills(),
  });

  const { data: learnedSkills } = useQuery({
    queryKey: ['skills-learned'],
    queryFn: () => api.listLearnedSkills(),
  });

  const { data: skillDetail } = useQuery({
    queryKey: ['skill-detail', viewingSkill],
    queryFn: () => (viewingSkill ? api.viewSkill(viewingSkill) : Promise.resolve(null)),
    enabled: !!viewingSkill,
  });

  const handleCreate = async () => {
    setError('');
    try {
      if (createForm.type === 'learned') {
        await api.createLearnedSkill({
          name: createForm.name,
          description: createForm.description || undefined,
          content: createForm.content,
          category: createForm.category || undefined,
        });
      } else {
        await api.createInstallableSkill({
          name: createForm.name,
          description: createForm.description,
          content: createForm.content,
        });
      }
      setShowCreate(false);
      setCreateForm({ name: '', description: '', content: '', category: '', type: 'learned' });
      queryClient.invalidateQueries({ queryKey: ['skills'] });
      queryClient.invalidateQueries({ queryKey: ['skills-learned'] });
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : 'Failed to create skill');
    }
  };

  const handleDeactivate = async (name: string) => {
    if (!confirm(`Deactivate skill "${name}"?`)) return;
    try {
      await api.deactivateLearnedSkill(name);
      queryClient.invalidateQueries({ queryKey: ['skills'] });
      queryClient.invalidateQueries({ queryKey: ['skills-learned'] });
      setViewingSkill(null);
    } catch (e: unknown) {
      alert(e instanceof Error ? e.message : 'Failed');
    }
  };

  const handleDeleteInstallable = async (name: string) => {
    if (!confirm(`Delete installable skill "${name}" from disk?`)) return;
    try {
      await api.deleteInstallableSkill(name);
      queryClient.invalidateQueries({ queryKey: ['skills'] });
    } catch (e: unknown) {
      alert(e instanceof Error ? e.message : 'Failed');
    }
  };

  const handleReload = async () => {
    const result = await api.reloadInstallableSkills();
    queryClient.invalidateQueries({ queryKey: ['skills'] });
    alert(`Reloaded ${result.reloaded} installable skills from disk.`);
  };

  // Skill detail view
  if (viewingSkill && skillDetail) {
    return (
      <div className="space-y-4">
        <button
          onClick={() => setViewingSkill(null)}
          className="text-zinc-400 hover:text-white text-sm"
        >
          &larr; Back to skills list
        </button>

        <div className="border border-zinc-800 rounded-lg p-6">
          <div className="flex items-center justify-between mb-4">
            <div className="flex items-center gap-3">
              <h3 className="text-lg font-medium">{skillDetail.name}</h3>
              <SourceBadge source={skillDetail.source} />
              {skillDetail.version && (
                <span className="text-xs text-zinc-500">v{skillDetail.version}</span>
              )}
            </div>
            <div className="flex gap-2">
              {skillDetail.source === 'learned' && (
                <button
                  onClick={() => handleDeactivate(skillDetail.name)}
                  className="text-xs text-red-400 hover:text-red-300 border border-red-900 rounded px-3 py-1"
                >
                  Deactivate
                </button>
              )}
              {skillDetail.source === 'installed' && (
                <button
                  onClick={() => handleDeleteInstallable(skillDetail.name)}
                  className="text-xs text-red-400 hover:text-red-300 border border-red-900 rounded px-3 py-1"
                >
                  Delete
                </button>
              )}
            </div>
          </div>

          {skillDetail.description && (
            <p className="text-sm text-zinc-400 mb-4">{skillDetail.description}</p>
          )}

          {skillDetail.source === 'learned' && (
            <div className="grid grid-cols-4 gap-4 mb-4 text-xs text-zinc-500">
              <div>
                <span className="block text-zinc-600">Usage</span>
                {skillDetail.usage_count ?? 0} calls
              </div>
              <div>
                <span className="block text-zinc-600">Success rate</span>
                {((skillDetail.success_rate ?? 1) * 100).toFixed(0)}%
              </div>
              <div>
                <span className="block text-zinc-600">Author</span>
                {skillDetail.author ?? '-'}
              </div>
              <div>
                <span className="block text-zinc-600">Category</span>
                {skillDetail.category ?? '-'}
              </div>
            </div>
          )}

          <div className="bg-zinc-900 rounded-lg p-4 overflow-auto max-h-96">
            <pre className="text-sm text-zinc-300 whitespace-pre-wrap font-mono">{skillDetail.content}</pre>
          </div>

          {skillDetail.source_path && (
            <p className="text-xs text-zinc-600 mt-2">Source: {skillDetail.source_path}</p>
          )}
        </div>
      </div>
    );
  }

  // Create form
  if (showCreate) {
    return (
      <div className="space-y-4">
        <button
          onClick={() => setShowCreate(false)}
          className="text-zinc-400 hover:text-white text-sm"
        >
          &larr; Back to skills list
        </button>

        <div className="border border-zinc-800 rounded-lg p-6 space-y-4">
          <h3 className="text-sm font-medium text-zinc-300">Create New Skill</h3>

          {error && (
            <div className="bg-red-900/30 border border-red-800 rounded-lg px-4 py-2 text-sm text-red-300">
              {error}
            </div>
          )}

          <div className="flex gap-3">
            <label className="flex items-center gap-2 text-sm text-zinc-400">
              <input
                type="radio"
                checked={createForm.type === 'learned'}
                onChange={() => setCreateForm((f) => ({ ...f, type: 'learned' }))}
                className="accent-blue-500"
              />
              Learned (DB)
            </label>
            <label className="flex items-center gap-2 text-sm text-zinc-400">
              <input
                type="radio"
                checked={createForm.type === 'installed'}
                onChange={() => setCreateForm((f) => ({ ...f, type: 'installed' }))}
                className="accent-blue-500"
              />
              Installable (SKILL.md)
            </label>
          </div>

          <input
            type="text"
            placeholder="Skill name (lowercase, hyphens)"
            value={createForm.name}
            onChange={(e) => setCreateForm((f) => ({ ...f, name: e.target.value }))}
            className="w-full bg-zinc-900 border border-zinc-700 rounded-lg px-3 py-2 text-sm text-zinc-300 focus:outline-none focus:border-zinc-500"
          />
          <input
            type="text"
            placeholder="Description"
            value={createForm.description}
            onChange={(e) => setCreateForm((f) => ({ ...f, description: e.target.value }))}
            className="w-full bg-zinc-900 border border-zinc-700 rounded-lg px-3 py-2 text-sm text-zinc-300 focus:outline-none focus:border-zinc-500"
          />
          {createForm.type === 'learned' && (
            <input
              type="text"
              placeholder="Category (optional)"
              value={createForm.category}
              onChange={(e) => setCreateForm((f) => ({ ...f, category: e.target.value }))}
              className="w-full bg-zinc-900 border border-zinc-700 rounded-lg px-3 py-2 text-sm text-zinc-300 focus:outline-none focus:border-zinc-500"
            />
          )}
          <textarea
            placeholder="Skill content (instructions, procedures, etc.)"
            value={createForm.content}
            onChange={(e) => setCreateForm((f) => ({ ...f, content: e.target.value }))}
            rows={12}
            className="w-full bg-zinc-900 border border-zinc-700 rounded-lg px-3 py-2 text-sm text-zinc-300 focus:outline-none focus:border-zinc-500 font-mono"
          />

          <div className="flex gap-3">
            <button
              onClick={handleCreate}
              disabled={!createForm.name || !createForm.content}
              className="px-4 py-2 bg-blue-600 hover:bg-blue-500 disabled:opacity-40 disabled:cursor-not-allowed rounded-lg text-sm transition-colors"
            >
              Create Skill
            </button>
            <button
              onClick={() => setShowCreate(false)}
              className="px-4 py-2 text-zinc-400 hover:text-zinc-300 text-sm"
            >
              Cancel
            </button>
          </div>
        </div>
      </div>
    );
  }

  // Skills list
  const skills = allSkills?.skills ?? [];
  const learned = learnedSkills?.skills ?? [];
  const learnedMap = new Map(learned.map((s) => [s.name, s]));

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <div className="text-xs text-zinc-500">
          {skills.length} skills ({skills.filter((s) => s.source === 'installed').length} installed, {skills.filter((s) => s.source === 'learned').length} learned)
        </div>
        <div className="flex gap-2">
          <button
            onClick={handleReload}
            className="text-xs text-zinc-400 hover:text-zinc-300 border border-zinc-700 rounded px-3 py-1.5"
          >
            Reload from disk
          </button>
          <button
            onClick={() => setShowCreate(true)}
            className="text-xs text-white bg-blue-600 hover:bg-blue-500 rounded px-3 py-1.5"
          >
            + New Skill
          </button>
        </div>
      </div>

      {isLoading && <p className="text-zinc-600 text-sm">Loading skills...</p>}

      <div className="border border-zinc-800 rounded-lg overflow-hidden">
        <table className="w-full text-sm">
          <thead>
            <tr className="bg-zinc-900/50 text-zinc-500 text-xs uppercase tracking-wide">
              <th className="text-left px-4 py-2 font-medium">Name</th>
              <th className="text-left px-4 py-2 font-medium">Description</th>
              <th className="text-left px-4 py-2 font-medium">Source</th>
              <th className="text-right px-4 py-2 font-medium">Usage</th>
              <th className="text-right px-4 py-2 font-medium">Actions</th>
            </tr>
          </thead>
          <tbody>
            {skills.length === 0 && !isLoading && (
              <tr>
                <td colSpan={5} className="text-center py-8 text-zinc-600">No skills available</td>
              </tr>
            )}
            {skills.map((skill) => {
              const learnedData = learnedMap.get(skill.name);
              return (
                <tr key={skill.name} className="border-t border-zinc-800/50 hover:bg-zinc-900/30">
                  <td className="px-4 py-2 font-mono text-zinc-300">{skill.name}</td>
                  <td className="px-4 py-2 text-zinc-500 text-xs truncate max-w-xs">{skill.description}</td>
                  <td className="px-4 py-2"><SourceBadge source={skill.source} /></td>
                  <td className="px-4 py-2 text-right text-xs text-zinc-500">
                    {learnedData ? (
                      <span>
                        {learnedData.usage_count} / {(learnedData.success_rate * 100).toFixed(0)}%
                      </span>
                    ) : (
                      '-'
                    )}
                  </td>
                  <td className="px-4 py-2 text-right">
                    <button
                      onClick={() => setViewingSkill(skill.name)}
                      className="text-xs text-blue-400 hover:text-blue-300"
                    >
                      View
                    </button>
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>
    </div>
  );
}

export default function Admin() {
  const navigate = useNavigate();
  const [activeTab, setActiveTab] = useState<'audit' | 'retention' | 'skills'>('audit');
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
          {(['audit', 'retention', 'skills'] as const).map((tab) => (
            <button
              key={tab}
              onClick={() => setActiveTab(tab)}
              className={`px-4 py-2 text-sm capitalize transition-colors border-b-2 -mb-px ${
                activeTab === tab
                  ? 'border-white text-white'
                  : 'border-transparent text-zinc-500 hover:text-zinc-300'
              }`}
            >
              {tab === 'audit' ? 'Audit Log' : tab === 'retention' ? 'Retention' : 'Skills'}
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

        {activeTab === 'skills' && <SkillsTab />}

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
