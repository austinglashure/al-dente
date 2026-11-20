// Facts about the analyzed repository loaded once at start: its units, how
// many namespaces it has, its blind spots and its libraries.

import { useEffect, useState } from 'react';
import type { Api } from './api';
import type { BlindSpot, LibrarySummary, UnitInfo } from './types';

/** Loads the repository facts once.
 *  Example: for mono-mini, `units` has `unit:services/orders`, `unit:libs/common`, … */
export function useRepoInfo(api: Api, fail: (e: unknown) => void) {
  const [units, setUnits] = useState<Map<string, UnitInfo>>(new Map());
  const [namespaceCount, setNamespaceCount] = useState<number | null>(null);
  const [spots, setSpots] = useState<BlindSpot[]>([]);
  const [libs, setLibs] = useState<LibrarySummary[]>([]);
  useEffect(() => {
    api
      .snapshot()
      .then((s) => {
        setUnits(new Map(s.units.map((u) => [u.id, u])));
        setNamespaceCount(s.summary.nodes?.namespace ?? null);
      })
      .catch(fail);
    api
      .blindspots()
      .then((r) => setSpots(r.blindspots))
      .catch(fail);
    api
      .libs()
      .then((r) => setLibs(r.libraries))
      .catch(fail);
  }, [api, fail]);
  return { units, namespaceCount, spots, libs };
}
