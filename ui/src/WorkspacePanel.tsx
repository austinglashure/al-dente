// Undo and redo, and workspaces saved by name in this browser.

import { useState } from 'react';

interface Props {
  canUndo: boolean;
  canRedo: boolean;
  onUndo: () => void;
  onRedo: () => void;
  saved: string[];
  onSave: (name: string) => void;
  onLoad: (name: string) => void;
  onRemove: (name: string) => void;
}

/** The workspace section of the left panel.
 *  Example: `<WorkspacePanel canUndo canRedo={false} … />`. */
export function WorkspacePanel(p: Props) {
  const [name, setName] = useState('');
  return (
    <fieldset className="workspace" data-testid="workspace">
      <legend>Workspace</legend>
      <button type="button" data-testid="undo" disabled={!p.canUndo} onClick={p.onUndo} title="Undo (Ctrl+Z)">
        Undo
      </button>{' '}
      <button type="button" data-testid="redo" disabled={!p.canRedo} onClick={p.onRedo} title="Redo (Ctrl+Shift+Z)">
        Redo
      </button>
      <form
        onSubmit={(e) => {
          e.preventDefault();
          if (name.trim()) p.onSave(name.trim());
          setName('');
        }}
      >
        <input data-testid="workspace-name" value={name} onChange={(e) => setName(e.target.value)} placeholder="Name this view" aria-label="Workspace name" />{' '}
        <button type="submit" data-testid="workspace-save">
          Save
        </button>
      </form>
      {p.saved.length > 0 && (
        <ul className="saved" data-testid="saved-workspaces">
          {p.saved.map((n) => (
            <li key={n}>
              <button type="button" className="name" data-workspace={n} onClick={() => p.onLoad(n)}>
                {n}
              </button>{' '}
              <button type="button" className="small" onClick={() => p.onRemove(n)} aria-label={`Delete ${n}`}>
                ×
              </button>
            </li>
          ))}
        </ul>
      )}
    </fieldset>
  );
}
