import { useState } from 'react';
import type { SchemaMethod } from '../api/schema';
import { callMethod } from '../api/schema';

interface CallTesterProps {
  method: SchemaMethod;
  baseUrl: string;
}

/** Interactive API tester — auto-generates form fields from params, sends request, shows response */
export default function CallTester({ method, baseUrl }: CallTesterProps) {
  const [params, setParams] = useState<Record<string, string>>({});
  const [authToken, setAuthToken] = useState('');
  const [response, setResponse] = useState<{ status: number; body: string; duration: number } | null>(null);
  const [loading, setLoading] = useState(false);

  const updateParam = (name: string, value: string) => {
    setParams((prev) => ({ ...prev, [name]: value }));
  };

  const handleSend = async () => {
    setLoading(true);
    setResponse(null);
    try {
      const result = await callMethod(baseUrl, method, params, authToken || undefined);
      setResponse(result);
    } catch (err) {
      setResponse({
        status: 0,
        body: `Error: ${err instanceof Error ? err.message : String(err)}`,
        duration: 0,
      });
    } finally {
      setLoading(false);
    }
  };

  const isError = response && (response.status === 0 || response.status >= 400);

  return (
    <div className="card fade-in">
      <div className="card__title">
        <span className="card__title-icon">▶</span>
        Test Call
      </div>

      <div className="tester">
        {/* Auth token (optional) */}
        <div className="tester__field">
          <label className="tester__label">
            🔑 Bearer Token
            <span style={{ color: 'var(--text-tertiary)', fontWeight: 400 }}>(optional)</span>
          </label>
          <input
            className="tester__input"
            type="password"
            placeholder="ghp_xxxx..."
            value={authToken}
            onChange={(e) => setAuthToken(e.target.value)}
          />
        </div>

        {/* Auto-generated param fields */}
        {method.params.map((p) => (
          <div key={p.name} className="tester__field">
            <label className="tester__label">
              <span className="param-name" style={{ fontSize: '0.8rem' }}>{p.name}</span>
              <span className="param-type" style={{ fontSize: '0.7rem' }}>{p.type}</span>
              {p.required && (
                <span className="param-required param-required--true" style={{ fontSize: '0.65rem' }}>
                  required
                </span>
              )}
            </label>
            <input
              className="tester__input"
              placeholder={p.description || `Enter ${p.name}...`}
              value={params[p.name] || ''}
              onChange={(e) => updateParam(p.name, e.target.value)}
            />
          </div>
        ))}

        {/* Send button */}
        <div className="tester__actions">
          <button
            className="tester__send"
            onClick={handleSend}
            disabled={loading}
          >
            {loading ? (
              <span style={{ display: 'flex', alignItems: 'center', gap: 6 }}>
                <span className="spinner" style={{ width: 14, height: 14, borderWidth: 2 }} />
                Sending…
              </span>
            ) : (
              `Send ${method.http.method}`
            )}
          </button>

          {response && (
            <span style={{
              fontSize: '0.8rem',
              color: isError ? 'var(--error)' : 'var(--success)',
              fontFamily: 'var(--font-mono)',
            }}>
              {response.status > 0 ? `${response.status}` : 'ERR'} · {response.duration.toFixed(0)}ms
            </span>
          )}
        </div>

        {/* Response output */}
        {response && (
          <div className={`tester__response ${isError ? 'tester__response--error' : ''}`}>
            {response.body}
          </div>
        )}
      </div>
    </div>
  );
}
