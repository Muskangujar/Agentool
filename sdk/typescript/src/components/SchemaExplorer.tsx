import { useState, useEffect, useCallback } from 'react';
import type { ToolSchema } from '../api/schema';
import { fetchRegistrySchemas } from '../api/schema';
import EndpointTree from './EndpointTree';
import EndpointDetail from './EndpointDetail';
import CallTester from './CallTester';
import RegistryBrowser from './RegistryBrowser';

/**
 * SchemaExplorer — The main dashboard tab.
 * Left sidebar: schema list + endpoint tree.
 * Right content: endpoint detail + call tester.
 */
export default function SchemaExplorer() {
  const [schemas, setSchemas] = useState<ToolSchema[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const [activeSchema, setActiveSchema] = useState<ToolSchema | null>(null);
  const [selectedMethod, setSelectedMethod] = useState<string | null>(null);
  const [searchFilter, setSearchFilter] = useState('');

  // Load registry schemas on mount
  useEffect(() => {
    fetchRegistrySchemas()
      .then((data) => {
        setSchemas(data);
        setLoading(false);
      })
      .catch((err) => {
        setError(err.message);
        setLoading(false);
      });
  }, []);

  const handleSelectSchema = useCallback((schema: ToolSchema) => {
    setActiveSchema(schema);
    setSelectedMethod(null);
  }, []);

  const handleSelectMethod = useCallback((name: string) => {
    setSelectedMethod(name);
  }, []);

  // Get the selected method object
  const currentMethod = activeSchema?.methods.find((m) => m.name === selectedMethod) ?? null;

  // Filter methods by search
  const filteredMethods = activeSchema
    ? activeSchema.methods.filter(
        (m) =>
          m.name.toLowerCase().includes(searchFilter.toLowerCase()) ||
          m.http.path.toLowerCase().includes(searchFilter.toLowerCase())
      )
    : [];

  // ── Loading / Error states ──────────────────────────────────────────────
  if (loading) {
    return (
      <div className="empty-state">
        <div className="spinner" />
        <div className="empty-state__title">Loading schemas…</div>
      </div>
    );
  }

  if (error) {
    return (
      <div className="empty-state">
        <div className="empty-state__icon">⚠️</div>
        <div className="empty-state__title">Failed to load schemas</div>
        <div className="empty-state__text">{error}</div>
      </div>
    );
  }

  // ── No schema selected → show registry browser ──────────────────────────
  if (!activeSchema) {
    return (
      <div className="content" style={{ padding: 32 }}>
        <div style={{ marginBottom: 8 }}>
          <h2 style={{ fontSize: '1.3rem', fontWeight: 700, letterSpacing: '-0.02em' }}>
            📚 Schema Registry
          </h2>
          <p style={{ color: 'var(--text-secondary)', fontSize: '0.85rem', marginTop: 6 }}>
            Select a schema to explore its endpoints, parameters, and test API calls.
          </p>
        </div>
        <RegistryBrowser schemas={schemas} onSelect={handleSelectSchema} />
      </div>
    );
  }

  // ── Schema selected → show explorer layout ──────────────────────────────
  return (
    <div className="explorer">
      {/* Sidebar: schema picker + endpoint tree */}
      <aside className="sidebar">
        {/* Schema list at top */}
        <div className="sidebar__header">
          <div style={{ display: 'flex', gap: 6, marginBottom: 8 }}>
            {schemas.map((s) => (
              <div
                key={s.tool_id}
                className={`sidebar__schema-item ${activeSchema.tool_id === s.tool_id ? 'sidebar__schema-item--active' : ''}`}
                onClick={() => handleSelectSchema(s)}
                style={{ padding: '6px 10px', fontSize: '0.8rem' }}
              >
                <span className="sidebar__schema-icon" style={{ width: 24, height: 24, fontSize: '0.6rem' }}>
                  {s.tool_id.slice(0, 2)}
                </span>
                <span className="sidebar__schema-name" style={{ fontSize: '0.8rem' }}>{s.tool_id}</span>
              </div>
            ))}
          </div>
          <input
            className="sidebar__search"
            placeholder="Filter methods…"
            value={searchFilter}
            onChange={(e) => setSearchFilter(e.target.value)}
          />
        </div>

        {/* Endpoint tree */}
        <div className="sidebar__list">
          <EndpointTree
            methods={filteredMethods}
            selectedMethod={selectedMethod}
            onSelect={handleSelectMethod}
          />
        </div>

        {/* Back button */}
        <div style={{ padding: 12, borderTop: '1px solid var(--border-subtle)' }}>
          <button
            onClick={() => { setActiveSchema(null); setSelectedMethod(null); }}
            style={{
              width: '100%',
              padding: '8px',
              borderRadius: 'var(--radius-sm)',
              border: '1px solid var(--border-subtle)',
              background: 'transparent',
              color: 'var(--text-secondary)',
              fontSize: '0.8rem',
              cursor: 'pointer',
            }}
          >
            ← Back to Registry
          </button>
        </div>
      </aside>

      {/* Content: detail + tester */}
      <main className="content">
        {currentMethod ? (
          <>
            <EndpointDetail method={currentMethod} />
            <CallTester method={currentMethod} baseUrl={activeSchema.base_url} />
          </>
        ) : (
          <div className="empty-state">
            <div className="empty-state__icon">🔍</div>
            <div className="empty-state__title">Select an endpoint</div>
            <div className="empty-state__text">
              Choose a method from the tree on the left to view its parameters
              and test it live.
            </div>
          </div>
        )}
      </main>
    </div>
  );
}
