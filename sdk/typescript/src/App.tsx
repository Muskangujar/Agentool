import { useState } from 'react';
import SchemaExplorer from './components/SchemaExplorer';

type Tab = 'schemas';

export default function App() {
  const [activeTab] = useState<Tab>('schemas');

  return (
    <div className="app">
      {/* ── Top Bar ──────────────────────────────────────────────── */}
      <header className="topbar">
        <div className="topbar__brand">
          <div className="topbar__logo">A</div>
          <span>Agentool</span>
          <span className="topbar__badge">v0.1.0</span>
        </div>

        <nav className="topbar__tabs">
          <button
            className={`topbar__tab ${activeTab === 'schemas' ? 'topbar__tab--active' : ''}`}
          >
            Schema Explorer
          </button>
          {/* Future tabs: MCP Servers, Recordings, Settings */}
        </nav>

        <div style={{ display: 'flex', alignItems: 'center', gap: 12 }}>
          <a
            href="https://github.com/samvardhan/agentool"
            target="_blank"
            rel="noopener noreferrer"
            style={{
              color: 'var(--text-tertiary)',
              fontSize: '0.8rem',
              textDecoration: 'none',
              transition: 'color 150ms',
            }}
            onMouseEnter={(e) => (e.currentTarget.style.color = 'var(--text-primary)')}
            onMouseLeave={(e) => (e.currentTarget.style.color = 'var(--text-tertiary)')}
          >
            GitHub ↗
          </a>
        </div>
      </header>

      {/* ── Tab Content ──────────────────────────────────────────── */}
      <div style={{ flex: 1, display: 'flex', overflow: 'hidden' }}>
        {activeTab === 'schemas' && <SchemaExplorer />}
      </div>
    </div>
  );
}
