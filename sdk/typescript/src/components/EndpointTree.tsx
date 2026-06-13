import { useState } from 'react';
import type { SchemaMethod } from '../api/schema';

interface EndpointTreeProps {
  methods: SchemaMethod[];
  selectedMethod: string | null;
  onSelect: (name: string) => void;
}

/** Group methods by HTTP verb and render a collapsible tree */
export default function EndpointTree({ methods, selectedMethod, onSelect }: EndpointTreeProps) {
  // Group by HTTP method
  const groups = new Map<string, SchemaMethod[]>();
  for (const m of methods) {
    const verb = m.http.method.toUpperCase();
    if (!groups.has(verb)) groups.set(verb, []);
    groups.get(verb)!.push(m);
  }

  // Sort groups: GET first, then POST, PUT, PATCH, DELETE
  const order = ['GET', 'POST', 'PUT', 'PATCH', 'DELETE'];
  const sorted = [...groups.entries()].sort(
    (a, b) => order.indexOf(a[0]) - order.indexOf(b[0])
  );

  return (
    <ul className="tree">
      {sorted.map(([verb, items]) => (
        <TreeGroup
          key={verb}
          verb={verb}
          methods={items}
          selectedMethod={selectedMethod}
          onSelect={onSelect}
        />
      ))}
    </ul>
  );
}

// ── Collapsible group ──────────────────────────────────────────────────────

function TreeGroup({
  verb,
  methods,
  selectedMethod,
  onSelect,
}: {
  verb: string;
  methods: SchemaMethod[];
  selectedMethod: string | null;
  onSelect: (name: string) => void;
}) {
  const [open, setOpen] = useState(true);

  return (
    <li className="tree__group">
      <div className="tree__group-header" onClick={() => setOpen(!open)}>
        <span className={`tree__chevron ${open ? 'tree__chevron--open' : ''}`}>▶</span>
        <HttpBadge method={verb} />
        <span>{verb}</span>
        <span style={{ marginLeft: 'auto', opacity: 0.4, fontSize: '0.75rem' }}>
          {methods.length}
        </span>
      </div>
      {open && (
        <ul className="tree__items fade-in">
          {methods.map((m) => (
            <li
              key={m.name}
              className={`tree__item ${selectedMethod === m.name ? 'tree__item--active' : ''}`}
              onClick={() => onSelect(m.name)}
            >
              <span className="tree__method-name">{m.name}</span>
              <span className="tree__method-path">{m.http.path}</span>
            </li>
          ))}
        </ul>
      )}
    </li>
  );
}

// ── HTTP badge ─────────────────────────────────────────────────────────────

export function HttpBadge({ method }: { method: string }) {
  const cls = `badge badge--${method.toLowerCase()}`;
  return <span className={cls}>{method}</span>;
}
