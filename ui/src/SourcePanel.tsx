// The selected node's source with a copy button.

import { useState } from 'react';

/** The selected node's source with a button that copies it. Keyed by the
 *  selection, so "Copied" resets when the selection changes.
 *  Example: `<SourcePanel source="(defn f [] 1)" />`. */
export function SourcePanel({ source }: { source: string }) {
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
  return (
    <section>
      <div className="details-head">
        <h2>Source</h2>
        <button type="button" data-testid="copy-source" onClick={copy} title="Copy the source to the clipboard">
          {copied ? 'Copied' : 'Copy'}
        </button>
      </div>
      <pre data-testid="source">{source}</pre>
    </section>
  );
}
