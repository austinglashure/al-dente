// The filter panel: units, kinds, flags, a name pattern and measure
// minimums. Each change is one workspace step (undoable, in the URL).

import { useState } from 'react';
import { active, NO_FILTERS, type Filters } from './filters';
import { METRICS } from './overlay';
import type { UnitInfo } from './types';

const KINDS = ['unit', 'namespace', 'fn', 'macro', 'var', 'multimethod', 'protocol', 'record', 'impl', 'test', 'external', 'platform', 'hole'];
const FLAGS = ['entry-point', 'test', 'private', 'deprecated', 'stateful', 'dynamic', 'service'];

/** Adds or removes `value` from a list.
 *  Example: `toggled(['a'], 'b', true)` is `['a', 'b']`. */
function toggled(list: string[], value: string, on: boolean): string[] {
  return on ? [...list.filter((v) => v !== value), value] : list.filter((v) => v !== value);
}

/** The filter panel.
 *  Example: `<FilterPanel filters={f} units={units} onChange={(f) => run({ op: 'filters', filters: f })} />`. */
export function FilterPanel({ filters, units, onChange }: { filters: Filters; units: UnitInfo[]; onChange: (f: Filters) => void }) {
  const [name, setName] = useState(filters.name);
  const [metric, setMetric] = useState<string>(METRICS[0]);
  const [min, setMin] = useState('1');
  const [prevName, setPrevName] = useState(filters.name);
  // Undo or a loaded workspace can change the pattern from outside.
  if (filters.name !== prevName) {
    setPrevName(filters.name);
    setName(filters.name);
  }
  const checks = (title: string, testId: string, all: string[], chosen: string[], key: 'units' | 'kinds' | 'flags', label: (v: string) => string = (v) => v) => (
    <fieldset className="filter-group" data-testid={testId}>
      <legend>{title}</legend>
      {all.map((v) => (
        <label key={v}>
          <input type="checkbox" data-value={v} checked={chosen.includes(v)} onChange={(e) => onChange({ ...filters, [key]: toggled(chosen, v, e.target.checked) })} /> {label(v)}
        </label>
      ))}
    </fieldset>
  );
  return (
    <details className="filters" data-testid="filters" open={active(filters)}>
      <summary>Filters{active(filters) ? ' (on)' : ''}</summary>
      <label>
        Name contains{' '}
        <input
          data-testid="filter-name"
          value={name}
          onChange={(e) => setName(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === 'Enter' && name !== filters.name) onChange({ ...filters, name });
          }}
          onBlur={() => {
            if (name !== filters.name) onChange({ ...filters, name });
          }}
          placeholder="part of a name or ID"
        />
      </label>
      {units.length > 1 && checks('Units', 'filter-units', units.map((u) => u.id), filters.units, 'units', (id) => id.replace(/^unit:/, ''))}
      {checks('Kinds', 'filter-kinds', KINDS, filters.kinds, 'kinds')}
      {checks('Flags (all of)', 'filter-flags', FLAGS, filters.flags, 'flags')}
      <fieldset className="filter-group" data-testid="filter-measures">
        <legend>Measures (at least)</legend>
        {filters.measures.map((m, i) => (
          <div key={`${m.metric}-${i}`}>
            {m.metric.replace(/_/g, ' ')} ≥ {m.min}{' '}
            <button type="button" className="small" onClick={() => onChange({ ...filters, measures: filters.measures.filter((_, j) => j !== i) })} aria-label="Remove">
              ×
            </button>
          </div>
        ))}
        <select data-testid="filter-metric" value={metric} onChange={(e) => setMetric(e.target.value)}>
          {METRICS.map((m) => (
            <option key={m} value={m}>
              {m.replace(/_/g, ' ')}
            </option>
          ))}
        </select>{' '}
        <input data-testid="filter-min" type="number" value={min} onChange={(e) => setMin(e.target.value)} style={{ width: '5em' }} />{' '}
        <button type="button" data-testid="filter-add-measure" onClick={() => Number.isFinite(Number(min)) && onChange({ ...filters, measures: [...filters.measures, { metric, min: Number(min) }] })}>
          Add
        </button>
      </fieldset>
      {active(filters) && (
        <button type="button" data-testid="filter-clear" onClick={() => onChange(NO_FILTERS)}>
          Clear filters
        </button>
      )}
    </details>
  );
}
