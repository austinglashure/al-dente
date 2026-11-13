// Paste a stack trace: normalize it, show its frames with noise folded,
// and pin a crater to it. The trace text is only ever sent to the local
// API and rendered as React text.

import { useState } from 'react';
import type { Api } from './api';
import type { Frame, Trace, TraceReport } from './types';

interface Props {
  api: Api;
  onPin: (text: string) => void;
  onClose: () => void;
}

/** The dialog.
 *  Example: `<TraceDialog api={api} onPin={(t) => openCrater(t)} onClose={close} />`. */
export function TraceDialog({ api, onPin, onClose }: Props) {
  const [text, setText] = useState('');
  const [report, setReport] = useState<TraceReport | null>(null);
  const [error, setError] = useState<string | null>(null);
  const normalize = () => {
    setError(null);
    api
      .normalizeTrace(text)
      .then(setReport)
      .catch((e: unknown) => {
        setReport(null);
        setError(e instanceof Error ? e.message : String(e));
      });
  };
  return (
    <div className="modal-backdrop" role="presentation" onClick={onClose}>
      <div className="modal" role="dialog" aria-modal="true" aria-label="Paste a stack trace" data-testid="trace-dialog" onClick={(e) => e.stopPropagation()}>
        <div className="details-head">
          <h2>Paste a stack trace</h2>
          <button type="button" className="close" onClick={onClose} aria-label="Close" data-testid="trace-close">
            ×
          </button>
        </div>
        <p className="muted small-print">JVM (printStackTrace, pst, logs), Clojure data (Throwable-&gt;map, #error) or ClojureScript (Chrome, Firefox, Safari, Node).</p>
        <textarea data-testid="trace-input" value={text} onChange={(e) => setText(e.target.value)} rows={10} spellCheck={false} />
        <div className="tools">
          <button type="button" data-testid="trace-normalize" disabled={!text.trim()} onClick={normalize}>
            Normalize
          </button>
          <button type="button" data-testid="trace-pin" disabled={!text.trim()} onClick={() => onPin(text)}>
            Pin the crater
          </button>
        </div>
        {error && (
          <p className="error" role="alert">
            {error}
          </p>
        )}
        {report && (
          <div data-testid="trace-frames">
            {report.traces.map((t, i) => (
              <TraceBlock key={i} t={t} depth={0} />
            ))}
          </div>
        )}
      </div>
    </div>
  );
}

/** One trace and its causes, noise runs folded into one expandable line.
 *  Example: `… 3 noise frames` between two repository frames. */
function TraceBlock({ t, depth }: { t: Trace; depth: number }) {
  const [open, setOpen] = useState(false);
  const runs: (Frame | Frame[])[] = [];
  for (const f of t.frames) {
    const last = runs[runs.length - 1];
    if (f.noise && !open) {
      if (Array.isArray(last)) last.push(f);
      else runs.push([f]);
    } else {
      runs.push(f);
    }
  }
  const head = [t.exception.type, t.exception.message].filter(Boolean).join(': ') || '(no exception line)';
  return (
    <div className="trace" style={{ marginLeft: depth * 12 }}>
      <p className="mono">{depth > 0 ? `caused by: ${head}` : head}</p>
      <ul>
        {runs.map((r, i) =>
          Array.isArray(r) ? (
            <li key={`n${i}`}>
              <button type="button" className="small" data-testid="noise-fold" onClick={() => setOpen(true)}>
                … {r.length} noise frame{r.length === 1 ? '' : 's'}
              </button>
            </li>
          ) : (
            <li key={r.index} className={r.noise ? 'muted' : ''} data-testid="frame" data-node-id={r.node ?? undefined} data-match={r.match}>
              <span className="mono">
                {r.namespace ?? '?'}/{r.name ?? '?'}
                {r.anonymous ? ' (anon)' : ''} {r.file ?? ''}
                {r.line !== null ? `:${r.line}` : ''}
              </span>
              {r.node && <span className="muted"> → {r.node} ({r.match})</span>}
            </li>
          ),
        )}
      </ul>
      {t.causes.map((c, i) => (
        <TraceBlock key={i} t={c} depth={depth + 1} />
      ))}
    </div>
  );
}
