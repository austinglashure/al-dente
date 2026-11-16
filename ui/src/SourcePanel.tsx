// Source text with line numbers, highlighting and a copy button. Every
// repository string is rendered as React text.

import { useState } from 'react';
import { highlight } from './highlight';

/** Source starting at line `first`; `mark` highlights one line (a call site).
 *  Keyed by the selection, so "Copied" resets when it changes.
 *  Example: `<SourcePanel title="Source" source="(defn f [] 1)" first={12} />`. */
export function SourcePanel({ source, first = 1, mark, title = 'Source', testId = 'source' }: { source: string; first?: number; mark?: number; title?: string; testId?: string }) {
  const [copied, setCopied] = useState(false);
  const copy = () => {
    void navigator.clipboard
      ?.writeText(source)
      .then(() => {
        setCopied(true);
        window.setTimeout(() => setCopied(false), 1500);
      })
      .catch(() => setCopied(false));
  };
  const lines = highlight(source.replace(/\n$/, ''));
  return (
    <section>
      <div className="details-head">
        <h2>{title}</h2>
        <button type="button" data-testid={`copy-${testId}`} onClick={copy} title="Copy the source to the clipboard">
          {copied ? 'Copied' : 'Copy'}
        </button>
      </div>
      <pre className="source" data-testid={testId}>
        {lines.map((tokens, i) => (
          <div key={i} className={first + i === mark ? 'line marked' : 'line'} data-line={first + i}>
            <span className="ln">{first + i}</span>
            {tokens.map((t, j) => (
              <span key={j} className={`tok-${t.kind}`}>
                {t.text}
              </span>
            ))}
            {'\n'}
          </div>
        ))}
      </pre>
    </section>
  );
}
