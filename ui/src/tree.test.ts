import { describe, expect, it } from 'vitest';
import { allDirIds, dirChildren, dirMembers, findDir, matchDirs, topDirIds } from './tree';
import type { TreeDir } from './types';

const root: TreeDir = {
  id: null,
  path: '.',
  label: '.',
  files: [],
  dirs: [
    {
      id: 'dir:libs/common/src/common',
      path: 'libs/common/src/common',
      label: 'libs/common/src/common',
      dirs: [],
      files: [{ path: 'libs/common/src/common/util.clj', name: 'util.clj', namespaces: ['clj:libs/common//common.util'], status: 'analyzed' }],
    },
    {
      id: 'dir:services',
      path: 'services',
      label: 'services',
      files: [{ path: 'services/user.clj', name: 'user.clj', namespaces: [], status: 'not-analyzed' }],
      dirs: [{ id: 'dir:services/orders', path: 'services/orders', label: 'orders', dirs: [], files: [] }],
    },
  ],
};

describe('tree helpers', () => {
  it('finds rows and lists top-level and all directory ids', () => {
    expect(findDir(root, 'dir:services/orders')?.label).toBe('orders');
    expect(findDir(root, 'dir:nope')).toBeNull();
    expect(topDirIds(root)).toEqual(['dir:libs/common/src/common', 'dir:services']);
    expect(allDirIds(root)).toEqual(['dir:libs/common/src/common', 'dir:services', 'dir:services/orders']);
  });
  it("lists a directory's children: sub-directories, then analyzed files by namespace", () => {
    expect(dirChildren(root, 'dir:services')).toEqual([
      { id: 'dir:services/orders', name: 'orders/', kind: 'directory', subkind: null },
    ]);
    expect(dirChildren(root, 'dir:libs/common/src/common')).toEqual([
      { id: 'clj:libs/common//common.util', name: 'util.clj', kind: 'namespace', subkind: null },
    ]);
  });
});

describe('dirMembers and matchDirs', () => {
  it('lists every sub-directory and namespace inside a directory', () => {
    expect(dirMembers(root, 'dir:services')).toEqual(new Set(['dir:services/orders']));
    expect(dirMembers(root, 'dir:libs/common/src/common')).toEqual(new Set(['clj:libs/common//common.util']));
    expect(dirMembers(root, 'dir:nope')).toEqual(new Set());
  });

  it('finds directories by path, ignoring case, best match first', () => {
    const ids = (q: string) => matchDirs(root, q).map((n) => n.id);
    expect(ids('ORDERS')).toEqual(['dir:services/orders']);
    // A name match beats a match further up the path.
    expect(ids('common')).toEqual(['dir:libs/common/src/common']);
    expect(ids('serv')).toEqual(['dir:services', 'dir:services/orders']);
    expect(ids('zzz')).toEqual([]);
    expect(matchDirs(root, 'services')[0]).toMatchObject({ kind: 'directory', name: 'services' });
  });
});
