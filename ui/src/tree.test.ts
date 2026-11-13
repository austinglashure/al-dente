import { describe, expect, it } from 'vitest';
import { allDirIds, dirChildren, findDir, topDirIds } from './tree';
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
