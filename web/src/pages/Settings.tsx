import { useState } from 'react';
import { useAuthStore } from '../stores/auth';
import { useNavigate } from 'react-router-dom';

export default function Settings() {
  const user = useAuthStore((s) => s.user);
  const logout = useAuthStore((s) => s.logout);
  const navigate = useNavigate();
  const [activeTab, setActiveTab] = useState<'profile' | 'preferences'>('profile');

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
          <h1 className="text-lg font-medium">Settings</h1>
        </div>
        <button
          onClick={logout}
          className="text-zinc-500 text-sm hover:text-zinc-300 transition-colors"
        >
          Sign out
        </button>
      </header>

      <div className="max-w-2xl mx-auto py-8 px-4">
        {/* Tabs */}
        <div className="flex gap-1 mb-8 border-b border-zinc-800">
          {(['profile', 'preferences'] as const).map((tab) => (
            <button
              key={tab}
              onClick={() => setActiveTab(tab)}
              className={`px-4 py-2 text-sm capitalize transition-colors border-b-2 -mb-px ${
                activeTab === tab
                  ? 'border-white text-white'
                  : 'border-transparent text-zinc-500 hover:text-zinc-300'
              }`}
            >
              {tab}
            </button>
          ))}
        </div>

        {activeTab === 'profile' && (
          <div className="space-y-6">
            <div>
              <label className="block text-sm text-zinc-400 mb-1">Email</label>
              <div className="bg-zinc-900 border border-zinc-800 rounded-lg px-4 py-2.5 text-sm text-zinc-300">
                {user?.email ?? '-'}
              </div>
            </div>
            <div>
              <label className="block text-sm text-zinc-400 mb-1">Display name</label>
              <div className="bg-zinc-900 border border-zinc-800 rounded-lg px-4 py-2.5 text-sm text-zinc-300">
                {user?.display_name ?? user?.email ?? '-'}
              </div>
            </div>
            <div>
              <label className="block text-sm text-zinc-400 mb-1">Role</label>
              <div className="bg-zinc-900 border border-zinc-800 rounded-lg px-4 py-2.5 text-sm text-zinc-300 capitalize">
                {user?.role ?? '-'}
              </div>
            </div>
          </div>
        )}

        {activeTab === 'preferences' && (
          <div className="space-y-6">
            <div>
              <label className="block text-sm text-zinc-400 mb-1">Theme</label>
              <div className="bg-zinc-900 border border-zinc-800 rounded-lg px-4 py-2.5 text-sm text-zinc-500">
                Dark (only theme available)
              </div>
            </div>
            <div>
              <label className="block text-sm text-zinc-400 mb-1">Default model</label>
              <div className="bg-zinc-900 border border-zinc-800 rounded-lg px-4 py-2.5 text-sm text-zinc-500">
                Configured in server config.yaml
              </div>
            </div>
            <p className="text-xs text-zinc-600 mt-8">
              More settings will be available in future updates.
            </p>
          </div>
        )}
      </div>
    </div>
  );
}
