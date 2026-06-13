import type { SchemaMethod } from '../api/schema';
import { HttpBadge } from './EndpointTree';

interface EndpointDetailProps {
  method: SchemaMethod;
}

/** Shows full detail for a selected endpoint: description, params table, return type */
export default function EndpointDetail({ method }: EndpointDetailProps) {
  return (
    <div className="detail fade-in">
      {/* Header: badge + name + path */}
      <div className="detail__header">
        <HttpBadge method={method.http.method} />
        <span className="detail__name">{method.name}</span>
      </div>

      {/* Path with highlighted params */}
      <div className="detail__path">
        <HighlightedPath path={method.http.path} />
      </div>

      {/* Description */}
      {method.description && (
        <p className="detail__description">{method.description}</p>
      )}

      {/* Params table */}
      {method.params.length > 0 && (
        <div className="card">
          <div className="card__title">
            <span className="card__title-icon">⚙</span>
            Parameters
          </div>
          <table className="params-table">
            <thead>
              <tr>
                <th>Name</th>
                <th>Type</th>
                <th>Required</th>
                <th>Location</th>
                <th>Description</th>
              </tr>
            </thead>
            <tbody>
              {method.params.map((p) => (
                <tr key={p.name}>
                  <td><span className="param-name">{p.name}</span></td>
                  <td><span className="param-type">{p.type}</span></td>
                  <td>
                    <span className={`param-required param-required--${p.required}`}>
                      {p.required ? 'required' : 'optional'}
                    </span>
                  </td>
                  <td><span className="param-location">{p.location || '—'}</span></td>
                  <td style={{ color: 'var(--text-secondary)', fontSize: '0.8rem' }}>
                    {p.description || '—'}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}

      {/* Return type */}
      {method.returns && (
        <div className="card">
          <div className="card__title">
            <span className="card__title-icon">↩</span>
            Response
          </div>
          <p style={{ color: 'var(--text-secondary)', fontSize: '0.85rem' }}>
            {typeof method.returns === 'string'
              ? method.returns
              : `${method.returns.type}${method.returns.description ? ' — ' + method.returns.description : ''}`
            }
          </p>
        </div>
      )}
    </div>
  );
}

// ── Path with highlighted {params} ─────────────────────────────────────────

function HighlightedPath({ path }: { path: string }) {
  const parts = path.split(/(\{[^}]+\})/g);
  return (
    <>
      {parts.map((part, i) =>
        part.startsWith('{') ? (
          <span key={i} className="detail__path-param">{part}</span>
        ) : (
          <span key={i}>{part}</span>
        )
      )}
    </>
  );
}
