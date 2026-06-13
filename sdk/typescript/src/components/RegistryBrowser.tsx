import type { ToolSchema } from '../api/schema';

interface RegistryBrowserProps {
  schemas: ToolSchema[];
  onSelect: (schema: ToolSchema) => void;
}

/** Grid of registry schema cards — click to load into the explorer */
export default function RegistryBrowser({ schemas, onSelect }: RegistryBrowserProps) {
  if (schemas.length === 0) {
    return (
      <div className="empty-state">
        <div className="empty-state__icon">📦</div>
        <div className="empty-state__title">No registry schemas found</div>
        <div className="empty-state__text">
          Add <code>.schema.json</code> files to the <code>registry/</code> directory
          or use <code>agentool wrap &lt;URL&gt;</code> to generate one.
        </div>
      </div>
    );
  }

  return (
    <div className="registry-grid fade-in">
      {schemas.map((schema) => (
        <div
          key={schema.tool_id}
          className="registry-card"
          onClick={() => onSelect(schema)}
        >
          <div className="registry-card__header">
            <div className="sidebar__schema-icon">
              {schema.tool_id.slice(0, 2)}
            </div>
            <div>
              <div className="registry-card__name">{schema.tool_id}</div>
              <div className="registry-card__url">{schema.base_url}</div>
            </div>
          </div>
          <div className="registry-card__methods">
            <strong>{schema.methods.length}</strong> method{schema.methods.length !== 1 ? 's' : ''}
            {schema.auth && schema.auth.auth_type !== 'none' && (
              <span style={{ marginLeft: 8, fontSize: '0.75rem', color: 'var(--warning)' }}>
                🔐 {schema.auth.auth_type}
              </span>
            )}
          </div>
          {schema.provenance && (
            <div style={{ fontSize: '0.7rem', color: 'var(--text-tertiary)' }}>
              Source: {schema.provenance.source}
            </div>
          )}
        </div>
      ))}
    </div>
  );
}
