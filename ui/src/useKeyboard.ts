// Keyboard navigation (06-gui): ← caller, → callee (strongest first), e
// expand or collapse, h hide, f focus, c crater, / search. Not while typing.

import { useEffect } from 'react';
import { isAggregate } from './model';
import type { GNode, Neighbor, ShowResult } from './types';
import type { Op } from './workspace';

interface Keys {
  /** What is selected, or null. */
  details: ShowResult | null;
  follow: (id: string) => void;
  run: (op: Op) => Promise<void>;
  /** Whether an aggregate is expanded in place now. */
  isExpanded: (id: string) => boolean;
  deselect: () => void;
  openCrater: (n: GNode, direction: 'forward' | 'reverse') => void;
}

/** Installs the shortcuts on the window for as long as the component lives.
 *  Example: with `shop.core/checkout` selected, → follows its strongest callee. */
export function useKeyboard({ details, follow, run, isExpanded, deselect, openCrater }: Keys): void {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const typing = e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement || e.target instanceof HTMLSelectElement;
      if (typing || e.ctrlKey || e.metaKey || e.altKey) return;
      if (e.key === '/') {
        e.preventDefault();
        document.querySelector<HTMLInputElement>('[data-testid="search"]')?.focus();
        return;
      }
      const d = details;
      if (!d) return;
      const id = d.node.id;
      const go = (list: Neighbor[]) => {
        const next = list[0];
        if (next) follow(next.id);
      };
      switch (e.key) {
        case 'ArrowLeft':
          go(d.callers);
          break;
        case 'ArrowRight':
          go(d.callees);
          break;
        case 'e':
          if (isAggregate(d.node)) void run({ op: isExpanded(id) ? 'collapse' : 'expand', id });
          break;
        case 'h':
          deselect();
          void run({ op: 'hide', ids: [id] });
          break;
        case 'f':
          void run({ op: 'focus', id });
          break;
        case 'c':
          openCrater(d.node, 'forward');
          break;
        default:
          return;
      }
      e.preventDefault();
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [details, follow, run, isExpanded, deselect, openCrater]);
}
